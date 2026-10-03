use mouse::model::{Charging, Status};
use tray_icon::Icon;

const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 1, 1],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];

fn pixel(rgba: &mut [u8], x: usize, y: usize, color: [u8; 4]) {
    if x < 32 && y < 32 {
        let offset = (y * 32 + x) * 4;
        rgba[offset..offset + 4].copy_from_slice(&color);
    }
}

pub fn render(status: &Status) -> Icon {
    let mut rgba = vec![0; 32 * 32 * 4];
    let (text, color, charging) = status.reading().map_or_else(
        || ("?".into(), [170, 180, 195, 255], false),
        |reading| {
            let battery = reading.battery;
            (
                battery.percent.to_string(),
                if matches!(battery.charging, Charging::Active | Charging::Full) {
                    [74, 222, 128, 255]
                } else if battery.percent <= 20 {
                    [255, 100, 100, 255]
                } else {
                    [240, 248, 255, 255]
                },
                matches!(battery.charging, Charging::Active | Charging::Full),
            )
        },
    );
    for y in 1..31 {
        for x in 1..31 {
            pixel(&mut rgba, x, y, [26, 35, 49, 255]);
        }
    }
    let scale = 3;
    let width = text.len() * 10 - 1;
    let start = (32 - width) / 2;
    for (index, ch) in text.bytes().enumerate() {
        let rows = if ch.is_ascii_digit() {
            DIGITS[usize::from(ch - b'0')]
        } else {
            [7, 1, 3, 0, 2]
        };
        for (row, bits) in rows.into_iter().enumerate() {
            for col in 0..3 {
                if bits & (1 << (2 - col)) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            pixel(
                                &mut rgba,
                                start + index * 10 + col * scale + dx,
                                4 + row * scale + dy,
                                color,
                            );
                        }
                    }
                }
            }
        }
    }
    if charging {
        for (row, bits) in [0b00110, 0b01100, 0b11111, 0b00110, 0b01100]
            .into_iter()
            .enumerate()
        {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    for dx in 0..2 {
                        pixel(&mut rgba, 11 + col * 2 + dx, 23 + row, color);
                    }
                }
            }
        }
    } else if let Some(reading) = status.reading() {
        let battery = reading.battery;
        let filled = usize::from(battery.percent) * 24 / 100;
        for y in 24..27 {
            for x in 4..28 {
                pixel(
                    &mut rgba,
                    x,
                    y,
                    if x - 4 < filled {
                        color
                    } else {
                        [67, 77, 90, 255]
                    },
                );
            }
        }
    }
    Icon::from_rgba(rgba, 32, 32).expect("fixed 32×32 RGBA icon")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mouse::model::{Battery, DeviceIdentity, Reading};

    #[test]
    fn renders_full_percent_range_and_all_availability_states() {
        let device = DeviceIdentity {
            id: "test".into(),
            driver: "another-mouse",
            name: "Another Mouse",
        };
        for percent in 0..=100 {
            for charging in [
                Charging::Discharging,
                Charging::Active,
                Charging::Full,
                Charging::Unknown(255),
            ] {
                let reading = Reading {
                    battery: Battery {
                        percent,
                        charging,
                        raw_charging: 0,
                        dpi: None,
                    },
                    received_at: std::time::UNIX_EPOCH,
                    kind: mouse::model::ReportKind::Broadcast,
                };
                render(&Status::Live(device.clone(), reading));
                render(&Status::Stale(device.clone(), Some(reading)));
            }
        }
        for status in [
            Status::Searching,
            Status::Waiting(device.clone()),
            Status::Stale(device, None),
            Status::Offline("test".into()),
        ] {
            render(&status);
        }
    }
}
