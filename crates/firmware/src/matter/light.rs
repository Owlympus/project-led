//! The lamp's shared state, and the signals that carry its changes between
//! the Matter clusters, the strip task and the touch button.

use core::cell::Cell;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use led_core::{Ambiance, LightState};
use log::info;

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
/// `OnOffLight::run`).
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

/// One per ambiance switch (index: [`Ambiance::index`]): tells its On/Off
/// cluster to *show* on or off in the Home app, without switching anything
/// (see `AmbianceSwitch::run`).
///
/// `[const { … }; N]` builds an array of N values that can't be copied
/// (a `Signal` can't): the block is evaluated once per element.
pub(super) static AMBIANCE_SWITCH_SHOWS: [Signal<CriticalSectionRawMutex, bool>; Ambiance::COUNT] =
    [const { Signal::new() }; Ambiance::COUNT];

/// The lamp's state right now.
pub(super) fn light() -> LightState {
    LIGHT.lock(Cell::get)
}

/// Changes one part of [`LIGHT`] and tells the strip task. If that leaves
/// an ambiance (lamp switched off, new color, another ambiance), its switch
/// shows "off" in the Home app.
///
/// `impl FnOnce(&mut LightState)` means "any closure that modifies a
/// `LightState`", a bit like a PHP `callable` with a typed parameter.
///
/// Returns the state *before* the change, so the caller can see what it
/// replaced (was the lamp on?).
pub(super) fn update_light(change: impl FnOnce(&mut LightState)) -> LightState {
    let (before, after) = LIGHT.lock(|cell| {
        let before = cell.get();
        let mut after = before;
        change(&mut after);
        cell.set(after);
        (before, after)
    });
    LIGHT_CHANGED.signal(after);

    // `if let … && …`: both must hold (a "let chain").
    if let Some(left) = before.ambiance
        && after.ambiance != Some(left)
    {
        info!("ambiance off: {}", left.name());
        AMBIANCE_SWITCH_SHOWS[left.index()].signal(false);
    }
    before
}
