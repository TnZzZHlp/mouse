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
            let label = if matches!(status, Status::Stale(_, _)) {
                "上次状态"
            } else {
                "电量"
            };
            format!(
                "{label}：{}% · {}",
                reading.battery.percent,
                reading.battery.charging.label()
            )
        },
    );
    format!("设备：{name}\n{battery}")
}

pub fn detail_text(status: &Status) -> String {
    let mut detail = status.reading().map_or_else(
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
    );
    if matches!(status, Status::Stale(_, _)) {
        detail.push_str("\n状态未更新，正在重新连接");
    }
    detail
}

pub fn device_summary(state: &mouse::fleet::DeviceState) -> String {
    use mouse::fleet::Availability;
    let availability = match state.availability {
        Availability::Waiting => "等待报告",
        Availability::Live => "在线",
        Availability::Stale => "数据过期",
        Availability::Offline => "离线",
    };
    state.reading.map_or_else(
        || format!("{availability} · 电量未知"),
        |reading| {
            let prefix = if state.availability == Availability::Live {
                ""
            } else {
                "上次 "
            };
            format!(
                "{availability} · {prefix}{}% · {}",
                reading.battery.percent,
                reading.battery.charging.label()
            )
        },
    )
}

pub fn connection_label(state: &mouse::fleet::DeviceState) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    state.identity.id.hash(&mut hasher);
    format!(
        "{} · {:08x}",
        state.identity.driver,
        hasher.finish() & 0xffff_ffff
    )
}

pub fn fleet_tooltip(fleet: &mouse::fleet::Fleet) -> String {
    if fleet.devices.is_empty() {
        return fleet
            .discovery_error
            .clone()
            .unwrap_or_else(|| "正在查找支持的鼠标".into());
    }
    let text = fleet
        .devices
        .values()
        .enumerate()
        .map(|(index, state)| {
            format!(
                "{} · {}: {}",
                state.identity.name,
                index + 1,
                device_summary(state)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut tooltip = String::new();
    for character in text.chars() {
        if tooltip.encode_utf16().count() + character.len_utf16() > 120 {
            tooltip.push_str("...");
            break;
        }
        tooltip.push(character);
    }
    tooltip
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
        let stale = Status::Stale(device.clone(), Some(reading));
        assert_eq!(status_text(&live), "设备：Test Mouse\n电量：76% · 正在充电");
        assert_eq!(
            status_text(&stale),
            "设备：Test Mouse\n上次状态：76% · 正在充电"
        );
        assert_eq!(
            detail_text(&stale),
            format!("{}\n状态未更新，正在重新连接", detail_text(&live))
        );
        assert_eq!(live.reading(), stale.reading());
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
        // A receiver timeout while plugging in a charger must not present the
        // previous discharging report as a current charging measurement.
        let discharging = Reading {
            battery: Battery {
                charging: Charging::Discharging,
                raw_charging: 0,
                ..reading.battery
            },
            kind: ReportKind::QueryResponse,
            ..reading
        };
        let stale = Status::Stale(device.clone(), Some(discharging));
        assert_eq!(
            status_text(&stale),
            "设备：Test Mouse\n上次状态：76% · 未充电"
        );
        assert!(detail_text(&stale).starts_with("上次上报：2026-10-"));
        assert!(detail_text(&stale).ends_with("状态未更新，正在重新连接"));
        let recovered = Status::Live(
            device,
            Reading {
                kind: ReportKind::QueryResponse,
                ..reading
            },
        );
        assert_eq!(
            status_text(&recovered),
            "设备：Test Mouse\n电量：76% · 正在充电"
        );
        assert!(!detail_text(&recovered).contains("重新连接"));
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
