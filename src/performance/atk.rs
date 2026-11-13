//! X1 PRO Compx EEPROM performance settings; see docs/ATK X1 PRO/PERFORMANCE.md.
use crate::devices::protocols::compx;
use anyhow::{Context, Result, ensure};
use hidapi::{HidApi, HidDevice};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

static IO: Mutex<()> = Mutex::new(());
pub const RATES: [u8; 7] = [8, 4, 2, 1, 16, 32, 64];
pub const MODES: [&str; 3] = ["基础", "竞技", "竞技增强"];
#[derive(Clone, Debug)]
pub struct Settings {
    pub slot: u8,
    pub rate: usize,
    pub mode: u8,
    pub max_rate: usize,
}
#[derive(Clone, Copy)]
pub enum Change {
    Rate(usize),
    Mode(u8),
}

fn packet(command: u8, address: u16, len: u8, data: &[u8]) -> [u8; 17] {
    let mut report = [0u8; 17];
    report[0] = 8;
    report[1] = command;
    report[3..5].copy_from_slice(&address.to_be_bytes());
    report[5] = len;
    report[6..6 + data.len()].copy_from_slice(data);
    report[16] = 0x55u8.wrapping_sub(report[..16].iter().fold(0u8, |a, b| a.wrapping_add(*b)));
    report
}
struct Connection {
    device: HidDevice,
    max_rate: usize,
}
impl Connection {
    fn open(wireless: bool) -> Result<Self> {
        let api = HidApi::new()?;
        let devices: Vec<_> = api
            .device_list()
            .filter(|d| {
                d.vendor_id() == 0x373b
                    && (if wireless {
                        matches!(d.product_id(), 0x101a | 0x101b)
                    } else {
                        d.product_id() == 0x1016
                    })
                    && d.usage_page() == 0xff02
                    && d.usage() == 2
            })
            .collect();
        ensure!(
            devices.len() == 1,
            "请连接一只 ATK X1 PRO 或其接收器；多只设备时请先断开其他设备"
        );
        let info = devices[0];
        let connection = Self {
            device: info.open_device(&api)?,
            max_rate: if info.product_id() == 0x101b { 6 } else { 3 },
        };
        let model = connection.command(0x10, 0, 0, &[])?;
        ensure!(
            compx::model(&model) == Some((2, 38)),
            "配对设备不是 ATK X1 PRO"
        );
        Ok(connection)
    }
    fn command(&self, command: u8, address: u16, len: u8, data: &[u8]) -> Result<[u8; 17]> {
        let request = packet(command, address, len, data);
        ensure!(
            self.device.write(&request)? == request.len(),
            "配置请求发送不完整"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            let mut reply = [0; 17];
            let size = self.device.read_timeout(&mut reply, 100)?;
            if size != 17
                || reply[0] != 8
                || reply[1] != command
                || reply[3..5] != request[3..5]
                || reply.iter().fold(0u8, |a, b| a.wrapping_add(*b)) != 0x55
            {
                continue;
            }
            ensure!(reply[2] == 0, "设备拒绝配置命令：{}", reply[2]);
            ensure!(
                compx::payload(
                    &reply,
                    command,
                    match command {
                        8 => len,
                        14 => 1,
                        16 => 2,
                        _ => 0,
                    }
                )
                .is_some(),
                "配置响应数据无效"
            );
            return Ok(reply);
        }
        anyhow::bail!("配置响应超时，请唤醒鼠标并关闭官方驱动后重试")
    }
    fn slot(&self) -> Result<u8> {
        let slot = self.command(14, 0, 0, &[])?[6];
        ensure!(slot < 8, "未知配置槽");
        Ok(slot)
    }
    fn pair(&self, address: u16) -> Result<u8> {
        let reply = self.command(8, address, 2, &[])?;
        ensure!(reply[6].wrapping_add(reply[7]) == 0x55, "配置字段校验失败");
        Ok(reply[6])
    }
    fn settings(&self) -> Result<Settings> {
        let slot = self.slot()?;
        let raw = self.pair(0)?;
        let rate = RATES.iter().position(|r| *r == raw).context("未知回报率")?;
        let mode = self.pair(185)?;
        ensure!(mode < 3, "未知传感器模式");
        ensure!(self.slot()? == slot, "配置槽已切换，请重试");
        Ok(Settings {
            slot,
            rate,
            mode,
            max_rate: self.max_rate,
        })
    }
}
pub fn load(wireless: bool) -> Result<Settings> {
    let _guard = IO.lock().map_err(|_| anyhow::anyhow!("配置锁异常"))?;
    Connection::open(wireless)?.settings()
}
pub fn apply(wireless: bool, expected_slot: u8, change: Change) -> Result<Settings> {
    let _guard = IO.lock().map_err(|_| anyhow::anyhow!("配置锁异常"))?;
    let connection = Connection::open(wireless)?;
    let before = connection.settings()?;
    ensure!(before.slot == expected_slot, "配置槽已切换，请刷新后重试");
    let (address, value) = match change {
        Change::Rate(index) => {
            ensure!(
                index <= before.max_rate || index == before.rate,
                "当前连接不支持此回报率"
            );
            (0, RATES[index])
        }
        Change::Mode(mode) => {
            ensure!(mode < 3, "无效传感器模式");
            (185, mode)
        }
    };
    ensure!(connection.slot()? == before.slot, "配置槽已切换，停止写入");
    connection.command(7, address, 2, &[value, 0x55u8.wrapping_sub(value)])?;
    ensure!(
        connection.pair(address)? == value,
        "写入后回读不一致，请刷新配置"
    );
    let after = connection.settings()?;
    ensure!(after.slot == before.slot, "配置槽已切换，请刷新检查");
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_packet_layout_and_field_checksums() {
        assert_eq!(
            packet(4, 0, 0, &[]),
            crate::transport::compx::BATTERY_REQUEST
        );
        let report = packet(7, 185, 2, &[2, 0x53]);
        assert_eq!(&report[..8], &[8, 7, 0, 0, 185, 2, 2, 0x53]);
        assert_eq!(report.iter().fold(0u8, |a, b| a.wrapping_add(*b)), 0x55);
        assert_eq!(RATES, [8, 4, 2, 1, 16, 32, 64]);
    }
}
