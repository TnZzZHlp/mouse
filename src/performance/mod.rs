//! Shared performance API. Device protocols stay in their own backends.
pub mod atk;
pub mod rapoo;

use anyhow::Result;

pub const RATE_LABELS: [&str; 7] = [
    "125 Hz", "250 Hz", "500 Hz", "1000 Hz", "2000 Hz", "4000 Hz", "8000 Hz",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Vt7V2,
    AtkX1Pro,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub slot: u8,
    pub rate: usize,
    pub mode: u8,
    pub max_rate: usize,
    pub radio: Option<u8>,
}

#[derive(Clone, Copy)]
pub enum Change {
    Rate(usize),
    Mode(usize, u8),
    Radio(u8),
}

impl From<rapoo::Settings> for Settings {
    fn from(value: rapoo::Settings) -> Self {
        Self {
            slot: value.slot,
            rate: value.rate,
            mode: value.modes[value.rate],
            max_rate: 6,
            radio: Some(value.rf),
        }
    }
}
impl From<atk::Settings> for Settings {
    fn from(value: atk::Settings) -> Self {
        Self {
            slot: value.slot,
            rate: value.rate,
            mode: value.mode,
            max_rate: value.max_rate,
            radio: None,
        }
    }
}

impl Device {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Vt7V2 => "VT7 V2",
            Self::AtkX1Pro => "ATK X1 PRO",
        }
    }
    pub const fn allowed_modes(self, rate: usize) -> &'static [u8] {
        match self {
            Self::Vt7V2 => rapoo::allowed_modes(rate),
            Self::AtkX1Pro => &[0, 1, 2],
        }
    }
    pub const fn mode_labels(self) -> &'static [&'static str] {
        match self {
            Self::Vt7V2 => &rapoo::MODES,
            Self::AtkX1Pro => &atk::MODES,
        }
    }
    pub fn automatic(self, wireless: bool) -> bool {
        self == Self::Vt7V2 && rapoo::automatic(wireless)
    }
    pub fn load(self, wireless: bool) -> Result<Settings> {
        match self {
            Self::Vt7V2 => rapoo::load_for(Some(wireless)).map(Into::into),
            Self::AtkX1Pro => atk::load(wireless).map(Into::into),
        }
    }
    pub fn apply(self, wireless: bool, slot: u8, change: Change) -> Result<Settings> {
        match self {
            Self::Vt7V2 => {
                let change = match change {
                    Change::Rate(rate) => rapoo::Change::Rate(rate),
                    Change::Mode(rate, mode) => rapoo::Change::Mode(rate, mode),
                    Change::Radio(value) => rapoo::Change::Radio(value),
                };
                rapoo::apply_for(Some(wireless), slot, change).map(Into::into)
            }
            Self::AtkX1Pro => {
                let change = match change {
                    Change::Rate(rate) => atk::Change::Rate(rate),
                    Change::Mode(_, mode) => atk::Change::Mode(mode),
                    Change::Radio(_) => anyhow::bail!("ATK X1 PRO 不支持此射频设置"),
                };
                atk::apply(wireless, slot, change).map(Into::into)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capabilities_and_settings_remain_device_specific() {
        let atk: Settings = atk::Settings {
            slot: 0,
            rate: 6,
            mode: 2,
            max_rate: 3,
        }
        .into();
        assert_eq!(atk.max_rate, 3);
        assert_eq!(atk.radio, None);
        assert_eq!(Device::AtkX1Pro.allowed_modes(0), &[0, 1, 2]);
        assert_eq!(Device::Vt7V2.allowed_modes(0), &[0, 1]);
        let rapoo: Settings = rapoo::Settings {
            slot: 1,
            rate: 3,
            modes: [0, 0, 0, 4, 3, 3, 3],
            rf: 1,
        }
        .into();
        assert_eq!(rapoo.mode, 4);
        assert_eq!(rapoo.radio, Some(1));
        assert!(!Device::AtkX1Pro.automatic(true));
    }
}
