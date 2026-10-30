use crate::model::{DeviceIdentity, Reading, Status};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Waiting,
    Live,
    Stale,
    Offline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceState {
    pub identity: DeviceIdentity,
    pub availability: Availability,
    pub reading: Option<Reading>,
    pub error: Option<String>,
}

impl DeviceState {
    pub const fn new(identity: DeviceIdentity) -> Self {
        Self {
            identity,
            availability: Availability::Waiting,
            reading: None,
            error: None,
        }
    }

    pub fn status(&self) -> Status {
        match self.availability {
            Availability::Live => self.reading.map_or_else(
                || Status::Waiting(self.identity.clone()),
                |reading| Status::Live(self.identity.clone(), reading),
            ),
            Availability::Waiting if self.reading.is_none() => {
                Status::Waiting(self.identity.clone())
            }
            _ => Status::Stale(self.identity.clone(), self.reading),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fleet {
    pub devices: BTreeMap<String, DeviceState>,
    pub discovery_error: Option<String>,
}

impl Fleet {
    pub fn apply(&mut self, mut update: DeviceState) {
        if let Some(previous) = self.devices.get(&update.identity.id)
            && update.reading.is_none()
        {
            update.reading = previous.reading;
            if update.availability == Availability::Waiting && update.reading.is_some() {
                update.availability = Availability::Stale;
            }
        }
        self.devices.insert(update.identity.id.clone(), update);
    }

    pub fn reconcile(&mut self, discovered: &[DeviceIdentity]) {
        for state in self.devices.values_mut() {
            if !discovered
                .iter()
                .any(|device| device.id == state.identity.id)
            {
                state.availability = Availability::Offline;
                state.error = None;
            }
        }
        for identity in discovered {
            self.devices
                .entry(identity.id.clone())
                .or_insert_with(|| DeviceState::new(identity.clone()));
        }
    }

    pub fn lowest_live(&self) -> Option<&DeviceState> {
        self.devices
            .values()
            .filter(|state| state.availability == Availability::Live && state.reading.is_some())
            .min_by_key(|state| state.reading.map(|reading| reading.battery.percent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Battery, Charging, ReportKind};

    #[test]
    fn disconnect_and_reconnect_preserve_only_the_same_instances_reading() {
        let first = DeviceIdentity {
            id: "a".into(),
            driver: "test",
            name: "Mouse",
        };
        let second = DeviceIdentity {
            id: "b".into(),
            ..first
        };
        let reading = Reading {
            battery: Battery {
                percent: 18,
                charging: Charging::Discharging,
                raw_charging: 0,
                dpi: None,
            },
            received_at: std::time::UNIX_EPOCH,
            kind: ReportKind::Broadcast,
        };
        let mut fleet = Fleet::default();
        fleet.apply(DeviceState {
            identity: first.clone(),
            availability: Availability::Live,
            reading: Some(reading),
            error: None,
        });
        fleet.reconcile(std::slice::from_ref(&second));
        assert_eq!(fleet.devices["a"].availability, Availability::Offline);
        assert_eq!(fleet.devices["a"].reading, Some(reading));
        assert_eq!(fleet.devices["b"].reading, None);
        fleet.apply(DeviceState::new(first));
        assert_eq!(fleet.devices["a"].availability, Availability::Stale);
        assert_eq!(fleet.devices["a"].reading, Some(reading));
    }

    #[test]
    fn tray_minimum_excludes_old_readings() {
        let mut fleet = Fleet::default();
        for (id, percent, availability) in [
            ("a", 5, Availability::Offline),
            ("b", 40, Availability::Live),
            ("c", 18, Availability::Live),
        ] {
            fleet.apply(DeviceState {
                identity: DeviceIdentity {
                    id: id.into(),
                    driver: "test",
                    name: "Mouse",
                },
                availability,
                reading: Some(Reading {
                    battery: Battery {
                        percent,
                        charging: Charging::Discharging,
                        raw_charging: 0,
                        dpi: None,
                    },
                    received_at: std::time::UNIX_EPOCH,
                    kind: ReportKind::Broadcast,
                }),
                error: None,
            });
        }
        assert_eq!(
            fleet.lowest_live().map(|state| state.identity.id.as_str()),
            Some("c")
        );
    }
}
