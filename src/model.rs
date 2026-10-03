//! Shared domain types. No HID, Windows or tray dependencies.
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charging {
    Discharging,
    Active,
    Full,
    Unknown(u8),
}

impl Charging {
    pub fn label(self) -> String {
        match self {
            Self::Discharging => "未充电".into(),
            Self::Active => "正在充电".into(),
            Self::Full => "充电已完成".into(),
            Self::Unknown(value) => format!("未知充电状态 ({value})"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub charging: Charging,
    pub raw_charging: u8,
    pub dpi: Option<(u16, u16)>,
}

/// Whether a valid report was broadcast or returned to a read-only query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportKind {
    Broadcast,
    QueryResponse,
}

/// Last valid battery report and its actual reception time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    pub battery: Battery,
    pub received_at: SystemTime,
    pub kind: ReportKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceIdentity {
    /// Stable for this HID instance; can be supplied to --device to select it.
    pub id: String,
    pub driver: &'static str,
    pub name: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HidDescriptor {
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Searching,
    Waiting(DeviceIdentity),
    Live(DeviceIdentity, Reading),
    Stale(DeviceIdentity, Option<Reading>),
    Offline(String),
}

impl Status {
    pub const fn reading(&self) -> Option<Reading> {
        match self {
            Self::Live(_, reading) | Self::Stale(_, Some(reading)) => Some(*reading),
            _ => None,
        }
    }
}
