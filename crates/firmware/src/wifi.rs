//! Wi-Fi: joins the home network (2.4 GHz only: the C6 has no 5 GHz radio)
//! and gets an IP address from the router (DHCP).
//!
//! Two layers work together:
//! - `esp-radio` drives the radio: it joins the network and reconnects when
//!   the link drops ([`connection`] task);
//! - `embassy-net` is the network stack (IP, DHCP, later TCP/UDP for Matter):
//!   it needs its own task ([`net_task`]) to process packets.
//!
//! The network name and password are read **at compile time** from the
//! `WIFI_SSID` and `WIFI_PASSWORD` environment variables, so they never end up
//! in git. Without them the firmware still builds and runs, just without
//! Wi-Fi.

use embassy_executor::Spawner;
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_hal::peripherals::WIFI;
use esp_hal::rng::Rng;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{AuthenticationMethodConfig, Config, Interface, WifiController};
use log::{info, warn};
use static_cell::ConstStaticCell;

/// `option_env!` is evaluated by the compiler: `Some("…")` if the variable was
/// set when building, `None` otherwise.
const SSID: Option<&str> = option_env!("WIFI_SSID");
const PASSWORD: Option<&str> = option_env!("WIFI_PASSWORD");

/// Pause between two connection attempts.
const RETRY_DELAY: Duration = Duration::from_secs(5);

/// Sockets the network stack can open at once. DHCP uses one; Matter will
/// need more later.
const SOCKETS: usize = 3;

/// The network stack's memory. Like the strip buffer, it must outlive every
/// task that uses it, hence a `static`. `ConstStaticCell` builds it at compile
/// time, so it never transits through the (small) stack.
static STACK_RESOURCES: ConstStaticCell<StackResources<SOCKETS>> =
    ConstStaticCell::new(StackResources::new());

/// Starts Wi-Fi in the background: spawns the connection, network stack and
/// status tasks, then returns right away. Does nothing (apart from a warning)
/// if the credentials were not set at build time.
#[allow(
    clippy::large_stack_frames,
    reason = "runs once at startup, called from main (same exemption): the network \
    stack's runner (~1.8 KB) only passes through before moving into its task"
)]
pub fn start(spawner: Spawner, wifi: WIFI<'static>) {
    // `let … else`: destructure, or run the `else` block (which must leave the
    // function). Like an early `return` after an `if (!$x)` in PHP.
    let (Some(ssid), Some(password)) = (SSID, PASSWORD) else {
        warn!("Wi-Fi disabled: build with WIFI_SSID and WIFI_PASSWORD set");
        return;
    };

    let authentication = if password.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        // Also accepted by WPA2/WPA3 "transition" routers. A WPA3-only
        // network would not work.
        AuthenticationMethodConfig::Wpa2Personal(
            password.try_into().expect("WIFI_PASSWORD is too long"),
        )
    };
    let station = StationConfig::default()
        .with_ssid(ssid.try_into().expect("WIFI_SSID is too long"))
        .with_authentication(authentication);

    let mut controller = WifiController::new(wifi, Default::default()).expect("Wi-Fi controller");
    controller
        .set_config(&Config::Station(station))
        .expect("Wi-Fi configuration");

    // The stack uses a random seed (for TCP sequence numbers, DHCP ids…).
    let rng = Rng::new();
    let seed = (u64::from(rng.random()) << 32) | u64::from(rng.random());
    let (stack, runner) = embassy_net::new(
        Interface::station(),
        embassy_net::Config::dhcpv4(Default::default()),
        STACK_RESOURCES.take(),
        seed,
    );

    spawner.spawn(connection(controller).expect("Wi-Fi task already running"));
    spawner.spawn(net_task(runner).expect("network task already running"));
    spawner.spawn(report_address(stack).expect("address task already running"));
}

/// Keeps the station connected: connects, waits for a disconnection, then
/// tries again after [`RETRY_DELAY`].
#[allow(
    clippy::large_stack_frames,
    reason = "~1 KB, almost all of it inside esp-radio's connect/disconnect futures"
)]
#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    loop {
        match controller.connect_async().await {
            Ok(connected) => {
                info!(
                    "Wi-Fi: connected to {} (channel {})",
                    connected.ssid.as_str(),
                    connected.channel
                );
                if let Ok(lost) = controller.wait_for_disconnect_async().await {
                    warn!("Wi-Fi: disconnected ({:?})", lost.reason);
                }
            }
            Err(err) => warn!("Wi-Fi: connection failed ({err:?})"),
        }
        Timer::after(RETRY_DELAY).await;
    }
}

/// Runs the network stack: sends and receives packets, renews the DHCP lease.
#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

/// Logs the IP address each time the network comes up (and when it is lost).
#[embassy_executor::task]
async fn report_address(stack: Stack<'static>) {
    loop {
        stack.wait_config_up().await;
        if let Some(config) = stack.config_v4() {
            info!("Wi-Fi: IP address {}", config.address);
        }
        stack.wait_config_down().await;
        warn!("Wi-Fi: IP address lost");
    }
}
