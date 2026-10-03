//! Read-only Compx battery and paired-model queries.
//! Configuration, pairing, EEPROM and firmware commands are not implemented.
use crate::{
    devices::{MouseDriver, protocols::compx},
    diagnostics::log,
    model::{Battery, DeviceIdentity, ReportKind},
    transport::BatteryConnection,
};
use anyhow::Result;
use hidapi::HidDevice;
use std::time::{Duration, Instant};

pub const BATTERY_REQUEST: [u8; 17] = [8, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x49];
pub const MODEL_REQUEST: [u8; 17] = [8, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x3d];
const QUERY_INTERVAL: Duration = Duration::from_secs(15);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);

pub struct CompxConnection {
    device: HidDevice,
    identity: DeviceIdentity,
    driver: &'static dyn MouseDriver,
    next_query: Instant,
    deadline: Option<Instant>,
    logged_report: bool,
}

impl CompxConnection {
    pub fn new(
        device: HidDevice,
        identity: DeviceIdentity,
        driver: &'static dyn MouseDriver,
        expected_model: Option<(u8, u8)>,
    ) -> Result<Self> {
        if let Some(expected) = expected_model {
            verify_receiver(&device, expected)?;
        }
        Ok(Self {
            device,
            identity,
            driver,
            next_query: Instant::now(),
            deadline: None,
            logged_report: false,
        })
    }
}

fn verify_receiver(device: &HidDevice, expected: (u8, u8)) -> Result<()> {
    anyhow::ensure!(
        device.write(&MODEL_REQUEST)? == MODEL_REQUEST.len(),
        "Incomplete model GET request"
    );
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    while Instant::now() < deadline {
        let mut report = [0; 17];
        let size = device.read_timeout(&mut report, 100)?;
        if let Some(actual) = compx::model(&report[..size]) {
            anyhow::ensure!(
                actual == expected,
                "Paired mouse model {actual:?} does not match expected {expected:?}"
            );
            log(&format!(
                "Compx receiver verified: cid={}, mid={}; {:02X?}",
                actual.0,
                actual.1,
                &report[..size]
            ));
            return Ok(());
        }
    }
    anyhow::bail!("Compx paired-model response timed out")
}

impl BatteryConnection for CompxConnection {
    fn identity(&self) -> &DeviceIdentity {
        &self.identity
    }
    fn report_kind(&self) -> ReportKind {
        ReportKind::QueryResponse
    }
    fn read(&mut self) -> Result<Option<Battery>> {
        let now = Instant::now();
        if self.deadline.is_none() && now >= self.next_query {
            let written = self.device.write(&BATTERY_REQUEST)?;
            anyhow::ensure!(
                written == BATTERY_REQUEST.len(),
                "Incomplete battery GET request"
            );
            self.next_query = now + QUERY_INTERVAL;
            self.deadline = Some(now + RESPONSE_TIMEOUT);
        }
        let mut report = [0; 17];
        let size = self.device.read_timeout(&mut report, 1000)?;
        let battery = self.driver.decode(&report[..size]);
        if battery.is_some() {
            self.deadline = None;
            if !self.logged_report {
                log(&format!("Compx battery response: {:02X?}", &report[..size]));
                self.logged_report = true;
            }
        } else if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            anyhow::bail!("Compx GetBatteryLevel response timed out");
        }
        Ok(battery)
    }
}
