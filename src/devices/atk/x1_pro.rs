//! Compx `GetBatteryLevel` response; see docs/ATK X1 PRO/PROTOCOL.md.
use crate::{
    devices::{MouseDriver, ReadMethod, protocols::compx},
    model::{Battery, Charging, HidDescriptor},
};

pub struct X1Pro;

impl MouseDriver for X1Pro {
    fn id(&self) -> &'static str {
        "atk-x1-pro"
    }
    fn name(&self) -> &'static str {
        "ATK X1 PRO"
    }
    fn matches(&self, descriptor: HidDescriptor) -> bool {
        descriptor.vendor_id == 0x373b
            && matches!(descriptor.product_id, 0x1016 | 0x101a | 0x101b)
            && descriptor.usage_page == 0xff02
            && descriptor.usage == 2
    }
    fn connection_priority(&self, descriptor: HidDescriptor) -> u8 {
        if descriptor.product_id == 0x1016 {
            2
        } else {
            1
        }
    }
    fn input_report_len(&self) -> usize {
        17
    }
    fn read_method(&self, descriptor: HidDescriptor) -> ReadMethod {
        ReadMethod::CompxBatteryQuery {
            expected_model: matches!(descriptor.product_id, 0x101a | 0x101b).then_some((2, 38)),
        }
    }
    fn decode(&self, report: &[u8]) -> Option<Battery> {
        decode(report)
    }
}

pub fn decode(report: &[u8]) -> Option<Battery> {
    let data = compx::payload(report, 4, 2)?;
    if data[0] > 100 {
        return None;
    }
    Some(Battery {
        percent: data[0],
        charging: match data[1] {
            0 => Charging::Discharging,
            1 => Charging::Active,
            value => Charging::Unknown(value),
        },
        raw_charging: data[1],
        dpi: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURE: [u8; 17] = [
        8, 4, 0, 0, 0, 2, 0x5f, 1, 0x10, 0x50, 0, 0, 0, 0, 0, 0, 0x87,
    ];

    fn checksum(report: &mut [u8; 17]) {
        report[16] = 0x55u8.wrapping_sub(
            report[..16]
                .iter()
                .fold(0u8, |sum, byte| sum.wrapping_add(*byte)),
        );
    }

    #[test]
    fn native_capture_and_charging_are_independent_of_rapoo() {
        let battery = X1Pro.decode(&CAPTURE).unwrap();
        assert_eq!(battery.percent, 95);
        assert_eq!(battery.charging, Charging::Active);
        for (code, state) in [
            (0, Charging::Discharging),
            (1, Charging::Active),
            (2, Charging::Unknown(2)),
        ] {
            for percent in [0, 100] {
                let mut report = CAPTURE;
                report[6] = percent;
                report[7] = code;
                checksum(&mut report);
                let battery = X1Pro.decode(&report).unwrap();
                assert_eq!(battery.percent, percent);
                assert_eq!(battery.charging, state);
            }
        }
    }

    #[test]
    fn rejects_corrupt_unrelated_and_failed_responses() {
        for size in 0..CAPTURE.len() {
            assert!(decode(&CAPTURE[..size]).is_none());
        }
        for (offset, value) in [(0, 9), (1, 10), (2, 0xff), (5, 1), (5, 11), (6, 101)] {
            let mut report = CAPTURE;
            report[offset] = value;
            checksum(&mut report);
            assert!(decode(&report).is_none());
        }
        let mut report = CAPTURE;
        report[6] ^= 1;
        assert!(
            decode(&report).is_none(),
            "Corrupt checksum must not change the displayed battery"
        );
    }

    #[test]
    fn only_matches_the_verified_query_collection() {
        let target = HidDescriptor {
            vendor_id: 0x373b,
            product_id: 0x1016,
            usage_page: 0xff02,
            usage: 2,
        };
        assert!(X1Pro.matches(target));
        for product_id in [0x101a, 0x101b] {
            let receiver = HidDescriptor {
                product_id,
                ..target
            };
            assert!(X1Pro.matches(receiver));
            assert_eq!(X1Pro.connection_priority(receiver), 1);
            assert_eq!(
                X1Pro.read_method(receiver),
                ReadMethod::CompxBatteryQuery {
                    expected_model: Some((2, 38))
                }
            );
            assert!(!X1Pro.matches(HidDescriptor {
                usage_page: 0xff04,
                ..receiver
            }));
            assert!(!X1Pro.matches(HidDescriptor {
                usage: 0,
                ..receiver
            }));
        }
        assert_eq!(X1Pro.connection_priority(target), 2);
        assert_eq!(
            X1Pro.read_method(target),
            ReadMethod::CompxBatteryQuery {
                expected_model: None
            }
        );
        for usage_page in [1, 0xff03, 0xff04, 0xff05, 0xff06] {
            assert!(!X1Pro.matches(HidDescriptor {
                usage_page,
                ..target
            }));
        }
        for product_id in [0x1017, 0x101c] {
            assert!(!X1Pro.matches(HidDescriptor {
                product_id,
                ..target
            }));
        }
        assert!(!X1Pro.matches(HidDescriptor {
            vendor_id: 0x24ae,
            ..target
        }));
    }

    #[test]
    fn wireless_capture_preserves_actual_charging_status() {
        for (capture, percent) in [
            (
                [8, 4, 0, 0, 0, 2, 0x5f, 0, 0x10, 5, 0, 0, 0, 0, 0, 0, 0xd3],
                95,
            ),
            (
                [
                    8, 4, 0, 0, 0, 2, 0x5a, 0, 0x0f, 0xe8, 0, 0, 0, 0, 0, 0, 0xf6,
                ],
                90,
            ),
        ] {
            let battery = decode(&capture).unwrap();
            assert_eq!(battery.percent, percent);
            assert_eq!(battery.charging, Charging::Discharging);
            assert_eq!(battery.raw_charging, 0);
        }
    }
}
