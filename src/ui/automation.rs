use gpui::{
    Context, FocusHandle, IntoElement, KeyDownEvent, Render, Timer, Window, div, prelude::*, rgb,
};
use mouse::{fleet::Fleet, performance::rapoo as performance};
use std::{
    sync::mpsc::{self, Receiver},
    time::Duration,
};

#[derive(Clone)]
struct Rule {
    min: u8,
    max: u8,
    rate: usize,
    mode: u8,
}
struct Editing {
    index: usize,
    upper: bool,
    text: String,
    cursor: usize,
    selected: bool,
}
fn validate(rules: &[Rule]) -> Result<(), String> {
    if rules.is_empty() {
        return Err("请先添加电量区间".into());
    }
    for (index, rule) in rules.iter().enumerate() {
        if rule.min > rule.max || rule.max > 100 {
            return Err(format!(
                "区间 {}：下限不能高于上限，电量须为 0–100%",
                index + 1
            ));
        }
        if !performance::allowed_modes(rule.rate).contains(&rule.mode) {
            return Err("回报率与模式不兼容".into());
        }
        if rules[..index]
            .iter()
            .any(|r| rule.min <= r.max && r.min <= rule.max)
        {
            return Err("区间不能重叠，例如 0–20% 和 21–50%".into());
        }
    }
    Ok(())
}
fn matching(rules: &[Rule], percent: u8) -> Option<usize> {
    rules
        .iter()
        .position(|r| (r.min..=r.max).contains(&percent))
}
fn connection_target(fleet: &Fleet) -> Option<(&mouse::fleet::DeviceState, bool)> {
    if let Some(wired) = fleet
        .connected()
        .find(|s| s.identity.id.to_ascii_lowercase().contains("pid_4660"))
    {
        return Some((wired, false));
    }
    fleet
        .connected()
        .find(|s| {
            let id = s.identity.id.to_ascii_lowercase();
            s.identity.driver == "rapoo-vt7"
                && (id.contains("pid_1413") || id.contains("pid_1460"))
                && s.availability == mouse::fleet::Availability::Live
        })
        .map(|s| (s, true))
}

pub struct Automation {
    focus: FocusHandle,
    editing: Option<Editing>,
    enabled: bool,
    wired_rate: usize,
    wired_mode: u8,
    rules: Vec<Rule>,
    active: Option<(String, usize)>,
    message: String,
    pending: Option<Receiver<Result<(), String>>>,
}
impl Automation {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(200)).await;
                if this
                    .update(cx, |view, cx| {
                        if let Some(result) = view.pending.as_ref().and_then(|r| r.try_recv().ok())
                        {
                            view.pending = None;
                            match result {
                                Ok(()) => view.message = "配置已应用并核对".into(),
                                Err(e) => {
                                    view.enabled = false;
                                    performance::set_automatic(true, false);
                                    view.active = None;
                                    view.message = format!("执行失败，已暂停：{e}");
                                }
                            }
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            focus: cx.focus_handle(),
            editing: None,
            enabled: false,
            wired_rate: 3,
            wired_mode: 2,
            rules: vec![Rule {
                min: 0,
                max: 20,
                rate: 2,
                mode: 0,
            }],
            active: None,
            message: "默认关闭，请编辑电量区间".into(),
            pending: None,
        }
    }
    pub fn observe(&mut self, fleet: &Fleet, cx: &mut Context<Self>) {
        if !self.enabled || self.pending.is_some() {
            return;
        }
        let Some((device, wireless)) = connection_target(fleet) else {
            self.active = None;
            return;
        };
        if !wireless {
            let target = (device.identity.id.clone(), usize::MAX);
            if self.active.as_ref() == Some(&target) {
                return;
            }
            self.active = Some(target);
            self.message = "已接入数据线，正在应用有线配置…".into();
            self.apply(false, self.wired_rate, self.wired_mode, cx);
            return;
        }
        let Some(reading) = device.reading else {
            return;
        };
        let Some(index) = matching(&self.rules, reading.battery.percent) else {
            if self.active.take().is_some() {
                self.message = "当前电量没有匹配区间，保持现有配置".into();
                cx.notify();
            }
            return;
        };
        let target = (device.identity.id.clone(), index);
        if self.active.as_ref() == Some(&target) {
            return;
        }
        self.active = Some(target);
        let rule = self.rules[index].clone();
        self.message = format!(
            "电量 {}%：正在应用 {}–{}% 区间配置…",
            reading.battery.percent, rule.min, rule.max
        );
        self.apply(true, rule.rate, rule.mode, cx);
    }
    fn apply(&mut self, wireless: bool, rate: usize, mode: u8, cx: &mut Context<Self>) {
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            let result = performance::apply_profile(wireless, rate, mode)
                .map(|_| ())
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(result);
        });
        cx.notify();
    }
    fn edited(&mut self, cx: &mut Context<Self>) {
        self.active = None;
        self.message = validate(&self.rules)
            .err()
            .unwrap_or_else(|| "区间已更新，启用后生效".into());
        cx.notify();
    }
    fn commit(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(edit) = &self.editing else {
            return true;
        };
        let Ok(value) = edit.text.parse::<u8>().ok().filter(|v| *v <= 100).ok_or(()) else {
            self.message = "请输入 0–100 的整数".into();
            cx.notify();
            return false;
        };
        let rule = &mut self.rules[edit.index];
        if edit.upper {
            rule.max = value;
        } else {
            rule.min = value;
        }
        self.editing = None;
        self.edited(cx);
        true
    }
    fn start_edit(
        &mut self,
        index: usize,
        upper: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.commit(cx) {
            return;
        }
        let rule = &self.rules[index];
        let text = if upper { rule.max } else { rule.min }.to_string();
        self.editing = Some(Editing {
            index,
            upper,
            cursor: text.len(),
            text,
            selected: true,
        });
        window.focus(&self.focus);
        cx.notify();
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.enabled || self.editing.is_none() {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "enter" {
            self.commit(cx);
            cx.stop_propagation();
            return;
        }
        if key == "escape" {
            self.editing = None;
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if key == "tab" {
            let edit = self.editing.as_ref().unwrap();
            let (index, upper) = (edit.index, edit.upper);
            let (index, upper) = if upper {
                ((index + 1) % self.rules.len(), false)
            } else {
                (index, true)
            };
            self.start_edit(index, upper, window, cx);
            cx.stop_propagation();
            return;
        }
        let edit = self.editing.as_mut().unwrap();
        if event.keystroke.modifiers.control && key == "a" {
            edit.selected = true;
        } else if event.keystroke.modifiers.control && key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                insert_digits(edit, text.trim());
            }
        } else if !event.keystroke.modifiers.control && !event.keystroke.modifiers.alt {
            match key {
                "backspace" | "delete" => {
                    if edit.selected {
                        edit.text.clear();
                        edit.cursor = 0;
                        edit.selected = false;
                    } else if key == "backspace" && edit.cursor > 0 {
                        edit.cursor -= 1;
                        edit.text.remove(edit.cursor);
                    } else if key == "delete" && edit.cursor < edit.text.len() {
                        edit.text.remove(edit.cursor);
                    }
                }
                "left" => {
                    edit.cursor = edit.cursor.saturating_sub(1);
                    edit.selected = false;
                }
                "right" => {
                    edit.cursor = (edit.cursor + 1).min(edit.text.len());
                    edit.selected = false;
                }
                "home" => {
                    edit.cursor = 0;
                    edit.selected = false;
                }
                "end" => {
                    edit.cursor = edit.text.len();
                    edit.selected = false;
                }
                _ => {
                    if let Some(text) = &event.keystroke.key_char {
                        insert_digits(edit, text);
                    }
                }
            }
        }
        cx.notify();
        cx.stop_propagation();
    }
}
fn insert_digits(edit: &mut Editing, text: &str) {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return;
    }
    let remaining = if edit.selected { 0 } else { edit.text.len() };
    if remaining + text.len() > 3 {
        return;
    }
    if edit.selected {
        edit.text.clear();
        edit.cursor = 0;
        edit.selected = false;
    }
    edit.text.insert_str(edit.cursor, text);
    edit.cursor += text.len();
}
impl Render for Automation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editable = !self.enabled && self.pending.is_none();
        let mut content =
            div()
                .flex()
                .flex_col()
                .gap_3()
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key))
                .child("无线按电量切换 · 接线恢复有线配置")
                .child(div().text_xs().text_color(rgb(0x009d_aabd)).child(
                    "区间包含上下限。充电或放电进入区间时应用一次；未覆盖的电量保持现有配置。",
                ))
                .child(
                    super::performance::choice(
                        ("enable-ranges", 0),
                        if self.enabled {
                            "已启用 · 点击关闭"
                        } else {
                            "已关闭 · 点击启用"
                        },
                        self.enabled,
                        self.pending.is_some(),
                    )
                    .on_click(cx.listener(|view, _, _, cx| {
                        if view.pending.is_some() {
                            return;
                        }
                        if !view.enabled && !view.commit(cx) {
                            return;
                        }
                        if !view.enabled
                            && let Err(e) = validate(&view.rules)
                        {
                            view.message = e;
                            cx.notify();
                            return;
                        }
                        view.enabled = !view.enabled;
                        performance::set_automatic(true, view.enabled);
                        view.active = None;
                        view.message = if view.enabled {
                            "等待在线电量报告".into()
                        } else {
                            "已关闭，可以编辑区间".into()
                        };
                        cx.notify();
                    })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x005c_c8a1))
                        .child(self.message.clone()),
                );
        let mut wired_rates = div().flex().flex_wrap().gap_2();
        for (rate, label) in performance::RATE_LABELS.iter().enumerate() {
            wired_rates = wired_rates.child(
                super::performance::choice(
                    ("wired-target-rate", rate),
                    label,
                    self.wired_rate == rate,
                    !editable,
                )
                .on_click(cx.listener(move |view, _, _, cx| {
                    if editable {
                        view.wired_rate = rate;
                        view.wired_mode = performance::allowed_modes(rate)[0];
                        cx.notify();
                    }
                })),
            );
        }
        let mut wired_modes = div().flex().flex_wrap().gap_2();
        for &mode in performance::allowed_modes(self.wired_rate) {
            wired_modes = wired_modes.child(
                super::performance::choice(
                    ("wired-target-mode", usize::from(mode)),
                    performance::MODES[usize::from(mode)],
                    self.wired_mode == mode,
                    !editable,
                )
                .on_click(cx.listener(move |view, _, _, cx| {
                    if editable {
                        view.wired_mode = mode;
                        cx.notify();
                    }
                })),
            );
        }
        content =
            content
                .child(
                    div()
                        .p_3()
                        .rounded_lg()
                        .bg(rgb(0x001c_2430))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child("接上数据线时使用")
                        .child(div().text_xs().text_color(rgb(0x009d_aabd)).child(
                            "启用后接线时应用一次，与电量无关。有线期间仍可在性能页手动修改。",
                        ))
                        .child("回报率")
                        .child(wired_rates)
                        .child("传感器模式")
                        .child(wired_modes),
                )
                .child("无线电量区间");
        for (index, rule) in self.rules.iter().enumerate() {
            let mut bounds = div().flex().items_center().gap_2();
            for (upper, value) in [(false, rule.min), (true, rule.max)] {
                let edit = self
                    .editing
                    .as_ref()
                    .filter(|e| e.index == index && e.upper == upper);
                let text = edit.map_or_else(
                    || value.to_string(),
                    |e| {
                        if e.selected {
                            e.text.clone()
                        } else {
                            format!("{}│{}", &e.text[..e.cursor], &e.text[e.cursor..])
                        }
                    },
                );
                bounds = bounds.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .id(("range-input", index * 2 + usize::from(upper)))
                                .w(gpui::px(80.))
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(if edit.is_some() {
                                    0x005c_c8a1
                                } else {
                                    0x0030_4054
                                }))
                                .bg(rgb(if edit.is_some_and(|e| e.selected) {
                                    0x0030_526b
                                } else {
                                    0x0021_2d3d
                                }))
                                .when(editable, |this| this.cursor(gpui::CursorStyle::IBeam))
                                .child(if text.is_empty() { " ".into() } else { text })
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if editable {
                                        view.start_edit(index, upper, window, cx);
                                    }
                                })),
                        )
                        .child("%"),
                );
                if !upper {
                    bounds = bounds.child("至");
                }
            }
            let mut rates = div().flex().flex_wrap().gap_2();
            for (rate, label) in performance::RATE_LABELS.iter().enumerate() {
                rates = rates.child(
                    super::performance::choice(
                        ("rule-rate", index * 7 + rate),
                        label,
                        rule.rate == rate,
                        !editable,
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if editable {
                            view.rules[index].rate = rate;
                            view.rules[index].mode = performance::allowed_modes(rate)[0];
                            view.edited(cx);
                        }
                    })),
                );
            }
            let mut modes = div().flex().flex_wrap().gap_2();
            for &mode in performance::allowed_modes(rule.rate) {
                modes = modes.child(
                    super::performance::choice(
                        ("rule-mode", index * 6 + usize::from(mode)),
                        performance::MODES[usize::from(mode)],
                        rule.mode == mode,
                        !editable,
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if editable {
                            view.rules[index].mode = mode;
                            view.edited(cx);
                        }
                    })),
                );
            }
            content = content.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(0x001c_2430))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(format!("区间 {}", index + 1))
                            .child(
                                super::performance::choice(
                                    ("remove-rule", index),
                                    "删除",
                                    false,
                                    !editable,
                                )
                                .on_click(cx.listener(
                                    move |view, _, _, cx| {
                                        if editable {
                                            view.editing = None;
                                            view.rules.remove(index);
                                            view.edited(cx);
                                        }
                                    },
                                )),
                            ),
                    )
                    .child(bounds)
                    .child("回报率")
                    .child(rates)
                    .child("传感器模式")
                    .child(modes),
            );
        }
        content
            .child(
                super::performance::choice(("add-range", 0), "＋ 添加区间", false, !editable)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if !editable {
                            return;
                        }
                        if !view.commit(cx) {return;}
                        let Some(min) = (0..=100).find(|p| matching(&view.rules, *p).is_none())
                        else {
                            view.message = "0–100% 已全部覆盖".into();
                            cx.notify();
                            return;
                        };
                        let max = (min..=100)
                            .take_while(|p| matching(&view.rules, *p).is_none())
                            .last()
                            .unwrap_or(min);
                        view.rules.push(Rule {
                            min,
                            max,
                            rate: 3,
                            mode: 1,
                        });
                        view.edited(cx);
                    })),
            )
            .child(div().text_xs().text_color(rgb(0x009d_aabd)).child(
                "仅对 VT7 V2 无线连接生效。关闭规则后编辑，上下限直接输入，Enter 确认、Tab 切换；退出程序后恢复关闭。",
            ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cable_takes_priority_and_unplug_returns_to_wireless() {
        use mouse::{
            fleet::{Availability, DeviceState},
            model::DeviceIdentity,
        };
        let wireless = DeviceIdentity {
            id: "hid#pid_1460".into(),
            driver: "rapoo-vt7",
            name: "Mouse",
        };
        let wired = DeviceIdentity {
            id: "hid#pid_4660".into(),
            driver: "rapoo-vt7",
            name: "Mouse",
        };
        let mut fleet = Fleet::default();
        fleet.apply(DeviceState {
            identity: wireless.clone(),
            availability: Availability::Live,
            reading: None,
            error: None,
        });
        assert!(connection_target(&fleet).unwrap().1);
        fleet.reconcile(&[wireless.clone(), wired]);
        assert!(!connection_target(&fleet).unwrap().1);
        fleet.reconcile(&[wireless]);
        assert!(connection_target(&fleet).unwrap().1);
        fleet.reconcile(&[]);
        assert!(connection_target(&fleet).is_none());
    }
    #[test]
    fn numeric_entry_replaces_selection_and_rejects_non_digits() {
        let mut edit = Editing {
            index: 0,
            upper: false,
            text: "20".into(),
            cursor: 2,
            selected: true,
        };
        insert_digits(&mut edit, "75");
        assert_eq!(edit.text, "75");
        assert_eq!(edit.cursor, 2);
        insert_digits(&mut edit, "x");
        assert_eq!(edit.text, "75");
        edit.selected = true;
        insert_digits(&mut edit, "100");
        assert_eq!(edit.text, "100");
        insert_digits(&mut edit, "0");
        assert_eq!(edit.text, "100");
    }
    const fn rule(min: u8, max: u8) -> Rule {
        Rule {
            min,
            max,
            rate: 2,
            mode: 0,
        }
    }
    #[test]
    fn boundaries_gaps_and_both_directions() {
        let rules = vec![rule(0, 20), rule(21, 60), rule(80, 100)];
        assert!(validate(&rules).is_ok());
        for (p, expected) in [
            (0, Some(0)),
            (20, Some(0)),
            (21, Some(1)),
            (60, Some(1)),
            (61, None),
            (79, None),
            (80, Some(2)),
            (100, Some(2)),
        ] {
            assert_eq!(matching(&rules, p), expected);
        }
        assert_eq!(
            [80, 60, 20, 21, 80].map(|p| matching(&rules, p)),
            [Some(2), Some(1), Some(0), Some(1), Some(2)]
        );
    }
    #[test]
    fn rejects_overlap_reversed_empty_and_invalid_profiles() {
        assert!(validate(&[rule(0, 20), rule(20, 100)]).is_err());
        assert!(validate(&[rule(21, 20)]).is_err());
        assert!(validate(&[]).is_err());
        assert!(
            validate(&[Rule {
                min: 0,
                max: 100,
                rate: 0,
                mode: 5
            }])
            .is_err()
        );
    }
}
