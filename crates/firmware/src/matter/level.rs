//! Our side of the Level Control cluster (brightness).

use core::future::pending;

use embassy_futures::select::{Either3, select3};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use led_core::light::matter_scale;
use rs_matter_embassy::matter::dm::Cluster;
use rs_matter_embassy::matter::dm::clusters::app::level_control::{
    self, LevelControlHooks, OutOfBandMessage,
};
use rs_matter_embassy::matter::dm::clusters::decl::level_control as level_cluster;
use rs_matter_embassy::matter::utils::sync::MaybeSend;
use rs_matter_embassy::matter::with;

use super::light::{light, update_light};

pub(super) struct DimmableLight;

/// Each level `rs-matter` sets, for [`DimmableLight::run`] to spot the one
/// the user settled on.
static LEVEL_SET: Signal<CriticalSectionRawMutex, u8> = Signal::new();
/// The lamp is off with its saved brightness possibly at the minimum: put
/// back the chosen one (see [`DimmableLight::run`]).
pub(super) static RESTORE_LEVEL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
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
    /// Start at half brightness, like `LightState::DEFAULT`.
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
    async fn run<F: Fn(OutOfBandMessage) + MaybeSend>(&self, notify: F) {
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
                Either3::Second(()) => notify(OutOfBandMessage::Update(chosen)),
                Either3::Third(()) => {
                    if let Some(level) = settling.take() {
                        if (light().on || first) && level > Self::MIN_LEVEL {
                            chosen = level;
                        }
                        first = false;
                    }
                }
            }
        }
    }
}
