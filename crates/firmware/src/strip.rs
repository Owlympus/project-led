//! The WS2805 strip, driven by the RMT peripheral.
//!
//! Same chain as the on-board LED (`led_core` → `ws2805` → RMT), with 40 bits
//! per pixel. The RMT needs all the codes of a frame in one array: for 30
//! pixels that is 1201 codes × 4 bytes ≈ 4.8 KB. It is too big for the stack,
//! so the caller provides a `'static` buffer sized with [`codes_len`].

use esp_hal::Async;
use esp_hal::rmt::{Channel, Error, PulseCode, Tx};
use led_core::Rgbcct;
use ws2805::{Timing, WhiteOrder, pulses, strip_bytes};

use crate::pulse_code::fill;

/// Bits sent per WS2805 pixel (5 channels × 8 bits).
const BITS_PER_PIXEL: usize = 40;

/// Number of RMT codes needed to send `pixels` pixels: one per bit, plus the
/// end marker. `const fn` so it can size a `static` array.
pub const fn codes_len(pixels: usize) -> usize {
    pixels * BITS_PER_PIXEL + 1
}

/// A WS2805 strip.
pub struct Strip<'ch> {
    channel: Channel<'ch, Async, Tx>,
    tick_hz: u32,
    whites: WhiteOrder,
    /// Where each frame is encoded before the RMT sends it.
    codes: &'static mut [PulseCode],
}

impl<'ch> Strip<'ch> {
    /// `channel` must be an RMT TX channel wired (through the level shifter)
    /// to the strip's DIN. `codes` must hold at least
    /// `codes_len(number of pixels)` codes.
    pub fn new(
        channel: Channel<'ch, Async, Tx>,
        tick_hz: u32,
        whites: WhiteOrder,
        codes: &'static mut [PulseCode],
    ) -> Self {
        Self {
            channel,
            tick_hz,
            whites,
            codes,
        }
    }

    /// How many pixels the buffer can hold.
    pub fn capacity(&self) -> usize {
        (self.codes.len() - 1) / BITS_PER_PIXEL
    }

    /// Lights the whole strip (all [`Strip::capacity`] pixels) with one color,
    /// without needing an array of pixels. The pixel is sent as is: apply
    /// brightness and gamma correction before (see `led_core`).
    ///
    /// Wait at least `Timing::WS2805.reset_us` before the next frame.
    pub async fn show_all(&mut self, pixel: Rgbcct) -> Result<(), Error> {
        self.send(core::iter::repeat_n(pixel, self.capacity()))
            .await
    }

    /// Sends one color per pixel (effects, status signals). Pixels beyond
    /// [`Strip::capacity`] are ignored. As for [`Strip::show_all`], apply
    /// brightness and gamma correction before.
    pub async fn show(&mut self, pixels: &[Rgbcct]) -> Result<(), Error> {
        // `copied()`: the iterator gives `&Rgbcct`, `strip_bytes` wants
        // `Rgbcct` values (cheap to copy: 5 bytes).
        let capacity = self.capacity();
        self.send(pixels.iter().copied().take(capacity)).await
    }

    /// Encodes `pixels` into the code buffer and sends it.
    async fn send(&mut self, pixels: impl Iterator<Item = Rgbcct>) -> Result<(), Error> {
        let wire = pulses(strip_bytes(pixels, self.whites), Timing::WS2805);
        let used = fill(self.codes, wire, self.tick_hz);
        self.channel.transmit(&self.codes[..used]).await
    }
}
