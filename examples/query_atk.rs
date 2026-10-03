//! Exact official Compx `GetBatteryLevel` request for the connected ATK X1 PRO.
//! No arbitrary command input, configuration writes or upgrade interface.
use anyhow::{Context, Result};
use hidapi::HidApi;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    let pid = std::env::args().nth(1).map_or(Ok(0x1016), |value| {
        u16::from_str_radix(value.trim_start_matches("0x"), 16)
    })?;
    anyhow::ensure!(
        matches!(pid, 0x1016 | 0x101b),
        "Only verified ATK wired/8K receiver PIDs allowed"
    );
    let api = HidApi::new()?;
    let info = api
        .device_list()
        .find(|info| {
            info.vendor_id() == 0x373b
                && info.product_id() == pid
                && info.usage_page() == 0xff02
                && info.usage() == 2
        })
        .context("ATK X1 PRO Report 8 collection not connected")?;
    let device = info.open_device(&api)?;
    if pid == 0x101b {
        let request = mouse::transport::compx::MODEL_REQUEST;
        println!("GET MODEL {request:02X?}");
        query(&device, &request, 0x10)?;
    }
    let request = mouse::transport::compx::BATTERY_REQUEST;
    println!("GET BATTERY {request:02X?}");
    query(&device, &request, 4)
}

fn query(device: &hidapi::HidDevice, request: &[u8; 17], command: u8) -> Result<()> {
    let written = device.write(request)?;
    anyhow::ensure!(
        written == request.len(),
        "Incomplete GET request: {written}"
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let mut report = [0; 17];
        let size = device.read_timeout(&mut report, 100)?;
        if size != 0 {
            println!("RESPONSE size={size} {:02X?}", &report[..size]);
            if size == report.len() && report[0] == 8 && report[1] == command {
                if command == 4 {
                    println!(
                        "battery={} raw_charging={} command_status={} valid_len={} sum={:02X}",
                        report[6],
                        report[7],
                        report[2],
                        report[5],
                        report.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
                    );
                } else {
                    println!("MODEL cid={} mid={}", report[6], report[7]);
                }
                return Ok(());
            }
        }
    }
    anyhow::bail!("No GetBatteryLevel response within 3 seconds")
}
