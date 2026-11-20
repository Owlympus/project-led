//! Sending the strip's bits through an SPI peripheral (with DMA) instead of
//! the RMT (ADR-0017).
//!
//! The trick: the SPI sends bits at a fixed rate, so each WS2805 bit becomes
//! **4 SPI bits** at [`SPI_FREQUENCY_HZ`] (3.2 MHz, 312.5 ns per SPI bit):
//!
//! | WS2805 bit | SPI bits | Line high | Line low |
//! |---|---|---|---|
//! | 0 | `1000` | 312.5 ns | 937.5 ns |
//! | 1 | `1100` | 625 ns   | 625 ns   |
//!
//! Both fit the datasheet ranges (see `HARDWARE.md`), and a whole frame sits
//! in one buffer that the DMA sends by itself, without the CPU: nothing can
//! interrupt the signal halfway, unlike with the RMT's small memory.

/// The SPI clock that makes 4 SPI bits last one WS2805 bit (1.25 µs).
/// 80 MHz / 25 on the ESP32-C6: an exact divider.
pub const SPI_FREQUENCY_HZ: u32 = 3_200_000;

/// SPI bytes per WS2805 pixel: 5 bytes × 8 bits × 4 SPI bits / 8.
pub const BYTES_PER_PIXEL: usize = 5 * 4;

/// Bytes at 0 (line low) around each frame: 120 × 8 × 312.5 ns = 300 µs,
/// more than the 280 µs "reset" the WS2805 needs. Sent **before** the frame,
/// so the strip starts a clean one whatever the line did in between, and
/// **after**, so it displays it.
pub const RESET_BYTES: usize = 120;

/// The size of the buffer for a strip of `pixels` pixels. A `const fn`: it
/// can size a `static` array.
pub const fn buffer_len(pixels: usize) -> usize {
    RESET_BYTES + pixels * BYTES_PER_PIXEL + RESET_BYTES
}

/// The 4 SPI bits for one WS2805 bit (in the low nibble).
const fn nibble(bit: bool) -> u8 {
    if bit { 0b1100 } else { 0b1000 }
}

/// One WS2805 byte as 4 SPI bytes, most significant bit first (like the
/// WS2805 and like SPI in its usual mode).
pub const fn encode_byte(byte: u8) -> [u8; 4] {
    let mut out = [0u8; 4];
    let mut i = 0;
    // `while`, not `for`: `for` loops aren't allowed in a `const fn` yet.
    while i < 4 {
        // Two WS2805 bits per SPI byte: bits 7-6 first, then 5-4…
        let high = (byte >> (7 - 2 * i)) & 1 == 1;
        let low = (byte >> (6 - 2 * i)) & 1 == 1;
        out[i] = (nibble(high) << 4) | nibble(low);
        i += 1;
    }
    out
}

/// Fills `buffer` with a whole frame: reset, the encoded `bytes` (from
/// [`strip_bytes`](crate::strip_bytes)), reset. Returns how many bytes were
/// written, the length to send.
///
/// Panics if `buffer` is too short: size it with [`buffer_len`].
pub fn fill(buffer: &mut [u8], bytes: impl Iterator<Item = u8>) -> usize {
    buffer[..RESET_BYTES].fill(0);
    let mut used = RESET_BYTES;
    for byte in bytes {
        buffer[used..used + 4].copy_from_slice(&encode_byte(byte));
        used += 4;
    }
    buffer[used..used + RESET_BYTES].fill(0);
    used + RESET_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WhiteOrder, strip_bytes};
    use led_core::Rgbcct;

    /// How long the line stays high, then low, for a 4-bit SPI pattern.
    fn pattern_ns(nibble: u8) -> (u32, u32) {
        let spi_bit_ns = 1_000_000_000 / SPI_FREQUENCY_HZ as u64; // 312
        let highs = (0..4).filter(|i| nibble >> (3 - i) & 1 == 1).count() as u64;
        (
            (highs * spi_bit_ns) as u32,
            ((4 - highs) * spi_bit_ns) as u32,
        )
    }

    #[test]
    fn bits_fit_the_ws2805_datasheet() {
        // Datasheet: T0H 220-380 ns, T1H 580-1000 ns, low 580-1000 ns,
        // period ≥ 1250 ns (with 312.5 ns rounded down to 312 here).
        let (high, low) = pattern_ns(nibble(false));
        assert!((220..=380).contains(&high), "T0H {high}");
        assert!((580..=1000).contains(&low), "T0L {low}");
        let (high, low) = pattern_ns(nibble(true));
        assert!((580..=1000).contains(&high), "T1H {high}");
        assert!((580..=1000).contains(&low), "T1L {low}");
        assert!(high + low >= 1248, "period");
    }

    #[test]
    fn encodes_bytes_most_significant_bit_first() {
        assert_eq!(encode_byte(0x00), [0x88; 4], "all zeros: 1000 1000…");
        assert_eq!(encode_byte(0xFF), [0xCC; 4], "all ones: 1100 1100…");
        assert_eq!(encode_byte(0b1000_0000), [0xC8, 0x88, 0x88, 0x88]);
        assert_eq!(encode_byte(0b0000_0001), [0x88, 0x88, 0x88, 0x8C]);
    }

    #[test]
    fn a_frame_is_surrounded_by_resets() {
        let pixels = [Rgbcct::new(0xFF, 0, 0, 0, 0); 2];
        let mut buffer = [0xAA; buffer_len(2)];
        let used = fill(&mut buffer, strip_bytes(pixels, WhiteOrder::WarmFirst));

        assert_eq!(used, buffer_len(2));
        assert_eq!(&buffer[..RESET_BYTES], &[0; RESET_BYTES], "reset before");
        assert_eq!(
            &buffer[used - RESET_BYTES..used],
            &[0; RESET_BYTES],
            "reset after"
        );
        // First pixel, red byte 0xFF, then green 0x00.
        assert_eq!(&buffer[RESET_BYTES..RESET_BYTES + 4], &[0xCC; 4]);
        assert_eq!(&buffer[RESET_BYTES + 4..RESET_BYTES + 8], &[0x88; 4]);
    }

    #[test]
    fn resets_last_long_enough() {
        let reset_ns = RESET_BYTES as u64 * 8 * 1_000_000_000 / SPI_FREQUENCY_HZ as u64;
        assert!(reset_ns > 280_000, "{reset_ns} ns");
    }

    #[test]
    fn the_whole_strip_fits_a_small_buffer() {
        assert_eq!(buffer_len(30), 840, "30 pixels: 600 bytes + 2 × 120");
    }
}
