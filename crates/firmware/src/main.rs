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
//! `ws2805` encodes them, and a peripheral sends them: the SPI with its DMA
//! for the strip (ADR-0017), the RMT for the board's single LED. Logs go to
//! the computer over USB (`cargo run` flashes and shows them).

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
use esp_hal::rmt::{Rmt, TxChannelConfig, TxChannelCreator};
use esp_hal::spi::Mode as SpiMode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{dma_rx_buffer, dma_tx_buffer};
use esp_metadata_generated::memory_range;
use led_core::{ColorTemperature, FadeTimes, LightState, Rgbcct, Scene, WhiteChannels};
use log::{debug, error, info};
use static_cell::ConstStaticCell;
use strip::Strip;
use ws2805::{WhiteOrder, spi};

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
/// The strip's white LEDs: 3000 K and 6500 K, from the seller's listing
/// (SEZO WS2805). Matter also uses them as the range of whites the lamp
/// offers.
const STRIP_WHITE_CHANNELS: WhiteChannels = WhiteChannels::new(
    ColorTemperature::from_kelvin(3000),
    ColorTemperature::from_kelvin(6500),
);
/// Matter runs the fades itself, but in steps (every 100 ms for colors),
/// and the Home app's slider sends instant jumps: we smooth each change over
/// this time so they don't show (ADR-0010). 200 ms, chosen by eye on the
/// board: softer than the first 100 ms, still following the slider.
const SMOOTHING_MS: u32 = 200;
/// Entering, leaving or changing an ambiance fades over this time.
const AMBIANCE_FADE_MS: u32 = 800;
/// Both fade times, for the scene.
const FADE_TIMES: FadeTimes = FadeTimes {
    smoothing_ms: SMOOTHING_MS,
    ambiance_ms: AMBIANCE_FADE_MS,
};
/// Time between two frames while smoothing (50 frames per second).
const FRAME_MS: u64 = 20;

/// What the strip shows (≈340 bytes): in a `static`, like the buffer below,
/// rather than in the task. `ConstStaticCell` hands out the one and only
/// `&'static mut` to it.
static SCENE: ConstStaticCell<Scene<STRIP_PIXELS>> =
    ConstStaticCell::new(Scene::new(STRIP_WHITE_CHANNELS, FADE_TIMES));

/// Where each strip frame is encoded as SPI bytes (840 bytes for 30 pixels,
/// see `ws2805::spi`). A `static` lives in RAM for the whole program instead
/// of the small task stack; `ConstStaticCell` hands out the one and only
/// `&'static mut` to it.
static STRIP_BUFFER: ConstStaticCell<[u8; spi::buffer_len(STRIP_PIXELS)]> =
    ConstStaticCell::new([0; spi::buffer_len(STRIP_PIXELS)]);
/// The DMA's own working buffers: the transmit one must hold a whole frame,
/// so it is never sent in two pieces (a gap in the middle would end the
/// frame early). Nothing is ever received from the strip.
const DMA_TX_BYTES: usize = 1024;
const DMA_RX_BYTES: usize = 4;

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

    // The strip: SPI2 with its DMA (ADR-0017). Only the data line (MOSI) is
    // used: GPIO2 → level shifter → strip DIN.
    let dma_tx = dma_tx_buffer!(DMA_TX_BYTES).expect("DMA transmit buffer");
    let dma_rx = dma_rx_buffer!(DMA_RX_BYTES).expect("DMA receive buffer");
    let strip_spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_hz(spi::SPI_FREQUENCY_HZ))
            .with_mode(SpiMode::_0),
    )
    .expect("SPI for the strip")
    .with_mosi(peripherals.GPIO2)
    .with_dma(peripherals.DMA_CH0)
    .with_buffers(dma_rx, dma_tx)
    .into_async();
    let strip = Strip::new(strip_spi, STRIP_WHITES, STRIP_BUFFER.take());

    // The board LED: the RMT, whose small memory holds its 25 pulses at once.
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(RMT_FREQUENCY_MHZ))
        .expect("RMT clock")
        .into_async();
    let tick_hz = rmt.frequency().as_hz(); // the frequency actually obtained
    info!("RMT at {tick_hz} Hz");

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

/// RMT settings for the board LED.
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

    // The picture and its fades are computed by `led_core` (testable on the
    // computer); this task only feeds it and sends its frames.
    let scene = SCENE.take();
    // One frame: 30 pixels × 5 bytes, small enough for the task's stack.
    let mut frame = [Rgbcct::OFF; STRIP_PIXELS];
    loop {
        // Draw and send first: when nothing moves any more, this is the last
        // frame (exactly the target) before we go to sleep.
        let now = now_ms();
        scene.draw(now, &mut frame);
        show_frame(&mut strip, &frame).await;
        show_on_board_led(&mut led, scene.preview(&frame, now)).await;

        if scene.is_moving() {
            // Next frame, taking whatever arrived in the meantime.
            Timer::after(Duration::from_millis(FRAME_MS)).await;
            if let Some(state) = matter::LIGHT_CHANGED.try_take() {
                new_light(scene, &state);
            }
            if let Some(status) = matter::STATUS_CHANGED.try_take() {
                scene.status(status, now_ms());
            }
        } else {
            // Nothing moving: sleep until Matter sends something. `select`
            // waits for both and returns the first one that happens.
            match select(matter::LIGHT_CHANGED.wait(), matter::STATUS_CHANGED.wait()).await {
                Either::First(state) => new_light(scene, &state),
                Either::Second(status) => scene.status(status, now_ms()),
            }
        }
    }
}

/// Gives the scene a new state from Matter, logging what changes for the
/// eye (not the many brightness steps of a fade): handy to match the Home
/// app's commands with the strip. `debug!`: hidden by default, see
/// `ESP_LOG` in the guide.
fn new_light(scene: &mut Scene<STRIP_PIXELS>, state: &LightState) {
    let previous = scene.state();
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
    scene.light(state, now_ms());
}

/// The time for the scene: milliseconds since the chip started. `as u32`
/// wraps every 49 days, which the scene handles.
fn now_ms() -> u32 {
    Instant::now().as_millis() as u32
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
    for (index, (name, pixel)) in STEPS.into_iter().enumerate() {
        let start = Instant::now();
        show_color(strip, pixel).await;
        if index == 0 {
            // Once, to check the strip is fed fast enough (~2.6 ms expected
            // with the SPI + DMA, ADR-0017).
            info!(
                "strip: one frame sent in {} µs",
                start.elapsed().as_micros()
            );
        }
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

/// The time printed at the start of each log line, in milliseconds since
/// the chip started: `INFO (12345) - …`. Handy to time what happens between
/// the lamp and the Apple TV.
///
/// `esp-println` (with its `timestamp` feature) declares this function
/// without defining it, and calls it by its exact name: `no_mangle` keeps
/// that name in the program (Rust usually renames functions internally).
/// It is `unsafe` to promise: no other function may have the same name.
///
/// `esp_hal`'s clock, not Embassy's: it works from the very first log line,
/// before Embassy is started.
#[unsafe(no_mangle)]
pub extern "Rust" fn _esp_println_timestamp() -> u64 {
    esp_hal::time::Instant::now()
        .duration_since_epoch()
        .as_millis()
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
