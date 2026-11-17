//! Effects: colors computed **pixel by pixel**, and changing over time.
//!
//! Like [`Transition`](crate::Transition), nothing here reads a clock: the
//! caller passes the time elapsed since the effect started. An effect fills
//! a slice of pixels (`&mut [Rgbcct]`), the frame to send to the strip; it
//! works on colors *before* gamma correction.
//!
//! A slice is a view on part of an array, whatever its size: the same code
//! works for 30 pixels on the lamp and 5 in the tests. (Close to passing a
//! PHP array by reference, but the size is fixed: an effect can change the
//! pixels, not add any.)

use crate::color::Rgbcct;

/// Position along the strip in 1/256ths of a pixel: the head of a moving
/// effect glides between two pixels instead of jumping from one to the next.
const SUBPIXELS: u32 = 256;

/// A light that sweeps back and forth along the strip, with a fading tail
/// on each side of its head (the "Larson scanner", as on K2000's car).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scanner {
    color: Rgbcct,
    /// Time for a full round trip (first pixel → last → first).
    period_ms: u32,
    /// How many pixels the light fades over, on each side of the head.
    tail: u8,
}

impl Scanner {
    /// A scanner of `color` doing a round trip every `period_ms`, fading
    /// over `tail` pixels on each side of its head (at least 1).
    pub const fn new(color: Rgbcct, period_ms: u32, tail: u8) -> Self {
        Self {
            color,
            period_ms: if period_ms == 0 { 1 } else { period_ms },
            tail: if tail == 0 { 1 } else { tail },
        }
    }

    /// Fills `pixels` with the frame at `elapsed_ms`. The head starts on the
    /// first pixel, reaches the last one at half the period, and comes back.
    pub fn render(&self, elapsed_ms: u32, pixels: &mut [Rgbcct]) {
        let head = self.head_position(elapsed_ms, pixels.len());
        let tail = self.tail as u32 * SUBPIXELS;

        // `iter_mut` hands out each pixel as `&mut Rgbcct` (we may change
        // it), `enumerate` adds its index: like `foreach ($pixels as $i => &$p)`.
        for (index, pixel) in pixels.iter_mut().enumerate() {
            let center = index as u32 * SUBPIXELS;
            let distance = center.abs_diff(head);
            *pixel = if distance >= tail {
                Rgbcct::OFF
            } else {
                // 255 on the head, fading linearly to 0 at `tail` pixels.
                let level = 255 - distance * 255 / tail;
                self.color.scale(level as u8)
            };
        }
    }

    /// Where the head is, in 1/256ths of a pixel from the first pixel.
    fn head_position(&self, elapsed_ms: u32, len: usize) -> u32 {
        if len <= 1 {
            return 0;
        }
        // From the center of the first pixel to the center of the last.
        let span = (len as u64 - 1) * SUBPIXELS as u64;
        let half = (self.period_ms as u64 / 2).max(1);
        let phase = (elapsed_ms % self.period_ms) as u64;

        // u64: `phase * span` can exceed a u32 for long periods.
        let position = if phase < half {
            phase * span / half // going towards the last pixel
        } else {
            span - (phase - half).min(half) * span / half // coming back
        };
        position as u32
    }
}

/// Whether a blinking light is on at `elapsed_ms`: on during the first half
/// of each period, off during the second.
pub fn blink_on(elapsed_ms: u32, period_ms: u32) -> bool {
    let period_ms = period_ms.max(2);
    elapsed_ms % period_ms < period_ms / 2
}

/// The color at `position` (0 = start, 65535 = end) of a gradient whose
/// `stops` are spread evenly from start to end and blended in between.
/// No stops: off.
pub fn gradient_at(stops: &[Rgbcct], position: u16) -> Rgbcct {
    // `match` on a slice: by number of elements, like a `switch (count())`.
    match stops {
        [] => Rgbcct::OFF,
        [only] => *only,
        _ => {
            const END: u32 = u16::MAX as u32;
            let segments = stops.len() as u32 - 1;
            // Which segment (between stop `index` and the next one), and
            // how far into it: `t` on the same 0..=65535 scale as `lerp`.
            let scaled = position as u32 * segments;
            let index = (scaled / END) as usize;
            let t = (scaled % END) as u16;
            match stops.get(index + 1) {
                Some(next) => stops[index].lerp(*next, t),
                None => stops[stops.len() - 1], // position == 65535
            }
        }
    }
}

/// Where pixel `index` of a strip of `len` pixels sits, on the 0..=65535
/// scale of [`gradient_at`]: the first pixel at 0, the last at 65535.
pub fn pixel_position(index: usize, len: usize) -> u16 {
    if len <= 1 {
        return 0;
    }
    let last = len as u32 - 1;
    let index = (index as u32).min(last);
    ((index * u16::MAX as u32 + last / 2) / last) as u16
}

/// Folds an ever-growing value back and forth over 0..=65535: 0 → 65535,
/// then back to 0, and so on. Moves a gradient without a jump when it
/// "wraps around".
pub fn ping_pong(value: u32) -> u16 {
    const ONE_WAY: u32 = 1 << 16;
    let phase = value % (2 * ONE_WAY);
    if phase < ONE_WAY {
        phase as u16
    } else {
        (2 * ONE_WAY - 1 - phase) as u16
    }
}

/// A smooth "random" value (0..=255) that drifts over time: a new random
/// target every `step_ms`, blended linearly in between. Each `seed` (one
/// per pixel, say) gives its own drift. Deterministic: the same inputs
/// always give the same value, which keeps it testable.
pub fn smooth_noise(seed: u32, elapsed_ms: u32, step_ms: u32) -> u8 {
    let step_ms = step_ms.max(1);
    let step = elapsed_ms / step_ms;
    let from = hash(seed, step);
    let to = hash(seed, step.wrapping_add(1));
    // Position between the two targets, on the 0..=65535 scale of `lerp`.
    let t = ((elapsed_ms % step_ms) as u64 * u16::MAX as u64 / step_ms as u64) as u32;
    let mixed = from as u32 * (u16::MAX as u32 - t) + to as u32 * t;
    (mixed / u16::MAX as u32) as u8
}

/// Mixes two numbers into a byte that looks random. There is no random
/// generator in `no_std`, and we don't need a real one: just values that
/// look unrelated to each other. (The steps of MurmurHash3's finalizer.)
fn hash(seed: u32, step: u32) -> u8 {
    let mut x = seed.wrapping_mul(0x9E37_79B1) ^ step;
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    (x >> 24) as u8
}

/// The lamp's status signals: what the strip shows when it has something to
/// say about its connection rather than the color asked by the Home app.
/// (See the idea box in the ROADMAP.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusSignal {
    /// Not paired yet (first start, or pairing forgotten): a **red** scanner.
    WaitingForPairing,
    /// Paired over Bluetooth, joining the Wi-Fi for the first time: a
    /// **blue** scanner.
    JoiningWifi,
    /// Wi-Fi lost: the **first pixel** blinks red, the others keep showing
    /// the lamp's color.
    WifiLost,
}

/// How bright the signals are: visible, without lighting up the room.
const SIGNAL_LEVEL: u8 = 128;
const SIGNAL_RED: Rgbcct = Rgbcct::new(SIGNAL_LEVEL, 0, 0, 0, 0);
const SIGNAL_BLUE: Rgbcct = Rgbcct::new(0, 0, SIGNAL_LEVEL, 0, 0);
/// A calm sweep: one round trip every 2 s, fading over 4 pixels (20 cm).
const SCANNER_PERIOD_MS: u32 = 2000;
const SCANNER_TAIL: u8 = 4;
/// The Wi-Fi alert blinks once per second.
const ALERT_PERIOD_MS: u32 = 1000;

impl StatusSignal {
    /// Draws the signal at `elapsed_ms` into `pixels`.
    ///
    /// The scanners replace the whole frame. [`StatusSignal::WifiLost`] only
    /// changes the first pixel: fill `pixels` with the lamp's normal frame
    /// first, then call this to draw the alert on top.
    pub fn render(self, elapsed_ms: u32, pixels: &mut [Rgbcct]) {
        match self {
            StatusSignal::WaitingForPairing => {
                Scanner::new(SIGNAL_RED, SCANNER_PERIOD_MS, SCANNER_TAIL)
                    .render(elapsed_ms, pixels);
            }
            StatusSignal::JoiningWifi => {
                Scanner::new(SIGNAL_BLUE, SCANNER_PERIOD_MS, SCANNER_TAIL)
                    .render(elapsed_ms, pixels);
            }
            StatusSignal::WifiLost => {
                // `first_mut` gives `None` for an empty slice: nothing to do.
                if let Some(first) = pixels.first_mut() {
                    *first = if blink_on(elapsed_ms, ALERT_PERIOD_MS) {
                        SIGNAL_RED
                    } else {
                        Rgbcct::OFF
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgbcct = Rgbcct::new(255, 0, 0, 0, 0);
    /// 1 s round trip over 5 pixels, fading over 2 pixels.
    const SCANNER: Scanner = Scanner::new(RED, 1000, 2);

    /// The red level of each pixel, easier to read in assertions.
    fn reds(pixels: &[Rgbcct]) -> [u8; 5] {
        let mut out = [0; 5];
        for (o, p) in out.iter_mut().zip(pixels) {
            *o = p.r;
        }
        out
    }

    fn frame(elapsed_ms: u32) -> [u8; 5] {
        let mut pixels = [Rgbcct::OFF; 5];
        SCANNER.render(elapsed_ms, &mut pixels);
        reds(&pixels)
    }

    #[test]
    fn scanner_starts_on_the_first_pixel() {
        assert_eq!(frame(0), [255, 128, 0, 0, 0]);
    }

    #[test]
    fn scanner_reaches_the_last_pixel_at_half_period() {
        assert_eq!(frame(500), [0, 0, 0, 128, 255]);
    }

    #[test]
    fn scanner_is_in_the_middle_at_a_quarter_and_three_quarters() {
        assert_eq!(frame(250), [0, 128, 255, 128, 0]);
        assert_eq!(frame(750), [0, 128, 255, 128, 0]);
    }

    #[test]
    fn scanner_glides_between_pixels() {
        // A 2 s round trip over 5 pixels: one pixel every 250 ms, so at
        // 125 ms the head is halfway between pixels 0 and 1.
        let mut pixels = [Rgbcct::OFF; 5];
        Scanner::new(RED, 2000, 2).render(125, &mut pixels);
        let [first, second, ..] = reds(&pixels);
        assert_eq!(first, second, "both pixels equally lit");
        assert!(first > 128 && first < 255, "got {first}");
    }

    #[test]
    fn scanner_repeats_every_period() {
        assert_eq!(frame(1250), frame(250));
    }

    #[test]
    fn scanner_handles_tiny_strips() {
        let mut empty: [Rgbcct; 0] = [];
        SCANNER.render(123, &mut empty); // must not panic
        let mut single = [Rgbcct::OFF];
        SCANNER.render(123, &mut single);
        assert_eq!(single, [RED]);
    }

    #[test]
    fn gradient_hits_its_stops() {
        let stops = [
            RED,
            Rgbcct::new(0, 255, 0, 0, 0),
            Rgbcct::new(0, 0, 255, 0, 0),
        ];
        assert_eq!(gradient_at(&stops, 0), stops[0]);
        assert_eq!(gradient_at(&stops, u16::MAX), stops[2]);
        // The middle pixel of a 5-pixel strip lands on the middle stop.
        assert_eq!(gradient_at(&stops, pixel_position(2, 5)), stops[1]);
    }

    #[test]
    fn gradient_blends_between_stops() {
        let stops = [Rgbcct::OFF, RED];
        assert_eq!(gradient_at(&stops, u16::MAX / 2).r, 127);
        assert_eq!(gradient_at(&[], 1234), Rgbcct::OFF);
        assert_eq!(gradient_at(&[RED], 1234), RED);
    }

    #[test]
    fn pixel_positions_span_the_scale() {
        assert_eq!(pixel_position(0, 30), 0);
        assert_eq!(pixel_position(29, 30), u16::MAX);
        assert_eq!(pixel_position(0, 1), 0);
        assert!(pixel_position(10, 30) < pixel_position(11, 30));
    }

    #[test]
    fn ping_pong_goes_and_comes_back() {
        assert_eq!(ping_pong(0), 0);
        assert_eq!(ping_pong(65_535), 65_535);
        assert_eq!(ping_pong(65_536), 65_535);
        assert_eq!(ping_pong(131_071), 0);
        assert_eq!(ping_pong(131_072), 0, "and again");
    }

    #[test]
    fn noise_is_smooth_and_deterministic() {
        assert_eq!(smooth_noise(7, 1234, 100), smooth_noise(7, 1234, 100));
        // At each step it sits exactly on the step's random value…
        assert_eq!(smooth_noise(7, 300, 100), hash(7, 3));
        // …and moves little from one millisecond to the next.
        for ms in 0..1000 {
            let a = smooth_noise(7, ms, 100);
            let b = smooth_noise(7, ms + 1, 100);
            assert!(a.abs_diff(b) <= 3, "{ms} ms: {a} → {b}");
        }
    }

    #[test]
    fn noise_differs_between_seeds() {
        let values: [u8; 4] = core::array::from_fn(|seed| smooth_noise(seed as u32, 0, 100));
        assert!(values.iter().any(|v| *v != values[0]), "got {values:?}");
    }

    #[test]
    fn blink_is_on_then_off() {
        assert!(blink_on(0, 1000));
        assert!(blink_on(499, 1000));
        assert!(!blink_on(500, 1000));
        assert!(!blink_on(999, 1000));
        assert!(blink_on(1000, 1000));
    }

    #[test]
    fn waiting_for_pairing_is_red_and_joining_wifi_is_blue() {
        let mut pixels = [Rgbcct::OFF; 5];
        StatusSignal::WaitingForPairing.render(0, &mut pixels);
        assert_eq!(pixels[0], SIGNAL_RED);

        StatusSignal::JoiningWifi.render(0, &mut pixels);
        assert_eq!(pixels[0], SIGNAL_BLUE);
    }

    #[test]
    fn wifi_lost_only_touches_the_first_pixel() {
        let warm = Rgbcct::new(0, 0, 0, 200, 0);
        let mut pixels = [warm; 5];

        StatusSignal::WifiLost.render(0, &mut pixels);
        assert_eq!(pixels[0], SIGNAL_RED);
        assert_eq!(&pixels[1..], &[warm; 4]);

        StatusSignal::WifiLost.render(600, &mut pixels);
        assert_eq!(pixels[0], Rgbcct::OFF, "blinking: off half the time");
        assert_eq!(&pixels[1..], &[warm; 4]);
    }
}
