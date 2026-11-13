//! Read X1 PRO performance settings; optionally verify writing the current values.
use anyhow::Result;
use mouse::performance::{Change, Device};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        args.iter().all(|a| a == "--wired" || a == "--verify-write"),
        "未知参数"
    );
    let wireless = !args.iter().any(|a| a == "--wired");
    let settings = Device::AtkX1Pro.load(wireless)?;
    println!("{settings:?}");
    if args.iter().any(|a| a == "--verify-write") {
        println!(
            "Rate readback: {:?}",
            Device::AtkX1Pro.apply(wireless, settings.slot, Change::Rate(settings.rate))?
        );
        println!(
            "Mode readback: {:?}",
            Device::AtkX1Pro.apply(
                wireless,
                settings.slot,
                Change::Mode(settings.rate, settings.mode)
            )?
        );
    }
    Ok(())
}
