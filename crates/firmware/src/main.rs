//! Firmware entry point for the ESP32-C6.
//!
//! Several tasks run side by side:
//! - **Matter** ([`matter`]): pairing with the Home app over Bluetooth, then
//!   Wi-Fi and the Apple TV's commands; runs in `main` itself;
//! - the **strip** ([`strip_task`]): a channel test at startup, then follows
//!   the lamp's state (on/off, brightness, color) coming from Matter, with
//!   the status signals on top (waiting for pairing, Wi-Fi…);
//! - the **touch button** ([`button`]): a tap switches the lamp on or off;
//!   held at power-up, it makes the lamp forget its pairing;
//! - the **dev board's RGB LED** mirrors the strip, summed up in one color
//!   (driven by [`strip_task`] too): handy to test without a strip;
//! - a **heartbeat** message every 2 seconds.
//!
//! For the LEDs the chain is always the same: `led_core` computes the colors,
//! `ws2805` encodes them and the RMT peripheral sends them. Logs go to the
//! computer over USB (`cargo run` flashes and shows them).

// No standard library (no OS on the chip), and no regular `fn main()`:
// `#[esp_rtos::main]` below provides the real entry point.
#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]
// The Matter types are deeply nested generics: the compiler needs more room.
#![recursion_limit = "256"]

mod board_led;
mod button;
mod matter;
mod pulse_code;
mod strip;

use board_led::BoardLed;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use esp_backtrace as _; // prints a backtrace over USB if the program panics
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Level, Pull};
use esp_hal::rmt::{PulseCode, Rmt, TxChannelConfig, TxChannelCreator};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_metadata_generated::memory_range;
use led_core::effect::{crossfade, frame_preview};
use led_core::{ColorTemperature, Easing, LightState, Rgbcct, StatusSignal, WhiteChannels};
use log::{debug, error, info};
use static_cell::ConstStaticCell;
use strip::Strip;
use ws2805::WhiteOrder;

// Metadata required by the bootloader (version, build date…).
esp_bootloader_esp_idf::esp_app_desc!();

/// Heap (memory allocated at run time, like PHP does for every array). Our
/// code doesn't use it, but the Wi-Fi/Bluetooth driver and Matter do.
const HEAP_SIZE: usize = 100 * 1024;
/// Part of the heap taken from RAM the bootloader no longer needs once the
/// program runs; the rest comes from regular RAM.
const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

/// RMT clock: 80 MHz, so one tick = 12.5 ns.
const RMT_FREQUENCY_MHZ: u32 = 80;
/// The on-board LED is very bright up close: keep it at ~30 % of what the
/// strip shows.
const BOARD_LED_BRIGHTNESS: u8 = 80;

/// 1.5 m at 60 LEDs/m, with one WS2805 chip per 3 LEDs: 20 pixels/m, so one
/// pixel every 5 cm. (1 chip per LED would give 90, but our strip doesn't.)
const STRIP_PIXELS: usize = 30;
/// ~25 % per channel during tests: safe for any bench power supply.
const STRIP_TEST_LEVEL: u8 = 64;
/// To check on the real strip with the channel test (see `HARDWARE.md`).
const STRIP_WHITES: WhiteOrder = WhiteOrder::WarmFirst;
/// The strip's white LEDs. Typical values: to check on the datasheet.
/// Matter also uses them as the range of whites the lamp offers.
const STRIP_WHITE_CHANNELS: WhiteChannels = WhiteChannels::new(
    ColorTemperature::from_kelvin(2700),
    ColorTemperature::from_kelvin(6500),
);
/// Matter runs the fades itself, but in steps (every 100 ms for colors),
/// and the Home app's slider sends instant jumps: we smooth each change over
/// this time so they don't show (ADR-0010). 200 ms, chosen by eye on the
/// board: softer than the first 100 ms, still following the slider.
const SMOOTHING_MS: u32 = 200;
/// Entering, leaving or changing an ambiance fades over this time.
const AMBIANCE_FADE_MS: u32 = 800;
/// Time between two frames while smoothing (50 frames per second).
const FRAME_MS: u64 = 20;

/// The strip's RMT codes (30 × 40 + 1 codes × 4 bytes ≈ 4.8 KB). A `static`
/// lives in RAM for the whole program instead of the small task stack;
/// `ConstStaticCell` hands out the one and only `&'static mut` to it.
static STRIP_CODES: ConstStaticCell<[PulseCode; strip::codes_len(STRIP_PIXELS)]> =
    ConstStaticCell::new([PulseCode::end_marker(); strip::codes_len(STRIP_PIXELS)]);

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    esp_alloc::heap_allocator!(size: HEAP_SIZE - RECLAIMED_RAM);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: RECLAIMED_RAM);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Embassy needs a hardware timer to know when to wake tasks up.
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);
    info!("Embassy started");

    spawner.spawn(heartbeat().expect("heartbeat task already running"));

    // The C6's RMT has 2 transmit channels: one for the board LED, one for
    // the strip.
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(RMT_FREQUENCY_MHZ))
        .expect("RMT clock")
        .into_async();
    let tick_hz = rmt.frequency().as_hz(); // the frequency actually obtained
    info!("RMT at {tick_hz} Hz");

    // Channel 1 → GPIO2 → level shifter → strip DIN.
    let strip_channel = rmt
        .channel1
        .configure_tx(&tx_config())
        .expect("RMT channel 1")
        .with_pin(peripherals.GPIO2);
    let strip = Strip::new(strip_channel, tick_hz, STRIP_WHITES, STRIP_CODES.take());

    // Channel 0 → GPIO8, where the board's WS2812 LED is wired.
    let led_channel = rmt
        .channel0
        .configure_tx(&tx_config())
        .expect("RMT channel 0")
        .with_pin(peripherals.GPIO8);
    let led = BoardLed::new(led_channel, tick_hz);

    // One task for both: the LED shows a summary of each strip frame right
    // after it, so they never need to talk to each other.
    spawner.spawn(strip_task(strip, led).expect("strip task already running"));

    // GPIO3 → TTP223 touch sensor output. The pull-down keeps the input at
    // "not touched" if the sensor is unplugged.
    let mut touch = Input::new(
        peripherals.GPIO3,
        InputConfig::default().with_pull(Pull::Down),
    );
    let forget_pairing = button::held_at_startup(&mut touch).await;
    spawner.spawn(button::button_task(touch).expect("button task already running"));

    // Last: Matter takes over `main` for good.
    matter::run(
        peripherals.WIFI,
        peripherals.BT,
        peripherals.RNG,
        peripherals.ADC1,
        peripherals.FLASH,
        forget_pairing,
    )
    .await
}

/// RMT settings shared by the LED and the strip.
fn tx_config() -> TxChannelConfig {
    TxChannelConfig::default()
        .with_clk_divider(1) // count ticks at the full RMT clock
        .with_idle_output(true) // between frames, hold the line…
        .with_idle_output_level(Level::Low) // …low (= reset/latch)
}

/// Drives the strip: the channel test once, then follows the lamp's state
/// (see `matter::LIGHT_CHANGED`) and shows the status signals on top (see
/// `matter::STATUS_CHANGED`).
#[embassy_executor::task]
async fn strip_task(mut strip: Strip<'static>, mut led: BoardLed<'static>) {
    channel_test(&mut strip, &mut led).await;

    let mut scene = Scene::new();
    // One frame: 30 pixels × 5 bytes, small enough for the task's stack.
    let mut frame = [Rgbcct::OFF; STRIP_PIXELS];
    loop {
        // Draw and send first: when nothing moves any more, this is the last
        // frame (exactly the target) before we go to sleep.
        scene.draw(&mut frame);
        show_frame(&mut strip, &frame).await;
        show_on_board_led(&mut led, scene.preview(&frame)).await;

        if scene.is_moving() {
            // Next frame, taking whatever arrived in the meantime.
            Timer::after(Duration::from_millis(FRAME_MS)).await;
            if let Some(state) = matter::LIGHT_CHANGED.try_take() {
                scene.light(&state);
            }
            if let Some(status) = matter::STATUS_CHANGED.try_take() {
                scene.status(status);
            }
        } else {
            // Nothing moving: sleep until Matter sends something. `select`
            // waits for both and returns the first one that happens.
            match select(matter::LIGHT_CHANGED.wait(), matter::STATUS_CHANGED.wait()).await {
                Either::First(state) => scene.light(&state),
                Either::Second(status) => scene.status(status),
            }
        }
    }
}

/// What the strip shows: the lamp's state (plain color or ambiance), faded
/// in from the previous picture, and sometimes a status signal on top.
struct Scene {
    /// The lamp's latest state, and since when its ambiance runs.
    state: LightState,
    ambiance_start: Instant,
    /// The picture shown when the state last changed (before gamma, without
    /// the status signal): the new picture fades in from it.
    fade_from: [Rgbcct; STRIP_PIXELS],
    fade_start: Instant,
    fade_ms: u32,
    /// Whether the last picture drawn was the fade's final one (the target).
    /// Decided when drawing, not later: sending a frame to the strip takes
    /// ~100 ms, so checking the clock after sending ended the fade before
    /// its final picture was ever drawn (seen on the board, 2026-11-20).
    fade_done: bool,
    /// The last picture drawn, same conventions as `fade_from`.
    shown: [Rgbcct; STRIP_PIXELS],
    status: Option<StatusSignal>,
    status_start: Instant,
}

impl Scene {
    fn new() -> Self {
        Self {
            state: LightState::DEFAULT,
            ambiance_start: Instant::now(),
            fade_from: [Rgbcct::OFF; STRIP_PIXELS],
            fade_start: Instant::now(),
            fade_ms: 0,
            fade_done: true,
            shown: [Rgbcct::OFF; STRIP_PIXELS],
            status: None,
            status_start: Instant::now(),
        }
    }

    /// A new state from the Home app (or the button).
    fn light(&mut self, state: &LightState) {
        // Log what changes for the eye (not the many brightness steps of a
        // fade): handy to match the Home app's commands with the strip.
        // `debug!`: hidden by default, see `ESP_LOG` in the guide.
        let previous = &self.state;
        if state.on != previous.on
            || state.color != previous.color
            || state.ambiance != previous.ambiance
        {
            debug!(
                "strip: on={} color={:?} ambiance={:?}",
                state.on,
                state.color,
                state.ambiance.map(led_core::Ambiance::name)
            );
        }

        // Fade from what is shown right now, so a new step in the middle of
        // a fade doesn't jump.
        self.fade_from = self.shown;
        self.fade_start = Instant::now();
        self.fade_done = false;
        // Matter's small steps are only smoothed; entering or leaving an
        // ambiance is a real change of picture, faded more slowly.
        self.fade_ms = if state.ambiance == self.state.ambiance {
            SMOOTHING_MS
        } else {
            self.ambiance_start = Instant::now();
            AMBIANCE_FADE_MS
        };
        self.state = *state;
    }

    /// A new status signal, or `None` when everything is fine again.
    fn status(&mut self, status: Option<StatusSignal>) {
        self.status = status;
        // The animation starts from its beginning (first pixel).
        self.status_start = Instant::now();
    }

    /// One color summing up `frame` for the board LED. During a status
    /// signal, the signal's own single-LED version instead (a breathing
    /// light where the strip has a scanner, the same blinking for the Wi-Fi
    /// alert), gamma corrected like the frame.
    fn preview(&self, frame: &[Rgbcct]) -> Rgbcct {
        match self.status {
            Some(status) => status
                .single_led(millis_since(self.status_start))
                .gamma_corrected(),
            None => frame_preview(frame),
        }
    }

    /// Whether the next frames will differ from this one.
    fn is_moving(&self) -> bool {
        self.status.is_some() || self.state.is_animated() || !self.fade_done
    }

    /// Fills `frame` with what to show now, gamma corrected.
    fn draw(&mut self, frame: &mut [Rgbcct; STRIP_PIXELS]) {
        // The lamp's picture, faded in from the previous one.
        self.state.render(
            &STRIP_WHITE_CHANNELS,
            millis_since(self.ambiance_start),
            frame,
        );
        // One reading of the clock for both: the progress of this picture,
        // and whether it is the fade's last one.
        let elapsed = millis_since(self.fade_start);
        let t = Easing::EaseInOut.progress(elapsed, self.fade_ms);
        crossfade(&self.fade_from, frame, t);
        self.fade_done = elapsed >= self.fade_ms;
        self.shown = *frame;

        // Then the status signal on top, and gamma last.
        if let Some(status) = self.status {
            status.render(millis_since(self.status_start), frame);
        }
        for pixel in frame.iter_mut() {
            *pixel = pixel.gamma_corrected();
        }
    }
}

/// Milliseconds since `start`. `as u32`: an animation or a transition never
/// lasts the 49 days a u32 of milliseconds can count (and if a status signal
/// did, the animations would simply restart).
fn millis_since(start: Instant) -> u32 {
    start.elapsed().as_millis() as u32
}

/// Lights the whole strip one channel at a time, 1 second each, and logs
/// which channel *should* be on. If the strip shows something else, the
/// channel order differs from the datasheet (see `HARDWARE.md`).
async fn channel_test(strip: &mut Strip<'static>, led: &mut BoardLed<'static>) {
    const L: u8 = STRIP_TEST_LEVEL;
    const STEPS: [(&str, Rgbcct); 6] = [
        ("red", Rgbcct::new(L, 0, 0, 0, 0)),
        ("green", Rgbcct::new(0, L, 0, 0, 0)),
        ("blue", Rgbcct::new(0, 0, L, 0, 0)),
        ("warm white", Rgbcct::new(0, 0, 0, L, 0)),
        ("cold white", Rgbcct::new(0, 0, 0, 0, L)),
        ("off", Rgbcct::OFF),
    ];
    for (name, pixel) in STEPS {
        show_color(strip, pixel).await;
        show_on_board_led(led, pixel).await;
        info!("strip test: {name}");
        Timer::after(Duration::from_secs(1)).await;
    }
}

/// Sends one color to the whole strip, logging (but otherwise ignoring)
/// errors: a missed frame is not worth stopping the lamp.
async fn show_color(strip: &mut Strip<'static>, pixel: Rgbcct) {
    if let Err(err) = strip.show_all(pixel).await {
        error!("strip: {err:?}");
    }
}

/// Sends one frame (one color per pixel), logging errors like
/// [`show_color`].
async fn show_frame(strip: &mut Strip<'static>, frame: &[Rgbcct]) {
    if let Err(err) = strip.show(frame).await {
        error!("strip: {err:?}");
    }
}

/// Shows `pixel` (what the strip shows, summed up in one color) on the
/// board LED, as RGB since the LED has no white channels. The colors arrive
/// already gamma corrected, like the strip's: only the brightness is lowered
/// here. Errors are logged and otherwise ignored, like the strip's.
async fn show_on_board_led(led: &mut BoardLed<'static>, pixel: Rgbcct) {
    let shown = pixel.rgb_preview().scale(BOARD_LED_BRIGHTNESS);
    if let Err(err) = led.show(shown).await {
        error!("board LED: {err:?}");
    }
}

/// Logs a message every 2 seconds, to show that tasks run side by side.
#[embassy_executor::task]
async fn heartbeat() {
    let mut beats: u32 = 0;
    loop {
        beats += 1;
        info!("heartbeat #{beats}");
        Timer::after(Duration::from_secs(2)).await;
    }
}
