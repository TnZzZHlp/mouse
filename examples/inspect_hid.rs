//! Read HID metadata/descriptors for one explicitly selected USB VID/PID.
//! Optional capture reads only the selected vendor-defined collection.
use anyhow::{Context, Result};
use hidapi::HidApi;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn hex_arg(value: Option<String>, name: &str) -> Result<u16> {
    u16::from_str_radix(
        value
            .context(format!("Missing {name}"))?
            .trim_start_matches("0x"),
        16,
    )
    .with_context(|| format!("Invalid {name}"))
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let vid = hex_arg(args.next(), "VID")?;
    let pid = hex_arg(args.next(), "PID")?;
    let capture = if let Some(page) = args.next() {
        let page = hex_arg(Some(page), "usage page")?;
        anyhow::ensure!(
            page >= 0xff00,
            "Capture requires a vendor-defined usage page"
        );
        let usage = hex_arg(args.next(), "usage")?;
        let seconds: u64 = args.next().context("Missing capture duration")?.parse()?;
        Some((page, usage, seconds))
    } else {
        None
    };
    anyhow::ensure!(args.next().is_none(), "Unexpected extra arguments");
    let api = HidApi::new()?;
    let mut channels = Vec::new();
    for info in api
        .device_list()
        .filter(|info| info.vendor_id() == vid && info.product_id() == pid)
    {
        println!(
            "DEVICE {vid:04X}:{pid:04X} usage={:04X}:{:04X} interface={} manufacturer={:?} product={:?} path={}",
            info.usage_page(),
            info.usage(),
            info.interface_number(),
            info.manufacturer_string(),
            info.product_string(),
            info.path().to_string_lossy()
        );
        if info.usage_page() < 0xff00 {
            continue;
        }
        let device = match info.open_device(&api) {
            Ok(device) => device,
            Err(error) => {
                println!("OPEN ERROR: {error}");
                continue;
            }
        };
        let mut descriptor = vec![0; 4096];
        match device.get_report_descriptor(&mut descriptor) {
            Ok(size) => println!(
                "DESCRIPTOR usage={:04X}:{:04X}: {:02X?}",
                info.usage_page(),
                info.usage(),
                &descriptor[..size]
            ),
            Err(error) => println!("DESCRIPTOR ERROR: {error}"),
        }
        if capture
            .is_some_and(|(page, usage, _)| page == info.usage_page() && usage == info.usage())
        {
            channels.push((info.path().to_string_lossy().into_owned(), device, 0));
        }
    }
    if let Some((_, _, seconds)) = capture {
        anyhow::ensure!(!channels.is_empty(), "Capture collection not found");
        let deadline = Instant::now() + Duration::from_secs(seconds);
        while Instant::now() < deadline {
            for (path, device, count) in &mut channels {
                let mut buffer = [0; 4096];
                let size = device.read_timeout(&mut buffer, 100)?;
                if size != 0 {
                    *count += 1;
                    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
                    println!(
                        "REPORT time={timestamp} size={size} path={path} {:02X?}",
                        &buffer[..size]
                    );
                }
            }
        }
        for (path, _, count) in channels {
            println!("SUMMARY path={path} reports={count}");
        }
    }
    Ok(())
}
