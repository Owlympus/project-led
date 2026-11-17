//! Firmware entry point for the ESP32-C6.
//!
//! For now it drives the RGB LED of the dev board around the color wheel:
//! `led_core` computes the colors, `ws2805` encodes them, and the RMT
//! peripheral sends them (see [`board_led`]). A heartbeat task runs side by
//! side, and logs go to the computer over USB (`cargo run` flashes and
//! shows them).
//!
//! The WS2805 strip itself is the next step.

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

use board_led::BoardLed;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _; // prints a backtrace over USB if the program panics
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Level;
use esp_hal::rmt::{Rmt, TxChannelConfig, TxChannelCreator};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use led_core::{Hsv, Rgb};
use log::{error, info};

// Metadata required by the bootloader (version, build date…).
esp_bootloader_esp_idf::esp_app_desc!();

/// The on-board LED is very bright up close: keep it at ~15 %.
const BOARD_LED_BRIGHTNESS: u8 = 40;
/// RMT clock: 80 MHz, so one tick = 12.5 ns.
const RMT_FREQUENCY_MHZ: u32 = 80;

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Embassy needs a hardware timer to know when to wake tasks up.
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);
    info!("Embassy started");

    // Runs `heartbeat` concurrently with the rest of `main`.
    spawner.spawn(heartbeat().expect("heartbeat task already running"));

    // RMT channel 0 → GPIO8, where the board's WS2812 LED is wired.
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(RMT_FREQUENCY_MHZ))
        .expect("RMT clock")
        .into_async();
    let tick_hz = rmt.frequency().as_hz(); // the frequency actually obtained
    let channel = rmt
        .channel0
        .configure_tx(
            &TxChannelConfig::default()
                .with_clk_divider(1) // count ticks at the full RMT clock
                .with_idle_output(true) // between frames, hold the line…
                .with_idle_output_level(Level::Low), // …low (= reset/latch)
        )
        .expect("RMT channel")
        .with_pin(peripherals.GPIO8);
    let mut led = BoardLed::new(channel, tick_hz);
    info!("Board LED ready (RMT at {tick_hz} Hz)");

    // Walk around the color wheel: 2° every 40 ms, so one turn in ~7 s.
    let mut degrees: u16 = 0;
    loop {
        let color: Rgb = Hsv::from_degrees(degrees, 255, 255).into();
        let shown = color.scale(BOARD_LED_BRIGHTNESS).gamma_corrected();
        if let Err(err) = led.show(shown).await {
            error!("board LED: {err:?}");
        }
        if degrees.is_multiple_of(60) {
            info!("hue {degrees:>3}° -> {color:?} -> LED {shown:?}");
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
