//! Passive comparison of VT7 wired and receiver status collections.
use anyhow::{Context, Result};
use hidapi::HidApi;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn main() -> Result<()> {
    let seconds = std::env::args()
        .nth(1)
        .map_or(Ok(25), |value| value.parse::<u64>())?;
    let api = HidApi::new()?;
    let mut channels = Vec::new();
    for info in api
        .device_list()
        .filter(|info| info.vendor_id() == 0x24ae && matches!(info.product_id(), 0x1413 | 0x4613))
    {
        println!(
            "PID={:04X} usage={:04X}:{:04X} interface={} product={:?} path={}",
            info.product_id(),
            info.usage_page(),
            info.usage(),
            info.interface_number(),
            info.product_string(),
            info.path().to_string_lossy()
        );
        if info.usage_page() == 0xff00 && info.usage() == 2 {
            let device = info.open_device(&api).context("Open status collection")?;
            let mut descriptor = vec![0; 4096];
            match device.get_report_descriptor(&mut descriptor) {
                Ok(size) => println!(
                    "DESCRIPTOR PID={:04X}: {:02X?}",
                    info.product_id(),
                    &descriptor[..size]
                ),
                Err(error) => println!("DESCRIPTOR PID={:04X}: {error}", info.product_id()),
            }
            channels.push((info.product_id(), device, 0));
        }
    }
    anyhow::ensure!(!channels.is_empty(), "No VT7 status collection connected");
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        for (pid, device, reports) in &mut channels {
            let mut buffer = [0; 64];
            match device.read_timeout(&mut buffer, 100) {
                Ok(0) => {}
                Ok(size) => {
                    *reports += 1;
                    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
                    println!(
                        "REPORT time={timestamp} PID={pid:04X} size={size} {:02X?}",
                        &buffer[..size]
                    );
                }
                Err(error) => println!("READ ERROR PID={pid:04X}: {error}"),
            }
        }
    }
    for (pid, _, reports) in channels {
        println!("SUMMARY PID={pid:04X} reports={reports}");
    }
    Ok(())
}
