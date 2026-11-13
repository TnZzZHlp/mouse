use gpui::{Context, IntoElement, Render, Timer, Window, div, prelude::*, rgb};
use mouse::performance::{self, Change, Device, Settings};
use std::{
    sync::mpsc::{self, Receiver},
    time::Duration,
};

pub fn choice(
    id: (&'static str, usize),
    label: &str,
    selected: bool,
    disabled: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(if selected { 0x005c_c8a1 } else { 0x0030_4054 }))
        .bg(rgb(if selected { 0x0021_493e } else { 0x0021_2d3d }))
        .text_color(rgb(if disabled {
            0x0079_899b
        } else if selected {
            0x00bc_f4de
        } else {
            0x00d3_dce7
        }))
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(|style| style.bg(rgb(0x0030_4054)))
        })
        .child(label.to_owned())
}

pub struct Performance {
    device: Device,
    wireless: bool,
    locked: bool,
    settings: Option<Settings>,
    result: Option<Receiver<Result<Settings, String>>>,
    message: String,
}
impl Performance {
    pub fn new(device: Device, wireless: bool, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(100)).await;
                if this
                    .update(cx, |view, cx| {
                        let locked = view.device.automatic(view.wireless);
                        if view.locked != locked {
                            view.locked = locked;
                            cx.notify();
                        }
                        if let Some(result) = view.result.as_ref().and_then(|r| r.try_recv().ok()) {
                            view.result = None;
                            match result {
                                Ok(settings) => {
                                    view.settings = Some(settings);
                                    view.message = "已从设备读取并核对".into();
                                }
                                Err(error) => {
                                    view.settings = None;
                                    view.message = error;
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
        let mut view = Self {
            wireless,
            device,
            locked: device.automatic(wireless),
            settings: None,
            result: None,
            message: String::new(),
        };
        view.request(None, cx);
        view
    }
    fn request(&mut self, change: Option<Change>, cx: &mut Context<Self>) {
        if self.result.is_some() || (change.is_some() && self.device.automatic(self.wireless)) {
            return;
        }
        let slot = self.settings.as_ref().map(|s| s.slot);
        let (tx, rx) = mpsc::channel();
        self.result = Some(rx);
        self.message = if change.is_some() {
            "正在保存并核对…"
        } else {
            "正在读取设备…"
        }
        .into();
        let wireless = self.wireless;
        let device = self.device;
        std::thread::spawn(move || {
            let result = match (slot, change) {
                (Some(slot), Some(change)) => device.apply(wireless, slot, change),
                _ => device.load(wireless),
            };
            let _ = tx.send(result.map_err(|e| format!("{e:#}")));
        });
        cx.notify();
    }
}
impl Render for Performance {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .rounded_lg()
            .bg(rgb(0x001c_2430))
            .child(format!(
                "{} · {}性能",
                self.device.name(),
                if self.wireless { "无线" } else { "有线" }
            ))
            .when(self.device.automatic(self.wireless), |this| {
                this.child("自动切换已启用，手动修改已锁定。请先关闭此连接的自动切换。")
            })
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x009d_aabd))
                    .child("支持有线与无线接收器，选中后自动保存。"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x005c_c8a1))
                    .child(self.message.clone()),
            );
        if let Some(settings) = &self.settings {
            content = content
                .child(format!("配置槽 {}", settings.slot + 1))
                .child(format!(
                    "鼠标保存的回报率：{}",
                    performance::RATE_LABELS[settings.rate]
                ))
                .when(settings.rate > settings.max_rate, |this| {
                    this.child("保存值高于当前连接上限，实际回报率受接收器限制。")
                })
                .child("回报率");
            let mut rates = div().flex().flex_wrap().gap_2();
            for (index, label) in performance::RATE_LABELS
                .iter()
                .enumerate()
                .take(settings.max_rate + 1)
            {
                rates = rates.child(
                    choice(
                        ("rate", index),
                        label,
                        index == settings.rate,
                        self.result.is_some() || self.device.automatic(self.wireless),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.request(Some(Change::Rate(index)), cx);
                    })),
                );
            }
            content = content.child(rates).child("传感器模式");
            let rate = settings.rate;
            let mut modes = div().flex().flex_wrap().gap_2();
            for &mode in self.device.allowed_modes(rate) {
                modes = modes.child(
                    choice(
                        ("mode", usize::from(mode)),
                        self.device.mode_labels()[usize::from(mode)],
                        settings.mode == mode,
                        self.result.is_some() || self.device.automatic(self.wireless),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.request(Some(Change::Mode(rate, mode)), cx);
                    })),
                );
            }
            content = content.child(modes);
            if let Some(radio) = settings.radio {
                content = content.child("无线射频策略（保存到鼠标配置）");
                for (value, label) in [(0, "智能调节"), (1, "满格射频")] {
                    content = content.child(
                        choice(
                            ("radio", usize::from(value)),
                            label,
                            radio == value,
                            self.result.is_some() || self.device.automatic(self.wireless),
                        )
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.request(Some(Change::Radio(value)), cx);
                        })),
                    );
                }
            }
        }
        content.child(
            div()
                .id("refresh-performance")
                .cursor_pointer()
                .p_1()
                .child("刷新配置")
                .on_click(cx.listener(|view, _, _, cx| view.request(None, cx))),
        )
    }
}
