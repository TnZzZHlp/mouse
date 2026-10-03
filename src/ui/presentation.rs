//! Device-independent text presentation. UI never inspects raw report bytes.
use mouse::model::{ReportKind, Status};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn status_text(status: &Status) -> String {
    let name = match status {
        Status::Waiting(device) | Status::Live(device, _) | Status::Stale(device, _) => device.name,
        Status::Searching | Status::Offline(_) => "未识别",
    };
    let battery = status.reading().map_or_else(
        || "电量：未知 · 充电状态：未知".into(),
        |reading| {
            format!(
                "电量：{}% · {}",
                reading.battery.percent,
                reading.battery.charging.label()
            )
        },
    );
    format!("设备：{name}\n{battery}")
}

pub fn detail_text(status: &Status) -> String {
    status.reading().map_or_else(
        || "上次广播：尚未收到".into(),
        |reading| {
            format!(
                "{}：{}",
                match reading.kind {
                    ReportKind::Broadcast => "上次广播",
                    ReportKind::QueryResponse => "上次上报",
                },
                local_time(reading.received_at).unwrap_or_else(|| "时间不可用".into())
            )
        },
    )
}

fn local_time(time: SystemTime) -> Option<String> {
    // FILETIME counts 100 ns intervals from 1601-01-01; SystemTime uses 1970.
    let elapsed = time.duration_since(UNIX_EPOCH).ok()?;
    let ticks = elapsed
        .as_secs()
        .checked_add(11_644_473_600)?
        .checked_mul(10_000_000)?
        .checked_add(u64::from(elapsed.subsec_nanos() / 100))?;
    let utc = winsafe::FileTimeToSystemTime(&winsafe::FILETIME::from(ticks)).ok()?;
    let local = winsafe::SystemTimeToTzSpecificLocalTime(None, &utc).ok()?;
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute, local.wSecond
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mouse::model::{Battery, Charging, DeviceIdentity, Reading};

    #[test]
    fn stale_reading_keeps_battery_charging_and_broadcast_time() {
        let device = DeviceIdentity {
            id: "test".into(),
            driver: "test-driver",
            name: "Test Mouse",
        };
        let reading = Reading {
            battery: Battery {
                percent: 76,
                charging: Charging::Active,
                raw_charging: 2,
                dpi: Some((1100, 1100)),
            },
            received_at: UNIX_EPOCH + std::time::Duration::from_secs(1_791_003_601),
            kind: ReportKind::Broadcast,
        };
        let live = Status::Live(device.clone(), reading);
        let stale = Status::Stale(device, Some(reading));
        assert_eq!(status_text(&live), "设备：Test Mouse\n电量：76% · 正在充电");
        assert_eq!(status_text(&live), status_text(&stale));
        assert_eq!(detail_text(&live), detail_text(&stale));
        let detail = detail_text(&live);
        assert!(detail.starts_with("上次广播：2026-10-"), "{detail}");
        assert!(!detail.contains("时间不可用"));
        let queried = Status::Live(
            device_for_query(),
            Reading {
                kind: ReportKind::QueryResponse,
                ..reading
            },
        );
        assert!(detail_text(&queried).starts_with("上次上报：2026-10-"));
        assert_eq!(
            status_text(&queried),
            "设备：Query Mouse\n电量：76% · 正在充电"
        );
    }

    fn device_for_query() -> DeviceIdentity {
        DeviceIdentity {
            id: "query".into(),
            driver: "query-driver",
            name: "Query Mouse",
        }
    }

    #[test]
    fn no_broadcast_does_not_invent_a_battery_or_timestamp() {
        for status in [Status::Searching, Status::Offline("test".into())] {
            assert_eq!(
                status_text(&status),
                "设备：未识别\n电量：未知 · 充电状态：未知"
            );
            assert_eq!(detail_text(&status), "上次广播：尚未收到");
        }
    }
}
