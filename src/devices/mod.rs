//! Device identification and pure protocol decoding. Register new drivers here.
pub mod atk;
pub mod protocols;
pub mod rapoo;

use crate::model::{Battery, HidDescriptor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadMethod {
    PassiveInput,
    CompxBatteryQuery { expected_model: Option<(u8, u8)> },
}

pub trait MouseDriver: Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    /// Match the specific battery collection, never every interface of a VID/PID.
    fn matches(&self, descriptor: HidDescriptor) -> bool;
    /// Higher priority is tried first; silent channels still rotate on retry.
    fn connection_priority(&self, _descriptor: HidDescriptor) -> u8 {
        0
    }
    fn input_report_len(&self) -> usize;
    fn read_method(&self, _descriptor: HidDescriptor) -> ReadMethod {
        ReadMethod::PassiveInput
    }
    /// Native HID bytes include report ID. None means unrelated/invalid report.
    fn decode(&self, report: &[u8]) -> Option<Battery>;
}

pub static DRIVERS: &[&dyn MouseDriver] = &[&rapoo::vt7::Vt7, &atk::x1_pro::X1Pro];

pub fn find_driver(descriptor: HidDescriptor) -> Option<&'static dyn MouseDriver> {
    DRIVERS
        .iter()
        .copied()
        .find(|driver| driver.matches(descriptor))
}
