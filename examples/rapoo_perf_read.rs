use anyhow::Result;
use mouse::performance::rapoo::{self as performance, Change};
fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--devices") {
        for device in mouse::transport::hid::list()?
            .into_iter()
            .filter(|d| d.driver == "rapoo-vt7")
        {
            let settings = performance::load_device(&device.id)?;
            println!("{}\n{}\n{settings:?}", device.name, device.id);
        }
        return Ok(());
    }
    let settings = performance::load()?;
    println!("{settings:?}");
    if std::env::args().any(|a| a == "--verify-write") {
        let after = performance::apply(settings.slot, Change::Radio(settings.rf))?;
        println!("write/readback verified: {after:?}");
    }
    Ok(())
}
