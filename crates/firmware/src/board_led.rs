//! The RGB LED soldered on the dev board: a WS2812 on GPIO8, driven by the
//! RMT peripheral.
//!
//! It speaks the same kind of protocol as the WS2805 strip (3 bytes instead
//! of 5, slightly different timings), so it is a dress rehearsal for the
//! strip driver: `led_core` color → `ws2805` encoding → RMT → LED.

use esp_hal::Async;
use esp_hal::rmt::{Channel, Error, PulseCode, Tx};
use led_core::Rgb;
use ws2805::{Timing, pulses, ws2812_bytes};

use crate::pulse_code::fill;

/// 24 bits (one pulse code per bit) + 1 end marker.
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
        // Small enough (100 bytes) to live on the stack.
        let mut codes = [PulseCode::end_marker(); CODES];
        let used = fill(
            &mut codes,
            pulses(ws2812_bytes(color), Timing::WS2812),
            self.tick_hz,
        );
        self.channel.transmit(&codes[..used]).await
    }
}
