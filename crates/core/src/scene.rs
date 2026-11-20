//! The scene: everything the strip shows, frame after frame.
//!
//! It gathers the lamp's state (plain color or ambiance), the fade from the
//! previous picture to the new one, and the status signal drawn on top. Each
//! call to [`Scene::draw`] gives the next frame, gamma corrected, ready for
//! the strip.
//!
//! Like [`Transition`](crate::Transition), the scene reads no clock: the
//! caller passes the current time, `now_ms`, in milliseconds (from any
//! starting point, e.g. the chip's start). That keeps it testable here, on
//! the computer, with made-up times.
//!
//! `Scene<N>` is generic over the number of pixels `N` (a *const generic*,
//! like an array's length): 30 on the lamp, 5 in the tests.

use crate::color::Rgbcct;
use crate::effect::{StatusSignal, crossfade, frame_preview};
use crate::light::LightState;
use crate::transition::Easing;
use crate::white::WhiteChannels;

/// How long the scene's fades last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FadeTimes {
    /// For changes of the same picture (brightness, color): smooths
    /// Matter's steps and the Home app slider's jumps.
    pub smoothing_ms: u32,
    /// For a change of picture: entering, leaving or changing an ambiance.
    pub ambiance_ms: u32,
}

/// What the strip shows, and how it gets there.
#[derive(Debug, Clone)]
pub struct Scene<const N: usize> {
    whites: WhiteChannels,
    fades: FadeTimes,
    /// The lamp's latest state, and since when its ambiance runs.
    state: LightState,
    ambiance_start_ms: u32,
    /// The picture shown when the state last changed (before gamma, without
    /// the status signal): the new picture fades in from it.
    fade_from: [Rgbcct; N],
    fade_start_ms: u32,
    fade_ms: u32,
    /// Whether the last picture drawn was the fade's final one. Decided when
    /// drawing, never later: the caller may spend a long time sending the
    /// frame (~100 ms to the strip), and a clock read after that would end
    /// the fade before its final picture was drawn (seen on the board on
    /// 2026-11-20: the board LED stayed on the old color).
    fade_done: bool,
    /// The last picture drawn, same conventions as `fade_from`.
    shown: [Rgbcct; N],
    status: Option<StatusSignal>,
    status_start_ms: u32,
}

impl<const N: usize> Scene<N> {
    /// A scene showing nothing (off), for a strip whose white LEDs are
    /// `whites`. A `const fn`: it can build a scene at compile time, to put
    /// it in a `static` on the chip.
    pub const fn new(whites: WhiteChannels, fades: FadeTimes) -> Self {
        Self {
            whites,
            fades,
            state: LightState::DEFAULT,
            ambiance_start_ms: 0,
            fade_from: [Rgbcct::OFF; N],
            fade_start_ms: 0,
            fade_ms: 0,
            fade_done: true,
            shown: [Rgbcct::OFF; N],
            status: None,
            status_start_ms: 0,
        }
    }

    /// The lamp's state the scene shows (or fades towards).
    pub fn state(&self) -> &LightState {
        &self.state
    }

    /// A new state for the lamp, at `now_ms`. The picture fades to it from
    /// what is shown right now, so a new step in the middle of a fade
    /// doesn't jump.
    pub fn light(&mut self, state: &LightState, now_ms: u32) {
        self.fade_from = self.shown;
        self.fade_start_ms = now_ms;
        self.fade_done = false;
        // Small steps are only smoothed; entering or leaving an ambiance is
        // a real change of picture, faded more slowly.
        self.fade_ms = if state.ambiance == self.state.ambiance {
            self.fades.smoothing_ms
        } else {
            self.ambiance_start_ms = now_ms;
            self.fades.ambiance_ms
        };
        self.state = *state;
    }

    /// A new status signal at `now_ms`, or `None` when everything is fine
    /// again. Its animation starts from the beginning.
    pub fn status(&mut self, status: Option<StatusSignal>, now_ms: u32) {
        self.status = status;
        self.status_start_ms = now_ms;
    }

    /// Whether the frames after the last one drawn will differ from it: a
    /// fade not finished yet, an animated ambiance or a status signal.
    /// When `false`, the caller can sleep until something changes.
    pub fn is_moving(&self) -> bool {
        self.status.is_some() || self.state.is_animated() || !self.fade_done
    }

    /// Fills `frame` with the picture at `now_ms`, gamma corrected.
    pub fn draw(&mut self, now_ms: u32, frame: &mut [Rgbcct; N]) {
        // The lamp's picture, faded in from the previous one.
        self.state
            .render(&self.whites, since(self.ambiance_start_ms, now_ms), frame);
        // One reading of the time for both: the progress of this picture,
        // and whether it is the fade's last one.
        let elapsed = since(self.fade_start_ms, now_ms);
        let t = Easing::EaseInOut.progress(elapsed, self.fade_ms);
        crossfade(&self.fade_from, frame, t);
        self.fade_done = elapsed >= self.fade_ms;
        self.shown = *frame;

        // Then the status signal on top, and gamma last.
        if let Some(status) = self.status {
            status.render(since(self.status_start_ms, now_ms), frame);
        }
        for pixel in frame.iter_mut() {
            *pixel = pixel.gamma_corrected();
        }
    }

    /// One color summing up `frame` (as drawn at `now_ms`) for a single LED
    /// standing in for the strip. During a status signal, the signal's own
    /// single-LED version instead (a breathing light where the strip has a
    /// scanner), gamma corrected like the frame.
    pub fn preview(&self, frame: &[Rgbcct; N], now_ms: u32) -> Rgbcct {
        match self.status {
            Some(status) => status
                .single_led(since(self.status_start_ms, now_ms))
                .gamma_corrected(),
            None => frame_preview(frame),
        }
    }
}

/// Milliseconds from `start_ms` to `now_ms`. `wrapping_sub` stays right
/// when the u32 of milliseconds wraps around (every 49 days): 5 − 4294967291
/// "wraps" to 10, the true gap.
fn since(start_ms: u32, now_ms: u32) -> u32 {
    now_ms.wrapping_sub(start_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ambiance::Ambiance;
    use crate::light::LightColor;
    use crate::white::ColorTemperature;

    const WHITES: WhiteChannels = WhiteChannels::new(
        ColorTemperature::from_kelvin(2700),
        ColorTemperature::from_kelvin(6500),
    );
    const FADES: FadeTimes = FadeTimes {
        smoothing_ms: 200,
        ambiance_ms: 800,
    };
    const RED: LightColor = LightColor::HueSaturation {
        hue: 0,
        saturation: 255,
    };
    const GREEN: LightColor = LightColor::HueSaturation {
        hue: 21845,
        saturation: 255,
    };

    fn lit(color: LightColor) -> LightState {
        LightState {
            on: true,
            brightness: 255,
            color,
            ambiance: None,
        }
    }

    /// The frame the scene draws at `now_ms`.
    fn frame_at(scene: &mut Scene<5>, now_ms: u32) -> [Rgbcct; 5] {
        let mut frame = [Rgbcct::OFF; 5];
        scene.draw(now_ms, &mut frame);
        frame
    }

    /// The frame a state shows once settled, for comparisons.
    fn settled(state: &LightState) -> [Rgbcct; 5] {
        let mut scene = Scene::<5>::new(WHITES, FADES);
        scene.light(state, 0);
        frame_at(&mut scene, 10_000)
    }

    #[test]
    fn starts_off_and_still() {
        let mut scene = Scene::<5>::new(WHITES, FADES);
        assert_eq!(frame_at(&mut scene, 0), [Rgbcct::OFF; 5]);
        assert!(!scene.is_moving());
    }

    #[test]
    fn fades_to_a_new_color_and_stops_on_it() {
        let mut scene = Scene::<5>::new(WHITES, FADES);
        scene.light(&lit(RED), 1_000);

        let start = frame_at(&mut scene, 1_000);
        assert_eq!(
            start,
            [Rgbcct::OFF; 5],
            "the fade starts from what was shown"
        );
        assert!(scene.is_moving());

        let end = frame_at(&mut scene, 1_200);
        assert_eq!(end, settled(&lit(RED)));
        assert!(!scene.is_moving(), "done once the last frame is drawn");
    }

    /// The bug seen on the board: the first frame of a short fade was drawn,
    /// sending it took longer than the fade, and checking the clock then
    /// stopped the scene on that first (old) picture.
    #[test]
    fn a_slow_caller_still_gets_the_final_frame() {
        let mut scene = Scene::<5>::new(WHITES, FADES);
        scene.light(&lit(RED), 0);
        frame_at(&mut scene, 0); // first frame of the fade: still off…
        // …then the caller is busy for longer than the fade (no clock is
        // read here: the scene only knows about frames it drew).
        assert!(scene.is_moving(), "the final frame was not drawn yet");
        assert_eq!(frame_at(&mut scene, 500), settled(&lit(RED)));
        assert!(!scene.is_moving());
    }

    #[test]
    fn a_new_state_mid_fade_starts_from_the_picture_shown() {
        let mut scene = Scene::<5>::new(WHITES, FADES);
        scene.light(&lit(RED), 0);
        let halfway = frame_at(&mut scene, 100);
        scene.light(&lit(GREEN), 100);
        assert_eq!(frame_at(&mut scene, 100), halfway, "no jump");
        assert_eq!(frame_at(&mut scene, 300), settled(&lit(GREEN)));
    }

    #[test]
    fn switching_off_fades_to_black() {
        let mut scene = Scene::<5>::new(WHITES, FADES);
        scene.light(&lit(RED), 0);
        frame_at(&mut scene, 1_000);
        let off = LightState {
            on: false,
            ..lit(RED)
        };
        scene.light(&off, 1_000);
        assert_eq!(frame_at(&mut scene, 1_200), [Rgbcct::OFF; 5]);
        assert!(!scene.is_moving());
    }

    #[test]
    fn an_ambiance_fades_in_more_slowly() {
        let mut scene = Scene::<5>::new(WHITES, FADES);
        let sunset = LightState {
            ambiance: Some(Ambiance::Sunset),
            ..lit(RED)
        };
        scene.light(&sunset, 0);
        frame_at(&mut scene, 400);
        assert!(scene.is_moving(), "800 ms for an ambiance, not 200");
        assert_eq!(frame_at(&mut scene, 800), settled(&sunset));
        assert!(!scene.is_moving(), "the sunset doesn't move");
    }

    #[test]
    fn an_animated_ambiance_keeps_moving() {
        let mut scene = Scene::<5>::new(WHITES, FADES);
        let campfire = LightState {
            ambiance: Some(Ambiance::Campfire),
            ..lit(RED)
        };
        scene.light(&campfire, 0);
        frame_at(&mut scene, 5_000);
        assert!(scene.is_moving());
    }

    #[test]
    fn a_status_signal_is_drawn_on_top_and_keeps_moving() {
        let mut scene = Scene::<5>::new(WHITES, FADES);
        scene.status(Some(StatusSignal::WaitingForPairing), 0);
        let frame = frame_at(&mut scene, 0);
        assert!(frame[0].r > 0, "the red scanner starts on the first pixel");
        assert!(scene.is_moving());

        scene.status(None, 100);
        assert_eq!(frame_at(&mut scene, 100), [Rgbcct::OFF; 5]);
        assert!(!scene.is_moving());
    }

    #[test]
    fn preview_sums_up_the_frame_or_shows_the_signal() {
        let mut scene = Scene::<5>::new(WHITES, FADES);
        scene.light(&lit(RED), 0);
        let frame = frame_at(&mut scene, 1_000);
        assert_eq!(scene.preview(&frame, 1_000), frame[0], "plain color");

        scene.status(Some(StatusSignal::WifiLost), 1_000);
        let frame = frame_at(&mut scene, 1_000);
        assert_eq!(
            scene.preview(&frame, 1_000),
            StatusSignal::WifiLost.single_led(0).gamma_corrected()
        );
    }

    #[test]
    fn time_can_wrap_around() {
        let mut scene = Scene::<5>::new(WHITES, FADES);
        let before_wrap = u32::MAX - 50;
        scene.light(&lit(RED), before_wrap);
        frame_at(&mut scene, before_wrap);
        // 250 ms later, the u32 has wrapped: the fade is still over.
        assert_eq!(frame_at(&mut scene, 199), settled(&lit(RED)));
        assert!(!scene.is_moving());
    }
}
