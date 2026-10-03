use crate::{
    devices::{self, MouseDriver, ReadMethod},
    diagnostics::log,
    model::{Battery, DeviceIdentity, HidDescriptor},
    transport::{BatteryConnection, BatteryTransport},
};
use anyhow::{Context, Result};
use hidapi::{DeviceInfo, HidApi, HidDevice};

#[derive(Default)]
pub struct HidTransport {
    last_id: Option<String>,
}

struct HidConnection {
    device: HidDevice,
    identity: DeviceIdentity,
    driver: &'static dyn MouseDriver,
    buffer: Vec<u8>,
    logged_report: bool,
}

fn descriptor(info: &DeviceInfo) -> HidDescriptor {
    HidDescriptor {
        vendor_id: info.vendor_id(),
        product_id: info.product_id(),
        usage_page: info.usage_page(),
        usage: info.usage(),
    }
}

fn identify(info: &DeviceInfo) -> Option<(&'static dyn MouseDriver, DeviceIdentity)> {
    let driver = devices::find_driver(descriptor(info))?;
    Some((
        driver,
        DeviceIdentity {
            id: info.path().to_string_lossy().into_owned(),
            driver: driver.id(),
            name: driver.name(),
        },
    ))
}

pub fn list() -> Result<Vec<DeviceIdentity>> {
    let api = HidApi::new().context("枚举 HID 设备失败")?;
    Ok(api
        .device_list()
        .filter_map(|info| identify(info).map(|(_, identity)| identity))
        .collect())
}

impl BatteryTransport for HidTransport {
    fn connect(&mut self, selector: Option<&str>) -> Result<Box<dyn BatteryConnection>> {
        let api = HidApi::new().context("枚举 HID 设备失败")?;
        let mut errors = Vec::new();
        let mut candidates: Vec<_> = api
            .device_list()
            .filter_map(|info| {
                let (driver, identity) = identify(info)?;
                if selector
                    .is_some_and(|selected| selected != identity.driver && selected != identity.id)
                {
                    return None;
                }
                Some((info, driver, identity))
            })
            .collect();
        candidates.sort_by(|left, right| {
            right
                .1
                .connection_priority(descriptor(right.0))
                .cmp(&left.1.connection_priority(descriptor(left.0)))
                .then_with(|| left.2.id.cmp(&right.2.id))
        });
        // A silent receiver must not prevent discovery of another supported mouse.
        if let Some(index) = candidates
            .iter()
            .position(|candidate| Some(&candidate.2.id) == self.last_id.as_ref())
        {
            let next = (index + 1) % candidates.len();
            candidates.rotate_left(next);
        }
        for (info, driver, identity) in candidates {
            match info.open_device(&api) {
                Ok(device) => {
                    self.last_id = Some(identity.id.clone());
                    log(&format!(
                        "HID connected: {} [{}] {}",
                        identity.name, identity.driver, identity.id
                    ));
                    if let ReadMethod::CompxBatteryQuery { expected_model } =
                        driver.read_method(descriptor(info))
                    {
                        match super::compx::CompxConnection::new(
                            device,
                            identity,
                            driver,
                            expected_model,
                        ) {
                            Ok(connection) => return Ok(Box::new(connection)),
                            Err(error) => errors.push(format!("{}：{error:#}", driver.name())),
                        }
                        continue;
                    }
                    return Ok(Box::new(HidConnection {
                        device,
                        identity,
                        driver,
                        buffer: vec![0; driver.input_report_len()],
                        logged_report: false,
                    }));
                }
                Err(error) => errors.push(format!("{}：{error}", identity.name)),
            }
        }
        if errors.is_empty() {
            anyhow::bail!("未找到支持的鼠标；请检查 USB 连接、接收器和 --device 选择器");
        }
        anyhow::bail!(
            "无法打开设备，请关闭占用设备的驱动程序：{}",
            errors.join("；")
        )
    }
}

impl BatteryConnection for HidConnection {
    fn identity(&self) -> &DeviceIdentity {
        &self.identity
    }
    fn read(&mut self) -> Result<Option<Battery>> {
        // Passive read only. No write or feature-report API is exposed to drivers.
        let size = self.device.read_timeout(&mut self.buffer, 1000)?;
        if size > 0 && !self.logged_report {
            log(&format!(
                "First HID report ({} bytes): {:02X?}",
                size,
                &self.buffer[..size]
            ));
            self.logged_report = true;
        }
        Ok(self.driver.decode(&self.buffer[..size]))
    }
}
