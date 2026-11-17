//! Turning pixels into what goes on the wire: bytes, then bits, then pulses.
//!
//! Every step is an *iterator*: values are produced one at a time, when the
//! sender asks for them. Nothing is stored in a big buffer, which matters on a
//! chip with little memory (90 pixels × 40 bits = 3600 pulses).
//!
//! Typical use, for a whole strip:
//! ```
//! use led_core::Rgbcct;
//! use ws2805::{Timing, WhiteOrder, pulses, strip_bytes};
//!
//! let pixels = [Rgbcct::new(255, 0, 0, 0, 0); 3];
//! let wire = pulses(strip_bytes(&pixels, WhiteOrder::WarmFirst), Timing::WS2805);
//! assert_eq!(wire.count(), 3 * 40);
//! ```

use led_core::{Rgb, Rgbcct};

use crate::timing::{Pulse, Timing};

/// Which white channel the strip expects first.
///
/// The WS2805 chip receives R, G, B, W1, W2, but which of W1 and W2 is wired
/// to the warm white LED depends on the strip maker: to check on the real strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteOrder {
    /// W1 = warm, W2 = cold.
    #[default]
    WarmFirst,
    /// W1 = cold, W2 = warm.
    ColdFirst,
}

/// The 5 bytes of one WS2805 pixel, in the order they are sent.
pub const fn ws2805_bytes(pixel: Rgbcct, whites: WhiteOrder) -> [u8; 5] {
    let (w1, w2) = match whites {
        WhiteOrder::WarmFirst => (pixel.warm, pixel.cold),
        WhiteOrder::ColdFirst => (pixel.cold, pixel.warm),
    };
    [pixel.r, pixel.g, pixel.b, w1, w2]
}

/// The 3 bytes of one WS2812 pixel (the dev board LED): green comes first.
pub const fn ws2812_bytes(color: Rgb) -> [u8; 3] {
    [color.g, color.r, color.b]
}

/// All the bytes of a WS2805 strip, first pixel first.
pub fn strip_bytes(pixels: &[Rgbcct], whites: WhiteOrder) -> impl Iterator<Item = u8> {
    pixels
        .iter()
        .flat_map(move |&pixel| ws2805_bytes(pixel, whites))
}

/// The bits of `bytes`, most significant bit first (`true` = 1).
pub fn bits(bytes: impl IntoIterator<Item = u8>) -> impl Iterator<Item = bool> {
    bytes
        .into_iter()
        .flat_map(|byte| (0..8).rev().map(move |shift| byte & (1 << shift) != 0))
}

/// The pulses that send `bytes` with the given chip timing.
pub fn pulses(bytes: impl IntoIterator<Item = u8>, timing: Timing) -> impl Iterator<Item = Pulse> {
    bits(bytes).map(move |bit| timing.pulse(bit))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIXEL: Rgbcct = Rgbcct::new(1, 2, 3, 4, 5);

    #[test]
    fn ws2805_bytes_are_rgb_then_whites() {
        assert_eq!(ws2805_bytes(PIXEL, WhiteOrder::WarmFirst), [1, 2, 3, 4, 5]);
        assert_eq!(ws2805_bytes(PIXEL, WhiteOrder::ColdFirst), [1, 2, 3, 5, 4]);
    }

    #[test]
    fn ws2812_bytes_start_with_green() {
        assert_eq!(ws2812_bytes(Rgb::new(10, 20, 30)), [20, 10, 30]);
    }

    #[test]
    fn strip_bytes_follow_the_pixels() {
        let pixels = [PIXEL, Rgbcct::OFF];
        let bytes: Vec<u8> = strip_bytes(&pixels, WhiteOrder::WarmFirst).collect();
        assert_eq!(bytes, [1, 2, 3, 4, 5, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn bits_are_most_significant_first() {
        let bits: Vec<bool> = bits([0b1010_0001]).collect();
        assert_eq!(bits, [true, false, true, false, false, false, false, true]);
    }

    #[test]
    fn one_pixel_is_40_bits() {
        assert_eq!(bits(ws2805_bytes(PIXEL, WhiteOrder::WarmFirst)).count(), 40);
        assert_eq!(bits(ws2812_bytes(Rgb::WHITE)).count(), 24);
    }

    #[test]
    fn pulses_follow_the_bits() {
        let t = Timing::WS2805;
        let wire: Vec<Pulse> = pulses([0b1000_0001], t).collect();
        assert_eq!(wire.len(), 8);
        assert_eq!(wire[0], t.one);
        assert!(wire[1..7].iter().all(|&pulse| pulse == t.zero));
        assert_eq!(wire[7], t.one);
    }

    #[test]
    fn black_pixel_is_only_zeros() {
        let t = Timing::WS2805;
        let all_zero = pulses(ws2805_bytes(Rgbcct::OFF, WhiteOrder::WarmFirst), t)
            .all(|pulse| pulse == t.zero);
        assert!(all_zero);
    }
}
