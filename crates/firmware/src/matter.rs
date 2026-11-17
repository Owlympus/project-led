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
//! Our part is three small "hook" types, one per lighting cluster
//! ([`OnOffHooks`], [`LevelControlHooks`], [`ColorControlHooks`]). The stack
//! calls them, they update the shared [`LightState`] and wake the strip task
//! up through [`LIGHT_CHANGED`].
//!
//! Limits of this version:
//! - test certificates: the Home app warns that the lamp is not certified;
//! - nothing is saved in flash yet (step 4c): after a restart, the lamp must
//!   be removed from the Home app and paired again.

use core::cell::Cell;
use core::pin::pin;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_hal::peripherals::{ADC1, BT, RNG, WIFI};
use esp_hal::rng::{Trng, TrngSource};
use led_core::light::matter_scale;
use led_core::{ColorTemperature, LightColor, LightState};
use log::info;
use rs_matter_embassy::matter::basic_info::BasicInfoConfig;
use rs_matter_embassy::matter::crypto::{Crypto, default_crypto};
use rs_matter_embassy::matter::dm::clusters::app::color_control::{
    self, ColorCapabilitiesBitmap, ColorControlHooks, SetDeviceColor,
};
use rs_matter_embassy::matter::dm::clusters::app::level_control::{self, LevelControlHooks};
use rs_matter_embassy::matter::dm::clusters::app::on_off::{self, EffectVariantEnum, OnOffHooks};
use rs_matter_embassy::matter::dm::clusters::decl::color_control as color_cluster;
use rs_matter_embassy::matter::dm::clusters::decl::level_control as level_cluster;
use rs_matter_embassy::matter::dm::clusters::decl::on_off as on_off_cluster;
use rs_matter_embassy::matter::dm::clusters::decl::scenes_management::FULL_CLUSTER as SCENES_CLUSTER;
use rs_matter_embassy::matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter_embassy::matter::dm::clusters::identify::{self, IdentifyHandler};
use rs_matter_embassy::matter::dm::clusters::scenes::{ScenesHandler, ScenesState};
use rs_matter_embassy::matter::dm::devices::DEV_TYPE_EXTENDED_COLOR_LIGHT;
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

/// The lamp's state, as last set by the Home app. Each cluster changes only
/// its own part (on/off, brightness or color), so they share one copy.
///
/// A `Mutex` from `embassy_sync::blocking_mutex` is not a lock that waits:
/// `lock` runs a short closure with interrupts briefly disabled, which is
/// enough to keep two tasks from writing at once. `Cell` lets us replace the
/// value inside a `static` (statics are read-only otherwise).
static LIGHT: Mutex<CriticalSectionRawMutex, Cell<LightState>> =
    Mutex::new(Cell::new(LightState::DEFAULT));

/// Wakes the strip task up with the new state. A `Signal` holds only the
/// latest value: during a fast transition the strip just sees the last step.
pub static LIGHT_CHANGED: Signal<CriticalSectionRawMutex, LightState> = Signal::new();

/// Changes one part of [`LIGHT`] and tells the strip task.
///
/// `impl FnOnce(&mut LightState)` means "any closure that modifies a
/// `LightState`", a bit like a PHP `callable` with a typed parameter.
fn update_light(change: impl FnOnce(&mut LightState)) {
    let state = LIGHT.lock(|cell| {
        let mut state = cell.get();
        change(&mut state);
        cell.set(state);
        state
    });
    LIGHT_CHANGED.signal(state);
}

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

/// Endpoint 0 is reserved for Matter's own system clusters: the light is on
/// endpoint 1.
const LIGHT_ENDPOINT_ID: u16 = 1;

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
    let on_off = on_off::OnOffHandler::new(
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
        .chain(
            |e, c| on_light(e, c, &desc::DescHandler::CLUSTER),
            Async(desc::DescHandler::new(Dataver::new_rand(&mut weak_rand)).adapt()),
        )
        .chain(
            |e, c| on_light(e, c, &identify::CLUSTER),
            Async(IdentifyHandler::new(Dataver::new_rand(&mut weak_rand)).adapt()),
        )
        .chain(
            |e, c| on_light(e, c, &OnOffLight::CLUSTER),
            Async(on_off::HandlerAdaptor(&on_off)),
        )
        .chain(
            |e, c| on_light(e, c, &DimmableLight::CLUSTER),
            Async(level_control::HandlerAdaptor(&level)),
        )
        .chain(
            |e, c| on_light(e, c, &ColorLight::CLUSTER),
            Async(color_control::HandlerAdaptor(&color)),
        )
        .chain(|e, c| on_light(e, c, &SCENES_CLUSTER), scenes.adapt());

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

/// Whether a request for (`endpoint`, `cluster`) is for `wanted` on the light.
///
/// A plain function, not a closure: `chain` only accepts filters that
/// capture nothing (Rust turns them into simple function pointers).
fn on_light(endpoint: u16, cluster: u32, wanted: &Cluster) -> bool {
    endpoint == LIGHT_ENDPOINT_ID && cluster == wanted.id
}

/// What the lamp exposes to Matter: the root endpoint (system clusters) and
/// one Extended Color Light.
///
/// The device type also asks for the Groups cluster, left out for now: it
/// needs `rs-matter`'s `groups` feature (multicast), and the Home app should
/// not need it (ADR-0010).
const NODE: Node = Node {
    endpoints: &[
        MatterStack::<0, EmbassyWifiNetwork>::wifi_root_endpoint(),
        Endpoint::new(
            LIGHT_ENDPOINT_ID,
            devices!(DEV_TYPE_EXTENDED_COLOR_LIGHT),
            clusters!(
                desc::DescHandler::CLUSTER,
                identify::CLUSTER,
                OnOffLight::CLUSTER,
                DimmableLight::CLUSTER,
                ColorLight::CLUSTER,
                SCENES_CLUSTER,
            ),
        ),
    ],
};

/// Our side of the On/Off cluster. It holds no state: the cluster keeps the
/// on/off value, we only pass it on.
struct OnOffLight;

/// `OnOffHooks` is a trait (like a PHP interface): `rs-matter` calls these
/// methods, we decide what they do.
impl OnOffHooks for OnOffLight {
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
        update_light(|light| light.on = on);
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {
        // Special "off" animations (fade, dying light…): not for now.
    }
}

/// Our side of the Level Control cluster (brightness).
struct DimmableLight;

impl LevelControlHooks for DimmableLight {
    /// Matter's brightness scale for lights: 1 (dimmest, still on) to 254.
    const MIN_LEVEL: u8 = 1;
    const MAX_LEVEL: u8 = 254;
    /// Fastest `Move` speed we accept, in levels per second.
    const FASTEST_RATE: u8 = 50;
    /// Switching on or off fades over 0.4 s (unit: tenths of a second).
    /// `rs-matter` runs that fade by stepping the level.
    const ON_OFF_TRANSITION_TIME: u16 = 4;
    /// Start at half brightness, like [`LightState::DEFAULT`].
    const CURRENT_LEVEL: Option<u8> = Some(127);

    /// `ON_OFF` couples the level to the On/Off cluster (fade on/off,
    /// `MoveToLevelWithOnOff`…); `LIGHTING` is required for a light.
    const CLUSTER: Cluster<'static> = level_cluster::FULL_CLUSTER
        .with_features(
            level_control::Feature::LIGHTING.bits() | level_control::Feature::ON_OFF.bits(),
        )
        .with_attrs(with!(
            required;
            level_cluster::AttributeId::CurrentLevel
                | level_cluster::AttributeId::RemainingTime
                | level_cluster::AttributeId::MinLevel
                | level_cluster::AttributeId::MaxLevel
                | level_cluster::AttributeId::OnOffTransitionTime
                | level_cluster::AttributeId::OnLevel
                | level_cluster::AttributeId::OnTransitionTime
                | level_cluster::AttributeId::OffTransitionTime
                | level_cluster::AttributeId::DefaultMoveRate
                | level_cluster::AttributeId::Options
                | level_cluster::AttributeId::StartUpCurrentLevel
        ))
        .with_cmds(with!(
            level_cluster::CommandId::MoveToLevel
                | level_cluster::CommandId::Move
                | level_cluster::CommandId::Step
                | level_cluster::CommandId::Stop
                | level_cluster::CommandId::MoveToLevelWithOnOff
                | level_cluster::CommandId::MoveWithOnOff
                | level_cluster::CommandId::StepWithOnOff
                | level_cluster::CommandId::StopWithOnOff
        ));

    /// Called at every step of a transition, so no log here (too many).
    fn set_device_level(&self, level: u8) -> Result<Option<u8>, ()> {
        update_light(|light| light.brightness = matter_scale::brightness(level));
        // We can show every level: report the one asked.
        Ok(Some(level))
    }
}

/// Our side of the Color Control cluster (colors and white temperature).
struct ColorLight;

impl ColorControlHooks for ColorLight {
    /// All three color modes: hue/saturation, CIE xy (required by the
    /// Extended Color Light device type) and white temperature.
    const CLUSTER: Cluster<'static> = color_cluster::FULL_CLUSTER
        .with_features(
            color_cluster::Feature::HUE_AND_SATURATION.bits()
                | color_cluster::Feature::XY.bits()
                | color_cluster::Feature::COLOR_TEMPERATURE.bits(),
        )
        .with_attrs(with!(
            required;
            color_cluster::AttributeId::CurrentHue
                | color_cluster::AttributeId::CurrentSaturation
                | color_cluster::AttributeId::CurrentX
                | color_cluster::AttributeId::CurrentY
                | color_cluster::AttributeId::RemainingTime
                | color_cluster::AttributeId::ColorTemperatureMireds
                | color_cluster::AttributeId::ColorTempPhysicalMinMireds
                | color_cluster::AttributeId::ColorTempPhysicalMaxMireds
                | color_cluster::AttributeId::StartUpColorTemperatureMireds
        ))
        .with_cmds(with!(
            color_cluster::CommandId::MoveToHue
                | color_cluster::CommandId::MoveHue
                | color_cluster::CommandId::StepHue
                | color_cluster::CommandId::MoveToSaturation
                | color_cluster::CommandId::MoveSaturation
                | color_cluster::CommandId::StepSaturation
                | color_cluster::CommandId::MoveToHueAndSaturation
                | color_cluster::CommandId::MoveToColor
                | color_cluster::CommandId::MoveColor
                | color_cluster::CommandId::StepColor
                | color_cluster::CommandId::MoveToColorTemperature
                | color_cluster::CommandId::MoveColorTemperature
                | color_cluster::CommandId::StepColorTemperature
                | color_cluster::CommandId::StopMoveStep
        ));

    /// Must match the features above.
    const COLOR_CAPABILITIES: ColorCapabilitiesBitmap = ColorCapabilitiesBitmap::from_bits_truncate(
        ColorCapabilitiesBitmap::HUE_SATURATION.bits()
            | ColorCapabilitiesBitmap::XY.bits()
            | ColorCapabilitiesBitmap::COLOR_TEMPERATURE.bits(),
    );

    /// The whites our strip can make: from its cold LED (fewest mireds) to
    /// its warm LED (most mireds). The Home app's slider uses this range.
    const COLOR_TEMP_PHYSICAL_MIN_MIREDS: u16 = crate::STRIP_WHITE_CHANNELS.cold.mireds();
    const COLOR_TEMP_PHYSICAL_MAX_MIREDS: u16 = crate::STRIP_WHITE_CHANNELS.warm.mireds();

    /// Start in the same white as [`LightState::DEFAULT`].
    const COLOR: Option<SetDeviceColor> = Some(SetDeviceColor::ColorTemperature {
        mireds: ColorTemperature::from_kelvin(3000).mireds(),
    });

    /// Called at every step of a transition, so no log here (too many).
    fn set_device_color(&self, target: SetDeviceColor) -> Result<(), ()> {
        let color = match target {
            SetDeviceColor::ColorTemperature { mireds } => {
                LightColor::Temperature(ColorTemperature::from_mireds(mireds))
            }
            SetDeviceColor::HueSaturation {
                enhanced_hue,
                saturation,
            } => LightColor::HueSaturation {
                // The "enhanced" hue already uses the full u16, like `Hsv`.
                hue: enhanced_hue,
                saturation: matter_scale::saturation(saturation),
            },
            // We have no xy → RGB conversion of our own: `rs-matter`'s
            // turns the point into a hue and a saturation.
            xy @ SetDeviceColor::Xy { .. } => {
                let (hue, saturation) = xy.to_hue_saturation();
                LightColor::HueSaturation {
                    hue,
                    saturation: matter_scale::saturation(saturation),
                }
            }
        };
        update_light(|light| light.color = color);
        Ok(())
    }
}
