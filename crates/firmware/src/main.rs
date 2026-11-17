//! Firmware entry point for the ESP32-C6.
//!
//! Several tasks run side by side:
//! - **Matter** ([`matter`]): pairing with the Home app over Bluetooth, then
//!   Wi-Fi and the Apple TV's commands; runs in `main` itself;
//! - the **strip** ([`strip_task`]): a channel test at startup, then follows
//!   the lamp's state (on/off, brightness, color) coming from Matter;
//! - the **dev board's RGB LED** walks around the color wheel, to show the
//!   firmware is alive ([`board_led_task`]);
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
mod matter;
mod pulse_code;
mod strip;

use board_led::BoardLed;
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use esp_backtrace as _; // prints a backtrace over USB if the program panics
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Level;
use esp_hal::rmt::{PulseCode, Rmt, TxChannelConfig, TxChannelCreator};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_metadata_generated::memory_range;
use led_core::{ColorTemperature, Hsv, Rgb, Rgbcct, Transition, WhiteChannels};
use log::{error, info};
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
/// The on-board LED is very bright up close: keep it at ~15 %.
const BOARD_LED_BRIGHTNESS: u8 = 40;

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
/// Matter runs the fades itself, but in steps (every 100 ms for colors):
/// we smooth each step over this time so they don't show (ADR-0010).
const SMOOTHING_MS: u32 = 100;
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
    spawner.spawn(strip_task(strip).expect("strip task already running"));

    // Channel 0 → GPIO8, where the board's WS2812 LED is wired.
    let led_channel = rmt
        .channel0
        .configure_tx(&tx_config())
        .expect("RMT channel 0")
        .with_pin(peripherals.GPIO8);
    let led = BoardLed::new(led_channel, tick_hz);
    spawner.spawn(board_led_task(led).expect("board LED task already running"));

    // Last: Matter takes over `main` for good.
    matter::run(
        peripherals.WIFI,
        peripherals.BT,
        peripherals.RNG,
        peripherals.ADC1,
        peripherals.FLASH,
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
/// sent by Matter (see `matter::LIGHT_CHANGED`).
#[embassy_executor::task]
async fn strip_task(mut strip: Strip<'static>) {
    channel_test(&mut strip).await;

    // Always heading from what is shown towards the latest state.
    let mut transition = Transition::instant(Rgbcct::OFF);
    let mut start = Instant::now();
    loop {
        // `as u32`: a transition lasts far less than the 49 days a u32 of
        // milliseconds can count.
        let elapsed = start.elapsed().as_millis() as u32;
        // Show first: when the transition is done, this is its last frame
        // (exactly the target) before we go to sleep.
        show(&mut strip, transition.at(elapsed).gamma_corrected()).await;

        let new_state = if transition.is_done(elapsed) {
            // Nothing moving: sleep until Matter sends something.
            Some(matter::LIGHT_CHANGED.wait().await)
        } else {
            // Moving: next frame, taking a new state if there is one.
            Timer::after(Duration::from_millis(FRAME_MS)).await;
            matter::LIGHT_CHANGED.try_take()
        };

        if let Some(state) = new_state {
            // Start again from the color shown right now, so a new step
            // in the middle of the smoothing doesn't jump.
            let shown = transition.at(start.elapsed().as_millis() as u32);
            let target = state.pixel(&STRIP_WHITE_CHANNELS);
            transition = Transition::new(shown, target, SMOOTHING_MS);
            start = Instant::now();
        }
    }
}

/// Lights the whole strip one channel at a time, 1 second each, and logs
/// which channel *should* be on. If the strip shows something else, the
/// channel order differs from the datasheet (see `HARDWARE.md`).
async fn channel_test(strip: &mut Strip<'static>) {
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
        show(strip, pixel).await;
        info!("strip test: {name}");
        Timer::after(Duration::from_secs(1)).await;
    }
}

/// Sends one color to the whole strip, logging (but otherwise ignoring)
/// errors: a missed frame is not worth stopping the lamp.
async fn show(strip: &mut Strip<'static>, pixel: Rgbcct) {
    if let Err(err) = strip.show_all(pixel).await {
        error!("strip: {err:?}");
    }
}

/// Walks the board LED around the color wheel: 2° every 40 ms, so one turn
/// in ~7 s.
#[embassy_executor::task]
async fn board_led_task(mut led: BoardLed<'static>) {
    let mut degrees: u16 = 0;
    loop {
        let color: Rgb = Hsv::from_degrees(degrees, 255, 255).into();
        let shown = color.scale(BOARD_LED_BRIGHTNESS).gamma_corrected();
        if let Err(err) = led.show(shown).await {
            error!("board LED: {err:?}");
        }
        degrees = (degrees + 2) % 360;
        Timer::after(Duration::from_millis(40)).await;
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
