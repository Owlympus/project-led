//! The lamp's local touch button: a TTP223 touch sensor (step 4d, ADR-0012).
//!
//! The TTP223 does the touch detection itself and gives us a plain digital
//! signal: high while a finger is on the pad, low otherwise (its default
//! "momentary, active high" mode). For us it is just a push button.
//!
//! Two gestures:
//! - **a tap** switches the lamp on or off. The request goes through Matter
//!   (see `matter::BUTTON_TOGGLE`), so the Home app sees the change and the
//!   usual fade applies;
//! - **held while the lamp powers up**, for [`RESET_HOLD`]: forgets the
//!   pairing (see [`held_at_startup`]).

use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::Input;
use log::{info, warn};

use crate::matter;

/// How long the button must stay held at power-up to forget the pairing.
/// Long enough not to happen by accident.
pub const RESET_HOLD: Duration = Duration::from_secs(5);

/// Ignore the signal for this long after each change: protects against a
/// finger that slides or bounces on the pad.
const DEBOUNCE: Duration = Duration::from_millis(50);

/// Checks, right at power-up, whether the button is held for [`RESET_HOLD`].
/// Returns `true` if so: the caller then erases Matter's saved data.
///
/// Called before the other tasks start, so a plain wait is fine here.
pub async fn held_at_startup(button: &mut Input<'_>) -> bool {
    if button.is_low() {
        return false;
    }
    warn!(
        "button held at startup: keep it held {} s to forget the pairing",
        RESET_HOLD.as_secs()
    );
    let start = Instant::now();
    while start.elapsed() < RESET_HOLD {
        if button.is_low() {
            info!("button released: pairing kept");
            return false;
        }
        Timer::after(DEBOUNCE).await;
    }
    warn!("button held long enough: the pairing will be forgotten");
    true
}

/// Turns each tap into an on/off toggle, forever.
#[embassy_executor::task]
pub async fn button_task(mut button: Input<'static>) {
    loop {
        // If the button is still held (after the startup check, or from the
        // last tap), wait for it to be released first: one touch, one toggle.
        button.wait_for_low().await;
        Timer::after(DEBOUNCE).await;

        // Sleeps until a finger touches the pad (no polling: the GPIO
        // interrupt wakes the task up).
        button.wait_for_high().await;
        info!("button: toggle");
        matter::BUTTON_TOGGLE.signal(());
        Timer::after(DEBOUNCE).await;
    }
}
