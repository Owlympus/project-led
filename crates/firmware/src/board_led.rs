//! The RGB LED soldered on the dev board: a WS2812 on GPIO8, driven by the
//! RMT peripheral.
//!
//! It speaks the same kind of protocol as the WS2805 strip (3 bytes instead
//! of 5, slightly different timings), so it is a dress rehearsal for the
//! strip driver: `led_core` color → `ws2805` encoding → RMT → LED.

use esp_hal::Async;
use esp_hal::gpio::Level;
use esp_hal::rmt::{Channel, Error, PulseCode, Tx};
use led_core::Rgb;
use ws2805::{Pulse, Timing, pulses, ws2812_bytes};

/// 24 bits (one pulse code per bit) + 1 end marker telling the RMT to stop.
const CODES: usize = 24 + 1;

/// The on-board LED.
pub struct BoardLed<'ch> {
    channel: Channel<'ch, Async, Tx>,
    /// RMT clock frequency: pulse lengths are counted in ticks of this clock.
    tick_hz: u32,
}

impl<'ch> BoardLed<'ch> {
    /// `channel` must be an RMT TX channel already wired to the LED pin.
    pub fn new(channel: Channel<'ch, Async, Tx>, tick_hz: u32) -> Self {
        Self { channel, tick_hz }
    }

    /// Sends `color` to the LED. The color is sent as is: apply brightness
    /// and gamma correction before (see `led_core`).
    ///
    /// The line then stays low, which latches the color. Wait at least
    /// `Timing::WS2812.reset_us` before sending the next one.
    pub async fn show(&mut self, color: Rgb) -> Result<(), Error> {
        // The array starts full of end markers; we overwrite the first 24
        // codes, so the last one stays an end marker.
        let mut codes = [PulseCode::end_marker(); CODES];
        let wire = pulses(ws2812_bytes(color), Timing::WS2812);
        // `zip` walks both sequences side by side and stops at the shorter one.
        for (code, pulse) in codes.iter_mut().zip(wire) {
            *code = to_code(pulse, self.tick_hz);
        }
        self.channel.transmit(&codes).await
    }
}

/// A `ws2805::Pulse` (nanoseconds) as an RMT pulse code (ticks): high, then low.
fn to_code(pulse: Pulse, tick_hz: u32) -> PulseCode {
    let (high, low) = pulse.to_ticks(tick_hz);
    PulseCode::new(Level::High, high, Level::Low, low)
}
