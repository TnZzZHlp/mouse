//! Monitor-facing interface. A future queried HID/Bluetooth transport implements
//! these traits without changing the monitor or UI.
pub mod compx;
pub mod hid;

use crate::model::{Battery, DeviceIdentity, ReportKind};
use anyhow::Result;

pub trait BatteryConnection: Send {
    fn identity(&self) -> &DeviceIdentity;
    fn report_kind(&self) -> ReportKind {
        ReportKind::Broadcast
    }
    /// None means timeout or unrelated report; errors trigger reconnect.
    fn read(&mut self) -> Result<Option<Battery>>;
}

pub trait BatteryTransport: Send {
    fn connect(&mut self, selector: Option<&str>) -> Result<Box<dyn BatteryConnection>>;
}
