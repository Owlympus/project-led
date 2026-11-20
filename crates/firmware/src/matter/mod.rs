//! Matter: makes the lamp a light in the iPhone Home app (ADR-0004, ADR-0009).
//!
//! The lamp is an **Extended Color Light**: on/off, brightness, colors and
//! white temperature (step 4b).
//!
//! What the Matter stack (`rs-matter-embassy`) does for us:
//! - at first start, it advertises the lamp over **Bluetooth** and prints a
//!   pairing QR code in the logs;
//! - during pairing, the iPhone sends it the **Wi-Fi** name and password, and
//!   the stack joins the network by itself;
//! - then it answers the Apple TV's commands over Wi-Fi, and runs the
//!   **transitions** itself (a fade asked by the Home app arrives here as a
//!   series of small steps, ADR-0010).
//!
//! Our part is small "hook" types, one per cluster, in their own modules.
//! The stack calls them, they update the shared state ([`light`]) and wake
//! the strip task up through [`LIGHT_CHANGED`]:
//! - [`on_off`], [`level`] and [`color`]: the light itself;
//! - [`ambiance`]: one extra switch per ambiance (endpoints 2, 3, 4; step
//!   4f, ADR-0014).
//!
//! Around them:
//! - [`node`]: the endpoints and clusters the lamp exposes;
//! - [`status`]: the connection's **status signals** on the strip (step 4e,
//!   ADR-0015), sent through [`STATUS_CHANGED`];
//! - [`store`]: everything Matter must remember (pairing, Wi-Fi, the lamp's
//!   last state) in the flash's **NVS partition** (step 4c, ADR-0011).
//!   Holding the touch button at power-up erases it (step 4d, ADR-0012).
//!
//! Limit: test certificates, so the Home app warns that the lamp is not
//! certified.

// One `mod x;` per file `x.rs` of the `matter/` folder; this file, `mod.rs`,
// is the folder's own module (a bit like a PHP namespace per folder).
mod ambiance;
mod color;
mod level;
mod light;
mod node;
mod on_off;
mod status;
mod store;

use core::pin::pin;

use embassy_futures::select::{Either, select};
use esp_hal::peripherals::{ADC1, BT, FLASH, RNG, WIFI};
use esp_hal::rng::{Trng, TrngSource};
use led_core::Ambiance;
use log::info;
use rs_matter_embassy::matter::basic_info::BasicInfoConfig;
use rs_matter_embassy::matter::crypto::{Crypto, default_crypto};
use rs_matter_embassy::matter::dm::clusters::app::color_control::{self, ColorControlHooks};
use rs_matter_embassy::matter::dm::clusters::app::level_control::{self, LevelControlHooks};
// `as`: renamed, because our own module is already called `on_off`.
use rs_matter_embassy::matter::dm::clusters::app::on_off::{self as on_off_handler, OnOffHooks};
use rs_matter_embassy::matter::dm::clusters::decl::scenes_management::FULL_CLUSTER as SCENES_CLUSTER;
use rs_matter_embassy::matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter_embassy::matter::dm::clusters::identify::{self, IdentifyHandler};
use rs_matter_embassy::matter::dm::clusters::scenes::{ScenesHandler, ScenesState};
use rs_matter_embassy::matter::dm::devices::test::{
    DAC_PRIVKEY, TEST_DEV_ATT, TEST_DEV_COMM, TEST_DEV_DET,
};
use rs_matter_embassy::matter::dm::endpoints::ROOT_ENDPOINT_ID;
use rs_matter_embassy::matter::dm::{Async, Dataver, EmptyHandler};
use rs_matter_embassy::matter::persist::VENDOR_KEYS_START;
use rs_matter_embassy::matter::utils::init::InitMaybeUninit;
use rs_matter_embassy::rand::reseeding_csprng;
use rs_matter_embassy::stack::MatterStack;
use rs_matter_embassy::stack::wireless::BTP_BUMP_SIZE;
use rs_matter_embassy::wireless::esp::EspWifiRadio;
use rs_matter_embassy::wireless::{EmbassyWifi, EmbassyWifiNetwork};
use static_cell::StaticCell;

// What the rest of the firmware uses: `pub use` re-exports it from here, so
// it is `matter::LIGHT_CHANGED` and not `matter::light::LIGHT_CHANGED`.
pub use light::{LIGHT_CHANGED, LIGHT_REQUEST, LightRequest};
pub use status::STATUS_CHANGED;

use ambiance::AmbianceSwitch;
use color::ColorLight;
use level::DimmableLight;
use node::{AMBIANCE_ENDPOINT_IDS, LIGHT_ENDPOINT_ID, NODE, on_ambiance, on_light};
use on_off::OnOffLight;
use status::{WifiWatcher, watch_status};

/// Memory for the stack's internal futures, on top of the stack itself.
/// The three lighting clusters run their transitions as futures, hence more
/// than for the On/Off light (value from the `rs-matter` color light
/// example). If the stack panics at startup because it is too small,
/// increase it.
const BUMP_SIZE: usize = 32 * 1024 + BTP_BUMP_SIZE;

/// What the lamp tells the Home app about itself. The test vendor and product
/// ids (from `TEST_DEV_DET`) must stay: they match the test certificates.
const DEVICE_INFO: BasicInfoConfig = BasicInfoConfig {
    vendor_name: "project-led",
    product_name: "Lampe barre",
    device_name: "Lampe barre",
    ..TEST_DEV_DET
};

/// How many scenes the lamp can remember (scenes are part of the Extended
/// Color Light device type).
const SCENES_CAPACITY: usize = 16;

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
    flash: FLASH<'static>,
    forget_pairing: bool,
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

    // The scene table, shared by the Scenes cluster and the three lighting
    // clusters (a scene captures and recalls their values).
    let scenes_state = ScenesState::<SCENES_CAPACITY>::new();

    // The three lighting clusters: `rs-matter` handles the protocol and the
    // transitions, and calls our hooks to actually change the lamp.
    let on_off = on_off_handler::OnOffHandler::new(
        Dataver::new_rand(&mut weak_rand),
        LIGHT_ENDPOINT_ID,
        VENDOR_KEYS_START + 0x10,
        OnOffLight,
    )
    .with_scene_invalidator(&scenes_state);
    let level = level_control::LevelControlHandler::new(
        Dataver::new_rand(&mut weak_rand),
        LIGHT_ENDPOINT_ID,
        VENDOR_KEYS_START + 0x11,
        DimmableLight,
    )
    .with_scene_invalidator(&scenes_state);
    let color = color_control::ColorControlHandler::new(
        Dataver::new_rand(&mut weak_rand),
        LIGHT_ENDPOINT_ID,
        VENDOR_KEYS_START + 0x12,
        ColorLight,
    )
    .with_scene_invalidator(&scenes_state);

    // One On/Off cluster per ambiance switch, on its own endpoint. They
    // stand alone (no brightness or color of their own).
    let ambiance_switches = Ambiance::ALL.map(|ambiance| {
        on_off_handler::OnOffHandler::new_standalone(
            Dataver::new_rand(&mut weak_rand),
            AMBIANCE_ENDPOINT_IDS[ambiance.index()],
            VENDOR_KEYS_START + 0x20 + ambiance.index() as u16,
            AmbianceSwitch(ambiance),
        )
    });
    let [sunset, aurora, campfire] = &ambiance_switches;

    // The clusters work together: switching on with a fade goes through the
    // level, and colors can be refused while the lamp is off.
    on_off.init(Some(&level));
    level.init(Some(&on_off));
    color.init(Some(&on_off));

    let scenes = ScenesHandler::new(
        Dataver::new_rand(&mut weak_rand),
        stack.matter(),
        &scenes_state,
        (&on_off, (&level, (&color, ()))),
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
        // Descriptor and Identify answer for every endpoint but the root
        // (handled above): the light and the ambiance switches.
        .chain(
            |_, c| c == desc::DescHandler::CLUSTER.id,
            Async(desc::DescHandler::new(Dataver::new_rand(&mut weak_rand)).adapt()),
        )
        .chain(
            |_, c| c == identify::CLUSTER.id,
            Async(IdentifyHandler::new(Dataver::new_rand(&mut weak_rand)).adapt()),
        )
        .chain(
            |e, c| on_light(e, c, &OnOffLight::CLUSTER),
            Async(on_off_handler::HandlerAdaptor(&on_off)),
        )
        .chain(
            |e, c| on_light(e, c, &DimmableLight::CLUSTER),
            Async(level_control::HandlerAdaptor(&level)),
        )
        .chain(
            |e, c| on_light(e, c, &ColorLight::CLUSTER),
            Async(color_control::HandlerAdaptor(&color)),
        )
        .chain(|e, c| on_light(e, c, &SCENES_CLUSTER), scenes.adapt())
        // `{ … }` around the endpoint: a const generic argument computed
        // from a constant (so the ids are written only once, in `node`).
        .chain(
            on_ambiance::<{ AMBIANCE_ENDPOINT_IDS[0] }>,
            Async(on_off_handler::HandlerAdaptor(sunset)),
        )
        .chain(
            on_ambiance::<{ AMBIANCE_ENDPOINT_IDS[1] }>,
            Async(on_off_handler::HandlerAdaptor(aurora)),
        )
        .chain(
            on_ambiance::<{ AMBIANCE_ENDPOINT_IDS[2] }>,
            Async(on_off_handler::HandlerAdaptor(campfire)),
        );

    // Matter's saved data lives in the flash's NVS partition. `startup`
    // reads it back: the pairing, then each cluster's last value.
    let mut store = store::flash_store(flash, forget_pairing);
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
        WifiWatcher, // runs from startup, Wi-Fi or not (ADR-0015)
    ));

    // Run the stack and the status watcher side by side. Neither ends on
    // its own: the stack only stops on an error (a bug, so we panic).
    match select(matter, watch_status(stack.matter())).await {
        Either::First(result) => {
            result.expect("Matter stack stopped");
            unreachable!("the Matter stack never stops without an error")
        }
    }
}
