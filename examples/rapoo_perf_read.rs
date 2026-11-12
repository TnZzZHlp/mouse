use anyhow::Result;
use mouse::performance::{self, Change};
fn main() -> Result<()> {
    let settings = performance::load()?;
    println!("{settings:?}");
    if std::env::args().any(|a| a == "--verify-write") {
        let after = performance::apply(settings.slot, Change::Radio(settings.rf))?;
        println!("write/readback verified: {after:?}");
    }
    Ok(())
}
