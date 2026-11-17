//! The state of a whole lamp (on/off, brightness, color), and how to turn it
//! into the pixel to show on the strip.
//!
//! This mirrors Matter's three lighting clusters (On/Off, Level Control and
//! Color Control) without depending on Matter: the [`matter_scale`] module
//! converts Matter's values (mostly `0..=254`) to ours (`0..=255`).

use crate::ambiance::Ambiance;
use crate::color::{Rgb, Rgbcct};
use crate::hsv::Hsv;
use crate::white::{ColorTemperature, WhiteChannels};

/// Which kind of color the lamp shows: a saturated color from the RGB LEDs,
/// or a white from the two white LEDs. Matter calls this the *color mode*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightColor {
    /// A color from the wheel. `hue` and `saturation` use the same scales as
    /// [`Hsv`] (full `u16` range for the hue, 255 = vivid).
    HueSaturation { hue: u16, saturation: u8 },
    /// A white at a given temperature, made by mixing the warm and cold LEDs.
    Temperature(ColorTemperature),
}

/// Everything the Home app can set on the lamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightState {
    pub on: bool,
    /// 0 = darkest, 255 = full brightness. Kept while the lamp is off, so it
    /// comes back at the same level.
    pub brightness: u8,
    pub color: LightColor,
    /// An ambiance shown instead of `color` (still at `brightness`), or
    /// `None` for the plain color.
    pub ambiance: Option<Ambiance>,
}

impl LightState {
    /// The state at power-up: off, ready to switch on to a warm white
    /// (3000 K) at half brightness.
    pub const DEFAULT: LightState = LightState {
        on: false,
        brightness: 128,
        color: LightColor::Temperature(ColorTemperature::from_kelvin(3000)),
        ambiance: None,
    };

    /// The pixel to send to the strip, before gamma correction.
    ///
    /// `whites` describes the strip's white LEDs: they are needed to mix a
    /// temperature, and they differ from one strip to another.
    pub fn pixel(&self, whites: &WhiteChannels) -> Rgbcct {
        if !self.on {
            return Rgbcct::OFF;
        }
        // `match` on an enum must handle every variant: if we add one later
        // (XY colors, say), the compiler will point us here.
        let full = match self.color {
            LightColor::HueSaturation { hue, saturation } => {
                let rgb: Rgb = Hsv::new(hue, saturation, 255).into();
                Rgbcct::from_rgb(rgb)
            }
            LightColor::Temperature(temperature) => Rgbcct::from_white(whites.mix(temperature)),
        };
        full.scale(self.brightness)
    }

    /// Fills `frame` with what the whole strip should show, before gamma
    /// correction: off, the ambiance, or the plain color everywhere.
    ///
    /// `ambiance_elapsed_ms` is the time since the ambiance started, for the
    /// animated ones (ignored otherwise).
    pub fn render(&self, whites: &WhiteChannels, ambiance_elapsed_ms: u32, frame: &mut [Rgbcct]) {
        match (self.on, self.ambiance) {
            (true, Some(ambiance)) => {
                ambiance.render(ambiance_elapsed_ms, frame);
                for pixel in frame.iter_mut() {
                    *pixel = pixel.scale(self.brightness);
                }
            }
            // Off (with or without ambiance), or a plain color.
            _ => frame.fill(self.pixel(whites)),
        }
    }

    /// Whether the frames from [`render`](Self::render) change over time.
    pub fn is_animated(&self) -> bool {
        self.on && self.ambiance.is_some_and(Ambiance::is_animated)
    }
}

impl Default for LightState {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Conversions from Matter's scales to ours.
///
/// Matter stores brightness (`CurrentLevel`), hue (`CurrentHue`) and
/// saturation (`CurrentSaturation`) on `0..=254`, because 255 means
/// "no value" in its protocol. Our types use the full `0..=255` (or the full
/// `u16` for the hue), so we stretch the values.
pub mod matter_scale {
    /// The highest value of Matter's `0..=254` scales.
    const MATTER_MAX: u32 = 254;

    /// Stretches `0..=254` to `0..=255`, rounded; above 254 counts as 254.
    const fn stretch(value: u8) -> u8 {
        let value = if value as u32 > MATTER_MAX {
            MATTER_MAX
        } else {
            value as u32
        };
        ((value * 255 + MATTER_MAX / 2) / MATTER_MAX) as u8
    }

    /// Matter `CurrentLevel` (1..=254 for a light) → our brightness.
    /// A non-zero level always gives a non-zero brightness.
    pub const fn brightness(level: u8) -> u8 {
        stretch(level)
    }

    /// Matter `CurrentSaturation` (0..=254) → our saturation (0..=255).
    pub const fn saturation(saturation: u8) -> u8 {
        stretch(saturation)
    }

    /// Matter `CurrentHue` (0..=254, where 254 is a full turn of the wheel)
    /// → our hue (full `u16` range, see [`Hsv`](crate::Hsv)).
    pub const fn hue(hue: u8) -> u16 {
        let hue = if hue as u32 > MATTER_MAX {
            MATTER_MAX
        } else {
            hue as u32
        };
        // Rounded `hue * 65536 / 254`. 254 gives 65536, one full turn: it
        // wraps to 0 (red), which is the same color.
        (((hue << 16) + MATTER_MAX / 2) / MATTER_MAX) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A typical strip: 2700 K warm LED, 6500 K cold LED.
    const WHITES: WhiteChannels = WhiteChannels::new(
        ColorTemperature::from_kelvin(2700),
        ColorTemperature::from_kelvin(6500),
    );

    const RED_PIXEL: Rgbcct = Rgbcct::new(255, 0, 0, 0, 0);

    fn on(brightness: u8, color: LightColor) -> LightState {
        LightState {
            on: true,
            brightness,
            color,
            ambiance: None,
        }
    }

    #[test]
    fn off_is_black_whatever_the_color() {
        let state = LightState {
            on: false,
            ..on(
                255,
                LightColor::HueSaturation {
                    hue: 0,
                    saturation: 255,
                },
            )
        };
        assert_eq!(state.pixel(&WHITES), Rgbcct::OFF);
        assert_eq!(LightState::DEFAULT.pixel(&WHITES), Rgbcct::OFF);
    }

    #[test]
    fn full_red_uses_only_the_red_led() {
        let red = on(
            255,
            LightColor::HueSaturation {
                hue: 0,
                saturation: 255,
            },
        );
        assert_eq!(red.pixel(&WHITES), Rgbcct::new(255, 0, 0, 0, 0));
    }

    #[test]
    fn warm_white_uses_only_the_warm_led() {
        let warm = on(255, LightColor::Temperature(WHITES.warm));
        assert_eq!(warm.pixel(&WHITES), Rgbcct::new(0, 0, 0, 255, 0));
    }

    #[test]
    fn brightness_scales_the_color() {
        let half_red = on(
            128,
            LightColor::HueSaturation {
                hue: 0,
                saturation: 255,
            },
        );
        let pixel = half_red.pixel(&WHITES);
        assert_eq!((pixel.g, pixel.b, pixel.warm, pixel.cold), (0, 0, 0, 0));
        assert!(pixel.r.abs_diff(128) <= 1, "got {pixel:?}");
    }

    #[test]
    fn render_fills_the_strip_with_the_plain_color() {
        let warm = on(255, LightColor::Temperature(WHITES.warm));
        let mut frame = [Rgbcct::OFF; 4];
        warm.render(&WHITES, 0, &mut frame);
        assert_eq!(frame, [Rgbcct::new(0, 0, 0, 255, 0); 4]);
    }

    #[test]
    fn render_shows_the_ambiance_at_the_lamp_brightness() {
        let full = LightState {
            ambiance: Some(Ambiance::Sunset),
            ..on(255, LightColor::Temperature(WHITES.warm))
        };
        let half = LightState {
            brightness: 128,
            ..full
        };
        let mut full_frame = [Rgbcct::OFF; 5];
        let mut half_frame = [Rgbcct::OFF; 5];
        full.render(&WHITES, 0, &mut full_frame);
        half.render(&WHITES, 0, &mut half_frame);

        let mut sunset = [Rgbcct::OFF; 5];
        Ambiance::Sunset.render(0, &mut sunset);
        assert_eq!(full_frame, sunset);
        assert_eq!(half_frame[0], sunset[0].scale(128));
    }

    #[test]
    fn render_is_black_when_off_even_with_an_ambiance() {
        let state = LightState {
            on: false,
            ambiance: Some(Ambiance::Campfire),
            ..on(255, LightColor::Temperature(WHITES.warm))
        };
        let mut frame = [RED_PIXEL; 3];
        state.render(&WHITES, 500, &mut frame);
        assert_eq!(frame, [Rgbcct::OFF; 3]);
        assert!(!state.is_animated(), "nothing moves while off");
    }

    #[test]
    fn matter_brightness_covers_our_range() {
        assert_eq!(matter_scale::brightness(254), 255);
        assert_eq!(matter_scale::brightness(1), 1, "lowest level stays lit");
        assert_eq!(matter_scale::brightness(0), 0);
        assert_eq!(matter_scale::brightness(255), 255, "out of range");
    }

    #[test]
    fn matter_saturation_covers_our_range() {
        assert_eq!(matter_scale::saturation(0), 0);
        assert_eq!(matter_scale::saturation(127), 128);
        assert_eq!(matter_scale::saturation(254), 255);
    }

    #[test]
    fn matter_hue_matches_degrees() {
        // Matter hue → degrees is `hue * 360 / 254`.
        assert_eq!(matter_scale::hue(0), Hsv::from_degrees(0, 0, 0).hue);
        assert_eq!(matter_scale::hue(127), Hsv::from_degrees(180, 0, 0).hue);
        assert_eq!(matter_scale::hue(254), 0, "full turn wraps to red");
        let green = matter_scale::hue(85); // ≈ 120.5°
        assert!(green.abs_diff(Hsv::from_degrees(120, 0, 0).hue) < 300);
    }
}
