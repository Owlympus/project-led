//! Matter: makes the lamp a light in the iPhone Home app (ADR-0004, ADR-0009).
//!
//! For now (step 4a) the lamp is an **On/Off light**: the Home app can only
//! switch it on and off. Brightness and colors come in step 4b.
//!
//! What the Matter stack (`rs-matter-embassy`) does for us:
//! - at first start, it advertises the lamp over **Bluetooth** and prints a
//!   pairing QR code in the logs;
//! - during pairing, the iPhone sends it the **Wi-Fi** name and password, and
//!   the stack joins the network by itself (no more `WIFI_SSID`, ADR-0008 is
//!   superseded);
//! - then it answers the Apple TV's commands over Wi-Fi.
//!
//! Our part is the [`Light`] type below: the stack calls it when the Home app
//! switches the lamp, and it forwards the request to the strip task through
//! [`LIGHT_ON`].
//!
//! Limits of this first version:
//! - test certificates: the Home app warns that the lamp is not certified;
//! - nothing is saved in flash yet (step 4c): after a restart, the lamp must
//!   be removed from the Home app and paired again.

use core::pin::pin;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_hal::peripherals::{ADC1, BT, RNG, WIFI};
use esp_hal::rng::{Trng, TrngSource};
use log::info;
use rs_matter_embassy::matter::basic_info::BasicInfoConfig;
use rs_matter_embassy::matter::crypto::{Crypto, default_crypto};
use rs_matter_embassy::matter::dm::clusters::app::on_off::{self, EffectVariantEnum, OnOffHooks};
use rs_matter_embassy::matter::dm::clusters::decl::on_off as on_off_cluster;
use rs_matter_embassy::matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter_embassy::matter::dm::devices::DEV_TYPE_ON_OFF_LIGHT;
use rs_matter_embassy::matter::dm::devices::test::{
    DAC_PRIVKEY, TEST_DEV_ATT, TEST_DEV_COMM, TEST_DEV_DET,
};
use rs_matter_embassy::matter::dm::endpoints::ROOT_ENDPOINT_ID;
use rs_matter_embassy::matter::dm::{Async, Cluster, Dataver, EmptyHandler, Endpoint, Node};
use rs_matter_embassy::matter::persist::{DummyKvBlobStore, VENDOR_KEYS_START};
use rs_matter_embassy::matter::utils::init::InitMaybeUninit;
use rs_matter_embassy::matter::{clusters, devices, with};
use rs_matter_embassy::rand::reseeding_csprng;
use rs_matter_embassy::stack::MatterStack;
use rs_matter_embassy::stack::wireless::BTP_BUMP_SIZE;
use rs_matter_embassy::wireless::esp::EspWifiRadio;
use rs_matter_embassy::wireless::{EmbassyWifi, EmbassyWifiNetwork};
use static_cell::StaticCell;

/// Whether the Home app wants the lamp on. A `Signal` holds only the latest
/// value: if the app toggles quickly, the strip task just sees the last one.
/// (`static` + `Signal` is how two tasks share a value safely, a bit like a
/// tiny message queue.)
pub static LIGHT_ON: Signal<CriticalSectionRawMutex, bool> = Signal::new();

/// Memory for the stack's internal futures, on top of the stack itself.
/// Value from the `rs-matter-embassy` examples: if the stack panics at
/// startup because it is too small, increase it.
const BUMP_SIZE: usize = 20_000 + BTP_BUMP_SIZE;

/// What the lamp tells the Home app about itself. The test vendor and product
/// ids (from `TEST_DEV_DET`) must stay: they match the test certificates.
const DEVICE_INFO: BasicInfoConfig = BasicInfoConfig {
    vendor_name: "project-led",
    product_name: "Lampe barre",
    device_name: "Lampe barre",
    ..TEST_DEV_DET
};

/// Endpoint 0 is reserved for Matter's own system clusters: the light is on
/// endpoint 1.
const LIGHT_ENDPOINT_ID: u16 = 1;

/// The whole Matter stack (~35 to 50 KB): too big for a task's stack, so it
/// lives in a `static`, initialized at run time by [`run`].
static STACK: StaticCell<MatterStack<BUMP_SIZE, EmbassyWifiNetwork>> = StaticCell::new();

/// Starts Matter and runs it forever. Call it last in `main`: it never
/// returns (unless the stack fails, which is a bug: we panic then).
#[allow(
    clippy::large_stack_frames,
    reason = "runs on main's stack (same exemption), like the rs-matter-embassy examples; \
    the big parts of the stack are in statics"
)]
pub async fn run(
    wifi: WIFI<'static>,
    bt: BT<'static>,
    rng: RNG<'static>,
    adc1: ADC1<'static>,
) -> ! {
    let stack = STACK.uninit().init_with(MatterStack::init(
        &DEVICE_INFO,
        TEST_DEV_COMM,
        &TEST_DEV_ATT,
    ));

    // Cryptography needs real randomness: the TRNG mixes radio and ADC noise.
    // `_trng_source` must stay alive as long as the TRNG is used.
    let _trng_source = TrngSource::new(rng, adc1);
    let crypto = default_crypto(
        reseeding_csprng(Trng::try_new().expect("TRNG"), 1000).expect("CSPRNG"),
        DAC_PRIVKEY,
    );
    let mut weak_rand = crypto.weak_rand().expect("random generator");

    // The On/Off cluster: `rs-matter` handles the protocol and calls our
    // `Light` (see `OnOffHooks` below) to actually switch the lamp.
    let on_off = on_off::OnOffHandler::new_standalone(
        Dataver::new_rand(&mut weak_rand),
        LIGHT_ENDPOINT_ID,
        VENDOR_KEYS_START + 0x10,
        Light,
    );

    // Which handler answers which (endpoint, cluster): a chain of filters,
    // a bit like routes in a PHP framework.
    let handler = EmptyHandler
        .chain(
            |endpoint, _| endpoint == ROOT_ENDPOINT_ID,
            Async(MatterStack::<0, EmbassyWifiNetwork>::root_handler(
                stack.matter(),
                &(),
                &mut weak_rand,
            )),
        )
        .chain(
            |endpoint, cluster| endpoint == LIGHT_ENDPOINT_ID && cluster == Light::CLUSTER.id,
            Async(on_off::HandlerAdaptor(&on_off)),
        )
        .chain(
            |endpoint, cluster| {
                endpoint == LIGHT_ENDPOINT_ID && cluster == desc::DescHandler::CLUSTER.id
            },
            Async(desc::DescHandler::new(Dataver::new_rand(&mut weak_rand)).adapt()),
        );

    // No persistence yet (step 4c): a store that saves nothing.
    let mut store = DummyKvBlobStore;
    stack
        .startup(&crypto, &mut store)
        .await
        .expect("Matter startup");
    let kv = stack.matter().kv(store);

    info!("Matter: starting (scan the QR code below with the Home app)");
    let matter = pin!(stack.run_wifi(
        EmbassyWifi::new(
            EspWifiRadio::new(wifi, bt),
            weak_rand,
            true, // random Bluetooth address
            stack,
        ),
        &crypto,
        (NODE, handler),
        kv,
        (), // no extra future to run alongside
    ));
    matter.await.expect("Matter stack stopped");
    unreachable!("the Matter stack never stops without an error")
}

/// What the lamp exposes to Matter: the root endpoint (system clusters) and
/// one On/Off light.
const NODE: Node = Node {
    endpoints: &[
        MatterStack::<0, EmbassyWifiNetwork>::wifi_root_endpoint(),
        Endpoint::new(
            LIGHT_ENDPOINT_ID,
            devices!(DEV_TYPE_ON_OFF_LIGHT),
            clusters!(desc::DescHandler::CLUSTER, Light::CLUSTER),
        ),
    ],
};

/// Our side of the On/Off cluster. It holds no state: the cluster keeps the
/// on/off value, we only pass it on to the strip.
struct Light;

/// `OnOffHooks` is a trait (like a PHP interface): `rs-matter` calls these
/// methods, we decide what they do.
impl OnOffHooks for Light {
    /// The cluster description. A light must have the `LIGHTING` feature,
    /// which brings a few extra attributes and commands; `rs-matter`
    /// implements them all, we just list them.
    const CLUSTER: Cluster<'static> = on_off_cluster::FULL_CLUSTER
        .with_revision(6)
        .with_features(on_off_cluster::Feature::LIGHTING.bits())
        .with_attrs(with!(
            required;
            on_off_cluster::AttributeId::OnOff
                | on_off_cluster::AttributeId::GlobalSceneControl
                | on_off_cluster::AttributeId::OnTime
                | on_off_cluster::AttributeId::OffWaitTime
                | on_off_cluster::AttributeId::StartUpOnOff
        ))
        .with_cmds(with!(
            on_off_cluster::CommandId::Off
                | on_off_cluster::CommandId::On
                | on_off_cluster::CommandId::Toggle
                | on_off_cluster::CommandId::OffWithEffect
                | on_off_cluster::CommandId::OnWithRecallGlobalScene
                | on_off_cluster::CommandId::OnWithTimedOff
        ));

    fn set_on_off(&self, on: bool) {
        info!("Matter: light {}", if on { "on" } else { "off" });
        LIGHT_ON.signal(on);
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {
        // Special "off" animations (fade, dying light…): not for now, the
        // strip task already fades out.
    }
}
