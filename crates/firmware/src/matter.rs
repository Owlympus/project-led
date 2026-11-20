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
//! Everything Matter needs to remember (pairing, Wi-Fi, and the lamp's last
//! on/off, brightness and color) is saved in the flash's **NVS partition**
//! (step 4c, ADR-0011): after a restart, the lamp is still in the Home app
//! and comes back as it was.
//!
//! Holding the touch button at power-up erases that partition: the lamp
//! forgets its pairing and waits to be added again (step 4d, ADR-0012).
//!
//! The **ambiances** (step 4f, ADR-0014) are extra switches on the same
//! lamp, one per ambiance (endpoints 2, 3, 4): [`AmbianceSwitch`].
//!
//! The strip also shows the connection's **status signals** (step 4e,
//! ADR-0013): [`watch_status`] picks the one to show and sends it through
//! [`STATUS_CHANGED`].
//!
//! Limit: test certificates, so the Home app warns that the lamp is not
//! certified.

use core::cell::Cell;
use core::future::pending;
use core::pin::pin;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_futures::select::{Either, Either3, select, select3};

use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use esp_bootloader_esp_idf::partitions::{
    self, DataPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionType,
};
use esp_hal::peripherals::{ADC1, BT, FLASH, RNG, WIFI};
use esp_hal::rng::{Trng, TrngSource};
use esp_storage::FlashStorage;
use led_core::light::matter_scale;
use led_core::{Ambiance, ColorTemperature, LightColor, LightState, StatusSignal};
use log::{debug, info, warn};
use rs_matter_embassy::matter::Matter;
use rs_matter_embassy::matter::basic_info::BasicInfoConfig;
use rs_matter_embassy::matter::crypto::{Crypto, default_crypto};
use rs_matter_embassy::matter::dm::clusters::app::color_control::{
    self, ColorCapabilitiesBitmap, ColorControlHooks, SetDeviceColor,
};
use rs_matter_embassy::matter::dm::clusters::app::level_control::{
    self, LevelControlHooks, OutOfBandMessage as LevelMessage,
};
use rs_matter_embassy::matter::dm::clusters::app::on_off::{
    self, EffectVariantEnum, OnOffHooks, OutOfBandMessage,
};
use rs_matter_embassy::matter::dm::clusters::decl::color_control as color_cluster;
use rs_matter_embassy::matter::dm::clusters::decl::level_control as level_cluster;
use rs_matter_embassy::matter::dm::clusters::decl::on_off as on_off_cluster;
use rs_matter_embassy::matter::dm::clusters::decl::scenes_management::FULL_CLUSTER as SCENES_CLUSTER;
use rs_matter_embassy::matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter_embassy::matter::dm::clusters::gen_diag::NetifDiag;
use rs_matter_embassy::matter::dm::clusters::identify::{self, IdentifyHandler};
use rs_matter_embassy::matter::dm::clusters::scenes::{ScenesHandler, ScenesState};
use rs_matter_embassy::matter::dm::devices::test::{
    DAC_PRIVKEY, TEST_DEV_ATT, TEST_DEV_COMM, TEST_DEV_DET,
};
use rs_matter_embassy::matter::dm::devices::{
    DEV_TYPE_EXTENDED_COLOR_LIGHT, DEV_TYPE_ON_OFF_LIGHT,
};
use rs_matter_embassy::matter::dm::endpoints::ROOT_ENDPOINT_ID;
use rs_matter_embassy::matter::dm::networks::NetChangeNotif;
use rs_matter_embassy::matter::dm::{Async, Cluster, Dataver, EmptyHandler, Endpoint, Node};
use rs_matter_embassy::matter::error::Error;
use rs_matter_embassy::matter::persist::VENDOR_KEYS_START;
use rs_matter_embassy::matter::utils::init::InitMaybeUninit;
use rs_matter_embassy::matter::utils::sync::MaybeSend;
use rs_matter_embassy::matter::{clusters, devices, with};
use rs_matter_embassy::persist::SeqMapKvBlobStore;
use rs_matter_embassy::rand::reseeding_csprng;
use rs_matter_embassy::stack::net::NetStack;
use rs_matter_embassy::stack::wireless::BTP_BUMP_SIZE;
use rs_matter_embassy::stack::{MatterStack, UserTask};
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

/// A request for the main light that doesn't come from the network. The
/// On/Off cluster applies it exactly as if the Home app had asked (see
/// [`OnOffLight::run`]).
#[derive(Debug, Clone, Copy)]
pub enum LightRequest {
    /// A tap on the touch button.
    Toggle,
    /// An ambiance was switched on while the lamp was off.
    On,
    /// The ambiance shown was switched off: the lamp goes off (ADR-0016).
    Off,
}
pub static LIGHT_REQUEST: Signal<CriticalSectionRawMutex, LightRequest> = Signal::new();

/// One per ambiance switch (same order as [`Ambiance::ALL`]): tells its
/// On/Off cluster to *show* on or off in the Home app, without switching
/// anything (see [`AmbianceSwitch::run`]).
///
/// `[const { … }; N]` builds an array of N values that can't be copied
/// (a `Signal` can't): the block is evaluated once per element.
static AMBIANCE_SWITCH_SHOWS: [Signal<CriticalSectionRawMutex, bool>; 3] =
    [const { Signal::new() }; 3];

/// The status signal to show on the strip (`None`: just the lamp's color).
/// Sent only when it changes.
pub static STATUS_CHANGED: Signal<CriticalSectionRawMutex, Option<StatusSignal>> = Signal::new();

/// Whether the Wi-Fi is up right now, and whether it has been since power-up
/// (see [`WifiWatcher`]). An `AtomicBool` is a `bool` that several tasks can
/// read and write safely, without a mutex.
static WIFI_UP: AtomicBool = AtomicBool::new(false);
static WIFI_WAS_UP: AtomicBool = AtomicBool::new(false);

/// How often [`watch_status`] looks at the connection.
const STATUS_POLL: Duration = Duration::from_millis(250);

/// Changes one part of [`LIGHT`] and tells the strip task.
///
/// `impl FnOnce(&mut LightState)` means "any closure that modifies a
/// `LightState`", a bit like a PHP `callable` with a typed parameter.
///
/// Returns the state *before* the change, so the caller can see what it
/// replaced (an ambiance, say).
fn update_light(change: impl FnOnce(&mut LightState)) -> LightState {
    let (before, after) = LIGHT.lock(|cell| {
        let before = cell.get();
        let mut after = before;
        change(&mut after);
        cell.set(after);
        (before, after)
    });
    LIGHT_CHANGED.signal(after);
    before
}

/// After `before` was replaced: if it had an ambiance and the new state
/// doesn't show it any more, its switch shows "off" in the Home app.
fn ambiance_left(before: &LightState) {
    let now = LIGHT.lock(Cell::get);
    if let Some(ambiance) = before.ambiance
        && now.ambiance != Some(ambiance)
    {
        info!("ambiance off: {}", ambiance.name());
        AMBIANCE_SWITCH_SHOWS[switch_index(ambiance)].signal(false);
    }
}

/// Position of `ambiance` in [`Ambiance::ALL`] (and of its switch).
fn switch_index(ambiance: Ambiance) -> usize {
    Ambiance::ALL
        .iter()
        .position(|candidate| *candidate == ambiance)
        .expect("every ambiance is in Ambiance::ALL")
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

/// The ambiance switches: endpoints 2, 3 and 4, in the order of
/// [`Ambiance::ALL`] (Sunset, Aurora, Campfire).
const AMBIANCE_ENDPOINT_IDS: [u16; 3] = [2, 3, 4];

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

    // One On/Off cluster per ambiance switch, on its own endpoint. They
    // stand alone (no brightness or color of their own).
    let ambiance_switches = core::array::from_fn::<_, 3, _>(|index| {
        on_off::OnOffHandler::new_standalone(
            Dataver::new_rand(&mut weak_rand),
            AMBIANCE_ENDPOINT_IDS[index],
            VENDOR_KEYS_START + 0x20 + index as u16,
            AmbianceSwitch {
                ambiance: Ambiance::ALL[index],
                index,
            },
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
        .chain(|e, c| on_light(e, c, &SCENES_CLUSTER), scenes.adapt())
        .chain(on_ambiance::<2>, Async(on_off::HandlerAdaptor(sunset)))
        .chain(on_ambiance::<3>, Async(on_off::HandlerAdaptor(aurora)))
        .chain(on_ambiance::<4>, Async(on_off::HandlerAdaptor(campfire)));

    // Matter's saved data lives in the flash's NVS partition. `startup`
    // reads it back: the pairing, then each cluster's last value.
    let mut store = flash_store(flash, forget_pairing);
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
        WifiWatcher, // runs only while the Wi-Fi is up
    ));

    // Run the stack and the status watcher side by side. Neither ends on
    // its own: the stack only stops on an error (a bug, so we panic).
    match select(matter, watch_status(stack.matter())).await {
        Either::First(result) => {
            result.expect("Matter stack stopped");
            unreachable!("the Matter stack never stops without an error")
        }
        Either::Second(()) => unreachable!("the status watcher never stops"),
    }
}

/// Picks the status signal to show, and tells the strip when it changes.
///
/// It asks Matter a few times per second rather than waiting for events:
/// simple, and cheap enough (two flags and a list of fabrics to look at).
async fn watch_status(matter: &Matter<'_>) {
    let mut shown = None;
    // Whether the lamp was paired and on the Wi-Fi at the last look, and
    // until when the blue "connected" signal shows.
    let mut was_synced = false;
    let mut connected_until = Instant::now();
    loop {
        // A "fabric" is a Matter network the lamp belongs to; ours is the
        // Apple TV's. No fabric: not paired yet.
        let paired = matter.has_fabrics();
        let wifi_up = WIFI_UP.load(Ordering::Relaxed);
        let synced = paired && wifi_up;
        if synced && !was_synced {
            // Just became fully connected (after pairing, at power-up, or
            // when the Wi-Fi comes back): one blue round trip.
            connected_until =
                Instant::now() + Duration::from_millis(StatusSignal::CONNECTED_MS.into());
        }
        was_synced = synced;

        let status = if !paired {
            Some(StatusSignal::WaitingForPairing)
        } else if synced && Instant::now() < connected_until {
            Some(StatusSignal::Connected)
        } else if !wifi_up && WIFI_WAS_UP.load(Ordering::Relaxed) {
            Some(StatusSignal::WifiLost)
        } else {
            // All good, or paired and still joining the Wi-Fi after power-up:
            // just the lamp's color.
            None
        };

        if status != shown {
            info!("status signal: {status:?}");
            STATUS_CHANGED.signal(status);
            shown = status;
        }
        Timer::after(STATUS_POLL).await;
    }
}

/// Keeps [`WIFI_UP`] up to date, as a Matter "user task".
///
/// `rs-matter` hands this task the network interface (`netif`). It runs from
/// startup, Wi-Fi or not (first tests on the board, ADR-0013): so it reads
/// the interface's real state, *operational* or not, and waits for it to
/// change.
struct WifiWatcher;

impl UserTask for WifiWatcher {
    async fn run<S, N>(&mut self, _net_stack: S, netif: N) -> Result<(), Error>
    where
        S: NetStack,
        N: NetifDiag + NetChangeNotif,
    {
        // If `rs-matter` ever stops this task, assume the Wi-Fi is gone.
        let _down_on_stop = WifiDownOnDrop;
        loop {
            // `netifs` calls the closure once per interface (just the Wi-Fi
            // here). `|=`: up if any interface is operational.
            let mut up = false;
            netif.netifs(&mut |info| {
                up |= info.operational;
                Ok(())
            })?;
            set_wifi_up(up);
            // Sleep until the interface changes (link, addresses…).
            netif.wait_changed().await;
        }
    }
}

/// Updates [`WIFI_UP`] (and [`WIFI_WAS_UP`]), logging only the changes.
fn set_wifi_up(up: bool) {
    // `swap` writes the new value and returns the old one, in one step.
    let was = WIFI_UP.swap(up, Ordering::Relaxed);
    if up {
        WIFI_WAS_UP.store(true, Ordering::Relaxed);
    }
    match (was, up) {
        (false, true) => info!("Wi-Fi: up"),
        (true, false) => warn!("Wi-Fi: down"),
        _ => {}
    }
}

/// Lowers [`WIFI_UP`] when dropped, that is when [`WifiWatcher`]'s task is
/// stopped.
///
/// `Drop` is Rust's destructor, like PHP's `__destruct`, but it runs at a
/// known moment (when the value goes away), never "later".
struct WifiDownOnDrop;

impl Drop for WifiDownOnDrop {
    fn drop(&mut self) {
        set_wifi_up(false);
    }
}

/// Opens the flash's NVS partition as a key-value store for Matter.
///
/// The partition table (written by `espflash` next to the program) says
/// where NVS is; with the default table, 24 KB at 0x9000. Reading it
/// instead of hard-coding the address keeps working if the table changes.
///
/// With `erase`, the partition is wiped first: the lamp forgets its pairing
/// and its last state, as on its very first start.
#[allow(
    clippy::large_stack_frames,
    reason = "called once from `run`, on main's stack (same exemption); the 3 KB table \
    buffer is freed as soon as the NVS partition is found"
)]
fn flash_store(
    flash: FLASH<'static>,
    erase: bool,
) -> SeqMapKvBlobStore<BlockingAsync<FlashStorage<'static>>> {
    let mut flash = FlashStorage::new(flash);

    let mut table = [0u8; PARTITION_TABLE_MAX_LEN];
    let nvs = partitions::read_partition_table(&mut flash, &mut table)
        .expect("partition table")
        .find_partition(PartitionType::Data(DataPartitionSubType::Nvs))
        .expect("partition table")
        .expect("no NVS partition in the partition table");
    let range = nvs.offset()..nvs.offset() + nvs.len();
    info!("Matter: saving to the NVS partition ({range:#x?})");

    if erase {
        warn!("Matter: erasing the NVS partition (pairing forgotten)");
        flash
            .erase(range.start, range.end)
            .expect("erasing the NVS partition");
    }

    // The store is written for async flash; ours is blocking (each write
    // waits for the chip), `BlockingAsync` adapts one to the other.
    SeqMapKvBlobStore::new(BlockingAsync::new(flash), range)
}

/// Whether a request for (`endpoint`, `cluster`) is for `wanted` on the light.
///
/// A plain function, not a closure: `chain` only accepts filters that
/// capture nothing (Rust turns them into simple function pointers).
fn on_light(endpoint: u16, cluster: u32, wanted: &Cluster) -> bool {
    endpoint == LIGHT_ENDPOINT_ID && cluster == wanted.id
}

/// Whether a request is for the On/Off cluster of the ambiance switch on
/// endpoint `E`.
///
/// `<const E: u16>` is a *const generic*: the compiler makes one copy of the
/// function per value used (`on_ambiance::<2>`, `::<3>`…). Each copy is a
/// plain function, which `chain` accepts, unlike a closure that would
/// capture the endpoint.
fn on_ambiance<const E: u16>(endpoint: u16, cluster: u32) -> bool {
    endpoint == E && cluster == AmbianceSwitch::CLUSTER.id
}

/// What the lamp exposes to Matter: the root endpoint (system clusters),
/// one Extended Color Light and one switch per ambiance.
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
        ambiance_endpoint(AMBIANCE_ENDPOINT_IDS[0]),
        ambiance_endpoint(AMBIANCE_ENDPOINT_IDS[1]),
        ambiance_endpoint(AMBIANCE_ENDPOINT_IDS[2]),
    ],
};

/// An ambiance switch's endpoint: an On/Off Light for the Home app.
const fn ambiance_endpoint(id: u16) -> Endpoint<'static> {
    Endpoint::new(
        id,
        devices!(DEV_TYPE_ON_OFF_LIGHT),
        clusters!(
            desc::DescHandler::CLUSTER,
            identify::CLUSTER,
            AmbianceSwitch::CLUSTER,
        ),
    )
}

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
        // Switching the lamp off also leaves its ambiance.
        let before = update_light(|light| {
            light.on = on;
            if !on {
                light.ambiance = None;
            }
        });
        ambiance_left(&before);
        if !on {
            // Let the brightness side check where it was left (see
            // `DimmableLight::run`).
            RESTORE_LEVEL.signal(());
        }
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {
        // Special "off" animations (fade, dying light…): not for now.
    }

    /// Runs alongside the cluster, for requests that don't come from the
    /// network: here, the touch button. `notify` hands them to the cluster,
    /// which then applies them like a Home app command (fade, saved state,
    /// Home app updated). Must never return.
    async fn run<F: Fn(OutOfBandMessage) + MaybeSend>(&self, notify: F) {
        loop {
            notify(match LIGHT_REQUEST.wait().await {
                LightRequest::Toggle => OutOfBandMessage::Toggle,
                LightRequest::On => OutOfBandMessage::On,
                LightRequest::Off => OutOfBandMessage::Off,
            });
        }
    }
}

/// Our side of the Level Control cluster (brightness).
struct DimmableLight;

/// Each level `rs-matter` sets, for [`DimmableLight::run`] to spot the one
/// the user settled on.
static LEVEL_SET: Signal<CriticalSectionRawMutex, u8> = Signal::new();
/// The lamp is off with its saved brightness possibly at the minimum: put
/// back the chosen one (see [`DimmableLight::run`]).
static RESTORE_LEVEL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// A level that stays this long is one the user chose (fades and Home app
/// transitions move on much faster).
const LEVEL_SETTLE: Duration = Duration::from_secs(1);

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
        let before = update_light(|light| light.brightness = matter_scale::brightness(level));
        LEVEL_SET.signal(level);
        // Lamp off and brightness at the minimum (the Home app often sends
        // "level 1" right after switching off): fix it at once.
        if !before.on && level <= Self::MIN_LEVEL {
            RESTORE_LEVEL.signal(());
        }
        // We can show every level: report the one asked.
        Ok(Some(level))
    }

    /// Works around a trap of Matter's "switch on" (first tests on the
    /// board, 2026-11-19): switching on goes back to the brightness saved
    /// when the lamp went off. If that is the **minimum** (slider pulled
    /// down to off, or a fade-out saved just before a restart), the lamp
    /// "switches on" to the minimum level, which Matter turns off again at
    /// once: the lamp seems to refuse to switch on.
    ///
    /// So: remember the last level the user settled on, and as soon as the
    /// lamp is off (or is set to the minimum while off), quietly put the
    /// saved brightness back to it, with no delay: a first version waited
    /// 1.3 s, and a quick off/on came in before the fix. `Update` only
    /// changes what Matter reports and saves; it doesn't drive the lamp or
    /// switch it on.
    async fn run<F: Fn(LevelMessage) + MaybeSend>(&self, notify: F) {
        let mut chosen = Self::CURRENT_LEVEL.unwrap_or(Self::MAX_LEVEL / 2);
        // The very first level is the one restored from flash at startup:
        // it counts even though the lamp is off then.
        let mut first = true;
        // The latest level seen, until it has stayed long enough to count.
        let mut settling: Option<u8> = None;
        loop {
            // Ends after `LEVEL_SETTLE` if a level is settling, never
            // otherwise. `async move` takes a copy of the `bool`, so the
            // future doesn't borrow `settling` (changed in the match below).
            let waiting = settling.is_some();
            let settled = async move {
                if waiting {
                    Timer::after(LEVEL_SETTLE).await;
                } else {
                    pending::<()>().await;
                }
            };
            // `select3`: like `select`, with three things to wait for.
            match select3(LEVEL_SET.wait(), RESTORE_LEVEL.wait(), settled).await {
                // A new level: start counting again from it.
                Either3::First(level) => settling = Some(level),
                // No delay, whatever the order of Matter's commands: only
                // the saved value changes, the lamp stays off.
                Either3::Second(()) => notify(LevelMessage::Update(chosen)),
                Either3::Third(()) => {
                    if let Some(level) = settling.take() {
                        let lamp_on = LIGHT.lock(Cell::get).on;
                        if (lamp_on || first) && level > Self::MIN_LEVEL {
                            chosen = level;
                        }
                        first = false;
                    }
                }
            }
        }
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

    /// Called at every step of a transition (every 100 ms), hence a
    /// `debug!` log only: hidden by default.
    fn set_device_color(&self, target: SetDeviceColor) -> Result<(), ()> {
        debug!("Matter: color {target:?}");
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
        // A new color from the Home app leaves the ambiance. (Same color:
        // nothing changes, e.g. when the cluster restores it at startup.)
        let before = update_light(|light| {
            if light.color != color {
                light.color = color;
                light.ambiance = None;
            }
        });
        ambiance_left(&before);
        Ok(())
    }
}

/// Our side of an ambiance switch's On/Off cluster.
struct AmbianceSwitch {
    ambiance: Ambiance,
    /// Its position in [`Ambiance::ALL`] and [`AMBIANCE_SWITCH_SHOWS`].
    index: usize,
}

impl OnOffHooks for AmbianceSwitch {
    /// Same description as the main light's On/Off cluster.
    const CLUSTER: Cluster<'static> = OnOffLight::CLUSTER;
    /// Ambiances always start off after a power cut: the lamp comes back in
    /// its plain color (ADR-0014).
    const START_UP_ON_OFF: Option<on_off_cluster::StartUpOnOffEnum> =
        Some(on_off_cluster::StartUpOnOffEnum::Off);

    fn set_on_off(&self, on: bool) {
        let ambiance = self.ambiance;
        if on {
            info!("ambiance on: {}", ambiance.name());
            let before = update_light(|light| light.ambiance = Some(ambiance));
            // Only one ambiance at a time: the previous one's switch goes off.
            ambiance_left(&before);
            // An ambiance on a lamp that is off: switch the lamp on too
            // (through Matter, so the Home app follows and the fade applies).
            if !before.on {
                LIGHT_REQUEST.signal(LightRequest::On);
            }
        } else if LIGHT.lock(Cell::get).ambiance == Some(ambiance) {
            // Switching off the ambiance shown switches the lamp off
            // (ADR-0016), through Matter so the Home app follows and the
            // fade applies. The ambiance stays on the strip during the fade
            // (no flash of the plain color); switching the lamp off then
            // leaves it.
            info!("ambiance off: {} (its switch), lamp off", ambiance.name());
            LIGHT_REQUEST.signal(LightRequest::Off);
        }
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {}

    /// Shows the switch on or off in the Home app when another part of the
    /// lamp changed the ambiance. `Update` only changes what Matter reports:
    /// it doesn't call `set_on_off` again (no loop).
    async fn run<F: Fn(OutOfBandMessage) + MaybeSend>(&self, notify: F) {
        loop {
            let on = AMBIANCE_SWITCH_SHOWS[self.index].wait().await;
            notify(OutOfBandMessage::Update(on));
        }
    }
}
