//! VT7 V2 performance controls, using the official nRF54L configuration protocol.
use anyhow::{Context, Result, ensure};
use hidapi::{HidApi, HidDevice};
use std::{sync::Mutex, thread, time::Duration};

static IO: Mutex<()> = Mutex::new(());
static REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn revision() -> u64 {
    REVISION.load(std::sync::atomic::Ordering::Relaxed)
}
static AUTOMATIC: [std::sync::atomic::AtomicBool; 2] =
    [const { std::sync::atomic::AtomicBool::new(false) }; 2];
pub fn set_automatic(wireless: bool, enabled: bool) {
    AUTOMATIC[usize::from(wireless)].store(enabled, std::sync::atomic::Ordering::SeqCst);
}
pub fn automatic(wireless: bool) -> bool {
    AUTOMATIC[usize::from(wireless)].load(std::sync::atomic::Ordering::SeqCst)
}
pub const RATES: [u8; 7] = [8, 4, 2, 1, 132, 130, 129];
pub use super::RATE_LABELS;
pub const MODES: [&str; 6] = ["办公", "均衡", "火力", "超核", "竞技超核", "狂暴竞技"];
pub const fn allowed_modes(rate: usize) -> &'static [u8] {
    match rate {
        0 | 1 => &[0, 1],
        2 => &[0, 1, 2],
        3 => &[1, 2, 3, 4, 5],
        4 => &[2, 3, 4, 5],
        5 | 6 => &[3, 4, 5],
        _ => &[],
    }
}
#[derive(Clone, Debug)]
pub struct Settings {
    pub slot: u8,
    pub rate: usize,
    pub modes: [u8; 7],
    pub rf: u8,
}
#[derive(Clone, Copy)]
pub enum Change {
    Rate(usize),
    Mode(usize, u8),
    Radio(u8),
}
struct Connection {
    output: HidDevice,
    feature: HidDevice,
    route: u8,
}
fn collection_group(path: &str) -> String {
    let normalized = path
        .to_ascii_lowercase()
        .replace("&col05", "")
        .replace("&col06", "")
        .replace("&col09", "");
    let parts: Vec<_> = normalized.split('#').collect();
    if parts.len() < 3 {
        return normalized;
    }
    let instance = parts[2]
        .rsplit_once('&')
        .map_or(parts[2], |(prefix, _)| prefix);
    format!("{}#{instance}", parts[1])
}
impl Connection {
    fn open(link: Option<bool>) -> Result<Self> {
        Self::open_selected(link, None)
    }
    fn open_selected(link: Option<bool>, path: Option<&str>) -> Result<Self> {
        let api = HidApi::new()?;
        let devices: Vec<_> = api
            .device_list()
            .filter(|d| {
                d.vendor_id() == 0x24ae
                    && matches!(d.product_id(), 0x4660 | 0x1413 | 0x1460)
                    && d.usage_page() == 0xff00
            })
            .collect();
        let mut candidates: Vec<_> = devices.iter().filter(|d| d.usage() == 2).collect();
        if let Some(path) = path {
            candidates.retain(|d| d.path().to_string_lossy().eq_ignore_ascii_case(path));
        }
        if let Some(wireless) = link {
            candidates.retain(|d| (d.product_id() != 0x4660) == wireless);
        }
        if link.is_none() && candidates.iter().any(|d| d.product_id() == 0x4660) {
            candidates.retain(|d| d.product_id() == 0x4660);
        }
        ensure!(
            candidates.len() == 1,
            "请连接一只 VT7 V2 或其无线接收器；多只设备时请先断开其他设备"
        );
        let selected = candidates[0];
        let group = collection_group(&selected.path().to_string_lossy());
        let wireless = selected.product_id() != 0x4660;
        if wireless {
            let battery = selected.open_device(&api)?;
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut verified = false;
            while std::time::Instant::now() < deadline {
                let mut report = [0; 19];
                let size = battery.read_timeout(&mut report, 250)?;
                if size == 19 && report[0] == 7 {
                    ensure!(
                        u16::from_le_bytes([report[13], report[14]]) == 0x4660,
                        "接收器配对的鼠标不是 VT7 V2"
                    );
                    if report[18] & 0x80 != 0 {
                        verified = true;
                        break;
                    }
                }
            }
            ensure!(
                verified,
                "尚未收到 VT7 V2 无线在线报告，请唤醒或移动鼠标后重试"
            );
        }
        let open = |usage| -> Result<HidDevice> {
            Ok(devices
                .iter()
                .find(|d| {
                    d.usage() == usage && collection_group(&d.path().to_string_lossy()) == group
                })
                .context("未找到配置接口")?
                .open_device(&api)?)
        };
        Ok(Self {
            output: open(14)?,
            feature: open(15)?,
            route: if wireless { 0xa5 } else { 0xff },
        })
    }
    fn command(&self, cmd: u8, address: u32, data: &[u8], len: u8) -> Result<Vec<u8>> {
        let mut request = [0; 33];
        request[..4].copy_from_slice(&[6, self.route, cmd, len]);
        request[4..8].copy_from_slice(&address.to_le_bytes());
        request[8..8 + data.len()].copy_from_slice(data);
        ensure!(
            self.output.write(&request)? == request.len(),
            "配置命令发送不完整"
        );
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(20));
            let mut reply = [0; 33];
            reply[0] = 8;
            let size = self.feature.get_feature_report(&mut reply)?;
            ensure!(size >= 4, "配置响应不完整");
            match reply[0] {
                1 => {
                    ensure!(size >= 4 + usize::from(len), "配置数据不完整");
                    return Ok(reply[4..4 + usize::from(len)].to_vec());
                }
                2 => (),
                value => anyhow::bail!("设备拒绝配置命令：{value}"),
            }
        }
        anyhow::bail!("配置响应超时")
    }
    fn read(&self, address: u32, len: u8) -> Result<Vec<u8>> {
        self.command(0xa4, address, &[], len)
    }
    fn settings(&self) -> Result<Settings> {
        let slot = self.read(0x10c, 1)?[0];
        ensure!(slot < 8, "无效配置槽");
        let base = 1536 + u32::from(slot) * 1024;
        let raw = self.read(base + 640, 1)?[0];
        let rate = RATES.iter().position(|r| *r == raw).context("未知回报率")?;
        let modes: [u8; 7] = self
            .read(base + 732, 7)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("模式数据不完整"))?;
        ensure!(modes.iter().all(|m| *m < 6), "未知传感器模式");
        let rf = self.read(base + 728, 1)?[0];
        ensure!(rf <= 1, "未知射频策略");
        ensure!(self.read(0x10c, 1)?[0] == slot, "配置槽已切换，请重试");
        Ok(Settings {
            slot,
            rate,
            modes,
            rf,
        })
    }
}
pub fn load() -> Result<Settings> {
    load_for(None)
}
pub fn load_device(path: &str) -> Result<Settings> {
    let _guard = IO.lock().map_err(|_| anyhow::anyhow!("配置锁异常"))?;
    Connection::open_selected(None, Some(path))?.settings()
}
pub fn load_for(link: Option<bool>) -> Result<Settings> {
    let _guard = IO.lock().map_err(|_| anyhow::anyhow!("配置锁异常"))?;
    Connection::open(link)?.settings()
}
pub fn apply(expected_slot: u8, change: Change) -> Result<Settings> {
    apply_for(None, expected_slot, change)
}
pub fn apply_for(link: Option<bool>, expected_slot: u8, change: Change) -> Result<Settings> {
    let _guard = IO.lock().map_err(|_| anyhow::anyhow!("配置锁异常"))?;
    if let Some(wireless) = link {
        ensure!(
            !automatic(wireless),
            "此连接已启用自动切换，请先关闭自动切换再手动修改"
        );
    }
    let connection = Connection::open(link)?;
    ensure!(
        !automatic(connection.route == 0xa5),
        "此连接已启用自动切换，请先关闭自动切换再手动修改"
    );
    let before = connection.settings()?;
    ensure!(before.slot == expected_slot, "配置槽已切换，请刷新后重试");
    let base = 1536 + u32::from(before.slot) * 1024;
    let (address, value) = match change {
        Change::Rate(index) => {
            ensure!(index < 7, "无效回报率");
            (base + 640, RATES[index])
        }
        Change::Mode(index, mode) => {
            ensure!(allowed_modes(index).contains(&mode), "此回报率不支持该模式");
            (base + 732 + u32::try_from(index)?, mode)
        }
        Change::Radio(value) => {
            ensure!(value <= 1, "无效射频策略");
            (base + 728, value)
        }
    };
    connection.command(0xa5, address, &[value], 1)?;
    REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ensure!(
        connection.read(address, 1)?[0] == value,
        "写入后回读不一致，请刷新检查设备"
    );
    connection.settings()
}

pub fn apply_profile(wireless: bool, rate: usize, mode: u8) -> Result<Settings> {
    ensure!(
        allowed_modes(rate).contains(&mode),
        "目标回报率不支持该传感器模式"
    );
    let _guard = IO.lock().map_err(|_| anyhow::anyhow!("配置锁异常"))?;
    let connection = Connection::open(Some(wireless))?;
    let before = connection.settings()?;
    let base = 1536 + u32::from(before.slot) * 1024;
    let mode_address = base + 732 + u32::try_from(rate)?;
    connection.command(0xa5, mode_address, &[mode], 1)?;
    REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ensure!(
        connection.read(mode_address, 1)?[0] == mode,
        "模式回读不一致"
    );
    ensure!(
        connection.read(0x10c, 1)?[0] == before.slot,
        "配置槽已切换，停止执行"
    );
    connection.command(0xa5, base + 640, &[RATES[rate]], 1)?;
    let after = connection.settings()?;
    ensure!(
        after.slot == before.slot && after.rate == rate && after.modes[rate] == mode,
        "自动配置回读不一致，请刷新检查设备"
    );
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_locks_only_its_connection_before_device_access() {
        set_automatic(true, true);
        assert!(automatic(true));
        assert!(!automatic(false));
        let error = apply_for(Some(true), 0, Change::Rate(3)).unwrap_err();
        set_automatic(true, false);
        assert!(error.to_string().contains("自动切换"));
        assert!(!automatic(true));
    }
    #[test]
    fn collections_share_device_but_not_separate_instances() {
        let first = r"\\?\HID#VID_24AE&PID_1460&MI_01&Col05#8&abcd&0&0004#{guid}";
        let second = r"\\?\HID#VID_24AE&PID_1460&MI_01&Col09#8&abcd&0&0008#{guid}";
        assert_eq!(collection_group(first), collection_group(second));
        assert_ne!(
            collection_group(first),
            collection_group(&second.replace("abcd", "efgh"))
        );
    }
}
