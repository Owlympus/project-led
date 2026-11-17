//! Hue / saturation / value colors, and their conversion to RGB.
//!
//! This is how a color wheel works (and how Matter's Color Control cluster
//! describes colors): the *hue* is the angle on the wheel, the *saturation*
//! goes from washed-out (0) to vivid (255), and the *value* is the brightness.

use crate::color::{Rgb, scale8};

/// A color as hue, saturation and value (brightness).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Hsv {
    /// Angle on the color wheel: the full `u16` range maps to 0°..360°,
    /// so 0 = red, ~21845 = green, ~43690 = blue, and it wraps around.
    pub hue: u16,
    /// 0 = gray/white, 255 = fully saturated color.
    pub saturation: u8,
    /// 0 = off, 255 = full brightness.
    pub value: u8,
}

impl Hsv {
    pub const fn new(hue: u16, saturation: u8, value: u8) -> Self {
        Self {
            hue,
            saturation,
            value,
        }
    }

    /// Creates a color from a hue in degrees (any value; 360 wraps to 0).
    pub const fn from_degrees(degrees: u16, saturation: u8, value: u8) -> Self {
        let degrees = (degrees % 360) as u32;
        // Rounded `degrees * 65536 / 360`; the result is always < 65536.
        let hue = ((degrees * 65_536 + 180) / 360) as u16;
        Self::new(hue, saturation, value)
    }
}

/// Converts with `Rgb::from(hsv)` or `let rgb: Rgb = hsv.into();`.
impl From<Hsv> for Rgb {
    fn from(hsv: Hsv) -> Self {
        let Hsv {
            hue,
            saturation: s,
            value: v,
        } = hsv;

        // Split the wheel into 6 sectors of 60°: red→yellow, yellow→green, …
        let h6 = hue as u32 * 6;
        let sector = h6 >> 16; // 0..=5
        let fraction = ((h6 & 0xFFFF) >> 8) as u8; // position inside the sector, 0..=255

        // The three possible channel levels inside a sector:
        let bottom = scale8(v, 255 - s); // the "missing" color
        let falling = scale8(v, 255 - scale8(s, fraction)); // going down across the sector
        let rising = scale8(v, 255 - scale8(s, 255 - fraction)); // going up across the sector

        let (r, g, b) = match sector {
            0 => (v, rising, bottom),
            1 => (falling, v, bottom),
            2 => (bottom, v, rising),
            3 => (bottom, falling, v),
            4 => (rising, bottom, v),
            _ => (v, bottom, falling), // 5
        };
        Rgb::new(r, g, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(degrees: u16, saturation: u8, value: u8) -> Rgb {
        Hsv::from_degrees(degrees, saturation, value).into()
    }

    #[test]
    fn primary_and_secondary_colors() {
        assert_eq!(rgb(0, 255, 255), Rgb::new(255, 0, 0), "red");
        assert_eq!(rgb(60, 255, 255), Rgb::new(255, 255, 0), "yellow");
        assert_eq!(rgb(120, 255, 255), Rgb::new(0, 255, 0), "green");
        assert_eq!(rgb(180, 255, 255), Rgb::new(0, 255, 255), "cyan");
        assert_eq!(rgb(240, 255, 255), Rgb::new(0, 0, 255), "blue");
        assert_eq!(rgb(300, 255, 255), Rgb::new(255, 0, 255), "magenta");
    }

    #[test]
    fn hue_wraps_around() {
        assert_eq!(
            Hsv::from_degrees(360, 255, 255),
            Hsv::from_degrees(0, 255, 255)
        );
        assert_eq!(
            Hsv::from_degrees(480, 255, 255),
            Hsv::from_degrees(120, 255, 255)
        );
    }

    #[test]
    fn zero_saturation_is_gray() {
        for degrees in [0, 90, 200, 359] {
            assert_eq!(rgb(degrees, 0, 200), Rgb::new(200, 200, 200));
        }
    }

    #[test]
    fn zero_value_is_black() {
        assert_eq!(rgb(123, 255, 0), Rgb::BLACK);
    }

    #[test]
    fn half_saturation_is_pastel() {
        assert_eq!(rgb(0, 128, 255), Rgb::new(255, 127, 127));
    }

    #[test]
    fn orange_is_between_red_and_yellow() {
        let orange = rgb(30, 255, 255);
        assert_eq!((orange.r, orange.b), (255, 0));
        assert!(orange.g.abs_diff(128) <= 2, "got {orange:?}");
    }
}
