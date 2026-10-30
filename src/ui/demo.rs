use mouse::{
    fleet::{Availability, DeviceState, Fleet},
    model::{Battery, Charging, DeviceIdentity, Reading, ReportKind},
};
use std::time::SystemTime;

pub fn snapshot(step: usize) -> Fleet {
    let mut fleet = Fleet::default();
    for (index, (name, driver, percent, charging)) in [
        ("Rapoo VT7", "rapoo-vt7", 76, Charging::Discharging),
        ("ATK X1 PRO", "atk-x1-pro", 18, Charging::Discharging),
        ("ATK X1 PRO", "atk-x1-pro", 42, Charging::Active),
    ]
    .into_iter()
    .enumerate()
    {
        fleet.apply(DeviceState {
            identity: DeviceIdentity {
                id: format!("demo-{index}"),
                driver,
                name,
            },
            availability: match (step % 4, index) {
                (1, 1) => Availability::Stale,
                (2, 1) => Availability::Offline,
                _ => Availability::Live,
            },
            reading: Some(Reading {
                battery: Battery {
                    percent,
                    charging,
                    raw_charging: 0,
                    dpi: None,
                },
                received_at: SystemTime::now(),
                kind: if index == 0 {
                    ReportKind::Broadcast
                } else {
                    ReportKind::QueryResponse
                },
            }),
            error: None,
        });
    }
    fleet
}
