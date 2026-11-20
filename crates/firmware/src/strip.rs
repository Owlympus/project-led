//! The WS2805 strip, driven by the SPI peripheral and its DMA (ADR-0017).
//!
//! Each WS2805 bit becomes 4 SPI bits (see `ws2805::spi`), and a whole frame
//! (840 bytes for 30 pixels, resets included) is prepared in one buffer. The
//! DMA then sends it by itself: the CPU is free during the ~2.6 ms of the
//! transfer, and nothing can cut the signal halfway.
//!
//! (The RMT, first used here, has room for only 48 pulses and relies on our
//! task to refill it every ~30 µs: with Matter and the Wi-Fi around, frames
//! took ~100 ms and would have shown wrong pixels.)

use esp_hal::Async;
use esp_hal::spi::Error;
use esp_hal::spi::master::SpiDma;
use led_core::Rgbcct;
use ws2805::{WhiteOrder, spi, strip_bytes};

/// A WS2805 strip.
pub struct Strip<'d> {
    spi: SpiDma<'d, Async>,
    whites: WhiteOrder,
    /// Where each frame is encoded before the DMA sends it.
    buffer: &'static mut [u8],
}

impl<'d> Strip<'d> {
    /// `spi` must send at `ws2805::spi::SPI_FREQUENCY_HZ`, its data pin
    /// (MOSI) wired through the level shifter to the strip's DIN. `buffer`
    /// must hold at least `ws2805::spi::buffer_len(number of pixels)` bytes.
    pub fn new(spi: SpiDma<'d, Async>, whites: WhiteOrder, buffer: &'static mut [u8]) -> Self {
        Self {
            spi,
            whites,
            buffer,
        }
    }

    /// How many pixels the buffer can hold.
    pub fn capacity(&self) -> usize {
        (self.buffer.len() - 2 * spi::RESET_BYTES) / spi::BYTES_PER_PIXEL
    }

    /// Lights the whole strip (all [`Strip::capacity`] pixels) with one color,
    /// without needing an array of pixels. The pixel is sent as is: apply
    /// brightness and gamma correction before (see `led_core`).
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

    /// Encodes `pixels` into the buffer and lets the DMA send it.
    async fn send(&mut self, pixels: impl Iterator<Item = Rgbcct>) -> Result<(), Error> {
        let used = spi::fill(self.buffer, strip_bytes(pixels, self.whites));
        self.spi.write_async(&self.buffer[..used]).await
    }
}
