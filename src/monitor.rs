mod fleet;
pub use fleet::watch_all;

use crate::{
    model::{Battery, DeviceIdentity, Reading, ReportKind, Status},
    transport::BatteryTransport,
};
use anyhow::Result;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

pub const STALE_AFTER: Duration = Duration::from_secs(30);

/// Keep showing the last reporting device while another channel is only being
/// tried. Its identity and original reception time remain attached to the data.
fn retain_last_reading(status: Status, last_known: Option<&(DeviceIdentity, Reading)>) -> Status {
    if status.reading().is_some() {
        return status;
    }
    last_known.map_or(status, |(identity, reading)| {
        Status::Stale(identity.clone(), Some(*reading))
    })
}

/// Tracks freshness separately from percentage changes, so constant battery
/// readings still refresh the timestamp. Testable without a HID device or UI.
pub struct Session {
    identity: DeviceIdentity,
    last_report: Instant,
    last_reading: Option<Reading>,
    report_kind: ReportKind,
}

impl Session {
    pub const fn new(
        identity: DeviceIdentity,
        now: Instant,
        cached: Option<Reading>,
        report_kind: ReportKind,
    ) -> Self {
        Self {
            identity,
            last_report: now,
            last_reading: cached,
            report_kind,
        }
    }
    pub fn observe(
        &mut self,
        battery: Option<Battery>,
        now: Instant,
        received_at: SystemTime,
    ) -> Option<Status> {
        if let Some(battery) = battery {
            self.last_report = now;
            let reading = Reading {
                battery,
                received_at,
                kind: self.report_kind,
            };
            self.last_reading = Some(reading);
            Some(Status::Live(self.identity.clone(), reading))
        } else if now.duration_since(self.last_report) >= STALE_AFTER {
            Some(Status::Stale(self.identity.clone(), self.last_reading))
        } else {
            None
        }
    }
}

pub fn watch(
    mut transport: impl BatteryTransport,
    selector: Option<&str>,
    stop: &Arc<AtomicBool>,
    mut publish: impl FnMut(Status),
) {
    let mut previous = None;
    let mut readings = HashMap::new();
    let mut last_known: Option<(DeviceIdentity, Reading)> = None;
    let mut emit = |status: Status| {
        if previous.as_ref() != Some(&status) {
            previous = Some(status.clone());
            publish(status);
        }
    };
    while !stop.load(Ordering::Relaxed) {
        match transport.connect(selector) {
            Ok(mut connection) => {
                let identity = connection.identity().clone();
                let cached = readings.get(&identity.id).copied();
                let initial = cached.map_or_else(
                    || Status::Waiting(identity.clone()),
                    |reading| Status::Stale(identity.clone(), Some(reading)),
                );
                emit(retain_last_reading(initial, last_known.as_ref()));
                let mut session = Session::new(
                    identity.clone(),
                    Instant::now(),
                    cached,
                    connection.report_kind(),
                );
                while !stop.load(Ordering::Relaxed) {
                    match connection.read() {
                        Ok(battery) => {
                            if let Some(status) =
                                session.observe(battery, Instant::now(), SystemTime::now())
                            {
                                if let Some(reading) = status.reading() {
                                    readings.insert(identity.id.clone(), reading);
                                    last_known = Some((identity.clone(), reading));
                                }
                                let stale = matches!(status, Status::Stale(_, _));
                                emit(retain_last_reading(status, last_known.as_ref()));
                                if stale {
                                    break;
                                }
                            }
                        }
                        Err(error) => {
                            let error = format!("设备读取失败：{error:#}");
                            crate::diagnostics::log(&error);
                            let status = session.last_reading.map_or_else(
                                || Status::Offline(error),
                                |reading| Status::Stale(identity.clone(), Some(reading)),
                            );
                            emit(retain_last_reading(status, last_known.as_ref()));
                            break;
                        }
                    }
                }
            }
            Err(error) => {
                let error = format!("{error:#}");
                crate::diagnostics::log(&error);
                emit(last_known.as_ref().map_or_else(
                    || Status::Offline(error),
                    |(identity, reading)| Status::Stale(identity.clone(), Some(*reading)),
                ));
            }
        }
        for _ in 0..10 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            thread::sleep(Duration::from_millis(300));
        }
    }
}

pub fn probe(mut transport: impl BatteryTransport, selector: Option<&str>) -> Result<()> {
    let mut connection = transport.connect(selector)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Some(battery) = connection.read()? {
            let text = format!(
                "电量：{}%\n充电状态：{}",
                battery.percent,
                battery.charging.label()
            );
            println!("{text}");
            crate::diagnostics::log(&format!("Probe: {text}"));
            return Ok(());
        }
    }
    anyhow::bail!("30 秒内未收到电量报告；请移动鼠标或关闭占用设备的驱动后重试")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Charging;
    use crate::transport::BatteryConnection;

    struct ReconnectingTransport {
        generation: usize,
    }
    struct FakeConnection {
        identity: DeviceIdentity,
        reads: usize,
    }

    impl BatteryTransport for ReconnectingTransport {
        fn connect(&mut self, selector: Option<&str>) -> Result<Box<dyn BatteryConnection>> {
            assert!(
                selector.is_none(),
                "Automatic discovery must not require a selector"
            );
            self.generation += 1;
            Ok(Box::new(FakeConnection {
                identity: DeviceIdentity {
                    id: self.generation.to_string(),
                    driver: "other-brand",
                    name: "Other Mouse",
                },
                reads: 0,
            }))
        }
    }
    impl BatteryConnection for FakeConnection {
        fn identity(&self) -> &DeviceIdentity {
            &self.identity
        }
        fn read(&mut self) -> Result<Option<Battery>> {
            self.reads += 1;
            if self.reads > 1 {
                anyhow::bail!("Receiver unplugged");
            }
            Ok(Some(Battery {
                percent: 55,
                charging: Charging::Discharging,
                raw_charging: 0,
                dpi: None,
            }))
        }
    }

    #[test]
    fn automatically_discovers_another_device_after_disconnect() {
        let stop = Arc::new(AtomicBool::new(false));
        let mut events = Vec::new();
        watch(
            ReconnectingTransport { generation: 0 },
            None,
            &stop,
            |status| {
                if matches!(&status,Status::Live(identity,_) if identity.id == "2") {
                    stop.store(true, Ordering::Relaxed);
                }
                events.push(status);
            },
        );
        assert!(
            events
                .iter()
                .any(|status| matches!(status,Status::Live(identity,_) if identity.id == "1"))
        );
        assert!(
            events
                .iter()
                .any(|status| matches!(status, Status::Stale(_, Some(_))))
        );
        assert!(
            events.iter().any(
                |status| matches!(status, Status::Stale(identity, Some(_)) if identity.id == "1")
            ),
            "Scanning another channel must keep the last reporting device's identity"
        );
        assert!(
            !events.iter().any(
                |status| matches!(status, Status::Stale(identity, Some(_)) if identity.id == "2")
            ),
            "A new device must not inherit another device's reading"
        );
        assert!(
            events
                .iter()
                .any(|status| matches!(status,Status::Live(identity,_) if identity.id == "2"))
        );
    }

    #[test]
    fn freshness_and_recovery_do_not_require_percent_change() {
        let identity = DeviceIdentity {
            id: "fake-device".into(),
            driver: "other-brand",
            name: "Other Mouse",
        };
        let now = Instant::now();
        let received_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let battery = Battery {
            percent: 55,
            charging: Charging::Active,
            raw_charging: 2,
            dpi: None,
        };
        let mut session = Session::new(identity.clone(), now, None, ReportKind::Broadcast);
        assert_eq!(
            session.observe(None, now + STALE_AFTER, received_at),
            Some(Status::Stale(identity.clone(), None))
        );
        assert_eq!(
            session.observe(Some(battery), now + STALE_AFTER, received_at),
            Some(Status::Live(
                identity.clone(),
                Reading {
                    battery,
                    received_at,
                    kind: ReportKind::Broadcast,
                }
            ))
        );
        let refreshed_at = received_at + Duration::from_secs(7);
        let refreshed = Reading {
            battery,
            received_at: refreshed_at,
            kind: ReportKind::Broadcast,
        };
        assert_eq!(
            session.observe(
                Some(battery),
                now + STALE_AFTER + Duration::from_secs(7),
                refreshed_at
            ),
            Some(Status::Live(identity.clone(), refreshed))
        );
        assert_eq!(
            session.observe(
                None,
                now + STALE_AFTER + Duration::from_secs(31),
                refreshed_at + Duration::from_secs(24)
            ),
            None
        );
        assert_eq!(
            session.observe(
                None,
                now + STALE_AFTER + Duration::from_secs(37),
                refreshed_at + Duration::from_secs(30)
            ),
            Some(Status::Stale(identity.clone(), Some(refreshed)))
        );
        let mut reconnected = Session::new(
            identity.clone(),
            now + Duration::from_secs(70),
            session.last_reading,
            ReportKind::Broadcast,
        );
        assert_eq!(
            reconnected.observe(
                None,
                now + Duration::from_secs(100),
                received_at + Duration::from_secs(100)
            ),
            Some(Status::Stale(identity.clone(), Some(refreshed)))
        );
        let recovered_at = received_at + Duration::from_secs(101);
        assert_eq!(
            reconnected.observe(Some(battery), now + Duration::from_secs(101), recovered_at),
            Some(Status::Live(
                identity,
                Reading {
                    battery,
                    received_at: recovered_at,
                    kind: ReportKind::Broadcast,
                }
            ))
        );
    }

    #[test]
    fn reconnect_without_broadcast_keeps_the_original_reading() {
        struct SameDeviceTransport {
            generation: usize,
            stop: Arc<AtomicBool>,
        }
        struct SilentConnection {
            identity: DeviceIdentity,
            stop: Arc<AtomicBool>,
        }
        impl BatteryTransport for SameDeviceTransport {
            fn connect(&mut self, _: Option<&str>) -> Result<Box<dyn BatteryConnection>> {
                self.generation += 1;
                let identity = DeviceIdentity {
                    id: "same-device".into(),
                    driver: "test",
                    name: "Test Mouse",
                };
                if self.generation == 1 {
                    Ok(Box::new(FakeConnection { identity, reads: 0 }))
                } else {
                    Ok(Box::new(SilentConnection {
                        identity,
                        stop: Arc::clone(&self.stop),
                    }))
                }
            }
        }
        impl BatteryConnection for SilentConnection {
            fn identity(&self) -> &DeviceIdentity {
                &self.identity
            }
            fn read(&mut self) -> Result<Option<Battery>> {
                self.stop.store(true, Ordering::Relaxed);
                Ok(None)
            }
        }
        let stop = Arc::new(AtomicBool::new(false));
        let mut events = Vec::new();
        watch(
            SameDeviceTransport {
                generation: 0,
                stop: Arc::clone(&stop),
            },
            None,
            &stop,
            |status| events.push(status),
        );
        assert_eq!(events.len(), 3, "Waiting, one broadcast, cached reading");
        assert!(matches!(events[0], Status::Waiting(_)));
        assert!(matches!(events[1], Status::Live(_, _)));
        assert!(matches!(events[2], Status::Stale(_, Some(_))));
        assert_eq!(events[1].reading(), events[2].reading());
    }
}
