//! Firmware entry point for the ESP32-C6.
//!
//! For now it runs several tasks side by side:
//! - the RGB LED of the dev board walks around the color wheel ([`board_led`]);
//! - the WS2805 strip lights each channel in turn (red, green, blue, W1, W2),
//!   to check the channel order of the real strip ([`strip`]);
//! - Wi-Fi joins the home network and logs its IP address ([`wifi`]);
//! - a heartbeat message every 2 seconds.
//!
//! Everywhere the chain is the same: `led_core` computes the colors, `ws2805`
//! encodes them and the RMT peripheral sends them. Logs go to the computer
//! over USB (`cargo run` flashes and shows them).

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

mod board_led;
mod pulse_code;
mod strip;
mod wifi;

use board_led::BoardLed;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _; // prints a backtrace over USB if the program panics
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Level;
use esp_hal::rmt::{PulseCode, Rmt, TxChannelConfig, TxChannelCreator};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use led_core::{Hsv, Rgb, Rgbcct};
use log::{error, info};
use static_cell::ConstStaticCell;
use strip::Strip;
use ws2805::WhiteOrder;

// Metadata required by the bootloader (version, build date…).
esp_bootloader_esp_idf::esp_app_desc!();

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

/// The strip's RMT codes (30 × 40 + 1 codes × 4 bytes ≈ 4.8 KB). A `static` lives in RAM for the whole
/// program instead of the small task stack; `ConstStaticCell` hands out the
/// one and only `&'static mut` to it.
static STRIP_CODES: ConstStaticCell<[PulseCode; strip::codes_len(STRIP_PIXELS)]> =
    ConstStaticCell::new([PulseCode::end_marker(); strip::codes_len(STRIP_PIXELS)]);

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // A heap (memory allocated at run time, like PHP does for every array):
    // our code doesn't use one, but the Wi-Fi driver does. 64 KB taken from
    // RAM the bootloader no longer needs once the program runs.
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 64 * 1024);

    // Embassy needs a hardware timer to know when to wake tasks up.
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);
    info!("Embassy started");

    spawner.spawn(heartbeat().expect("heartbeat task already running"));
    wifi::start(spawner, peripherals.WIFI);

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
    spawner.spawn(strip_channel_test(strip).expect("strip task already running"));

    // Channel 0 → GPIO8, where the board's WS2812 LED is wired.
    let led_channel = rmt
        .channel0
        .configure_tx(&tx_config())
        .expect("RMT channel 0")
        .with_pin(peripherals.GPIO8);
    let mut led = BoardLed::new(led_channel, tick_hz);

    // Walk around the color wheel: 2° every 40 ms, so one turn in ~7 s.
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

/// RMT settings shared by the LED and the strip.
fn tx_config() -> TxChannelConfig {
    TxChannelConfig::default()
        .with_clk_divider(1) // count ticks at the full RMT clock
        .with_idle_output(true) // between frames, hold the line…
        .with_idle_output_level(Level::Low) // …low (= reset/latch)
}

/// Lights the whole strip one channel at a time, 1 second each, and logs
/// which channel *should* be on. If the strip shows something else, the
/// channel order differs from the datasheet (see `HARDWARE.md`).
#[embassy_executor::task]
async fn strip_channel_test(mut strip: Strip<'static>) {
    const L: u8 = STRIP_TEST_LEVEL;
    const STEPS: [(&str, Rgbcct); 6] = [
        ("red", Rgbcct::new(L, 0, 0, 0, 0)),
        ("green", Rgbcct::new(0, L, 0, 0, 0)),
        ("blue", Rgbcct::new(0, 0, L, 0, 0)),
        ("warm white", Rgbcct::new(0, 0, 0, L, 0)),
        ("cold white", Rgbcct::new(0, 0, 0, 0, L)),
        ("off", Rgbcct::OFF),
    ];
    loop {
        for (name, pixel) in STEPS {
            match strip.show_all(pixel).await {
                Ok(()) => info!("strip: {name}"),
                Err(err) => error!("strip: {err:?}"),
            }
            Timer::after(Duration::from_secs(1)).await;
        }
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
