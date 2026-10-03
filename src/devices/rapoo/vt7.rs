//! Shared wired/receiver Report 7 decoder; see docs/Rapoo VT7/USB.md.
use crate::{
    devices::MouseDriver,
    model::{Battery, Charging, HidDescriptor},
};

pub struct Vt7;

impl MouseDriver for Vt7 {
    fn id(&self) -> &'static str {
        "rapoo-vt7"
    }
    fn name(&self) -> &'static str {
        "Rapoo VT7"
    }
    fn matches(&self, descriptor: HidDescriptor) -> bool {
        descriptor.vendor_id == 0x24ae
            && matches!(descriptor.product_id, 0x1413 | 0x4613)
            && descriptor.usage_page == 0xff00
            && descriptor.usage == 2
    }
    fn connection_priority(&self, descriptor: HidDescriptor) -> u8 {
        u8::from(descriptor.product_id == 0x4613)
    }
    fn input_report_len(&self) -> usize {
        19
    }
    fn decode(&self, report: &[u8]) -> Option<Battery> {
        decode(report)
    }
}

pub const fn decode(report: &[u8]) -> Option<Battery> {
    if report.len() < 19
        || report[0] != 7
        || report[8] > 100
        || u16::from_le_bytes([report[13], report[14]]) != 0x4613
    {
        return None;
    }
    Some(Battery {
        percent: report[8],
        charging: match report[7] {
            // 0 is the documented VT7 capture. Official UI: 1 normal, 2 charging.
            0 | 1 => Charging::Discharging,
            2 => Charging::Active,
            value => Charging::Unknown(value),
        },
        raw_charging: report[7],
        dpi: Some((
            u16::from_le_bytes([report[3], report[4]]),
            u16::from_le_bytes([report[5], report[6]]),
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: [u8; 19] = [
        7, 0x10, 3, 0x4c, 4, 0x4c, 4, 0, 100, 0, 3, 1, 0, 0x13, 0x46, 1, 2, 2, 0xb6,
    ];

    #[test]
    fn native_capture_and_charging_codes() {
        let battery = decode(&SAMPLE).unwrap();
        assert_eq!(battery.percent, 100);
        assert_eq!(battery.dpi, Some((1100, 1100)));
        for (code, state) in [
            (0, Charging::Discharging),
            (1, Charging::Discharging),
            (2, Charging::Active),
            (255, Charging::Unknown(255)),
        ] {
            let mut report = SAMPLE;
            report[7] = code;
            report[8] = 0;
            let battery = decode(&report).unwrap();
            assert_eq!(battery.charging, state);
            assert_eq!(battery.raw_charging, code);
            assert_eq!(battery.percent, 0);
        }
    }

    #[test]
    fn rejects_bad_reports_but_ignores_reserved_byte() {
        for size in 0..19 {
            assert!(decode(&SAMPLE[..size]).is_none());
        }
        for (offset, value) in [(0, 8), (8, 101), (13, 0), (14, 0)] {
            let mut report = SAMPLE;
            report[offset] = value;
            assert!(decode(&report).is_none());
        }
        let mut report = SAMPLE;
        report[18] = 0;
        assert_eq!(decode(&report), decode(&SAMPLE));
    }

    #[test]
    fn only_matches_battery_collection() {
        let target = HidDescriptor {
            vendor_id: 0x24ae,
            product_id: 0x1413,
            usage_page: 0xff00,
            usage: 2,
        };
        assert!(Vt7.matches(target));
        for product_id in [0x1413, 0x4613] {
            let descriptor = HidDescriptor {
                product_id,
                ..target
            };
            assert!(Vt7.matches(descriptor));
            for usage in [1, 14, 15, 16, 17, 18] {
                assert!(!Vt7.matches(HidDescriptor {
                    usage,
                    ..descriptor
                }));
            }
        }
        assert!(!Vt7.matches(HidDescriptor {
            product_id: 0x4614,
            ..target
        }));
        assert!(!Vt7.matches(HidDescriptor {
            usage_page: 1,
            ..target
        }));
    }

    #[test]
    fn wired_capture_uses_the_same_battery_layout() {
        let report = [
            7, 0x12, 3, 0x4c, 4, 0x4c, 4, 2, 100, 0, 3, 1, 0, 0x13, 0x46, 1, 2, 2, 0,
        ];
        let battery = Vt7.decode(&report).unwrap();
        assert_eq!(battery.percent, 100);
        assert_eq!(battery.charging, Charging::Active);
        assert_eq!(battery.dpi, Some((1100, 1100)));
        assert_eq!(report[1] & 15, 2, "Observed wired connection mode");
    }
}
