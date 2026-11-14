use gpui::{
    Context, Entity, IntoElement, Render, Subscription, Timer, Window, div, prelude::*, px, rgb,
};
use mouse::fleet::{Availability, DeviceState, Fleet};
use std::{
    collections::HashMap,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

struct Parameters {
    settings: Option<mouse::performance::rapoo::Settings>,
    error: Option<String>,
    received: Option<Instant>,
}
type ParameterResult = (String, Result<mouse::performance::rapoo::Settings, String>);

pub struct Panel {
    fleet: Entity<Fleet>,
    performance: [Option<Entity<super::performance::Performance>>; 4],
    atk: bool,
    automation: Entity<super::automation::Automation>,
    page: usize,
    wireless: bool,
    parameters: HashMap<String, Parameters>,
    pending: Option<Receiver<Vec<ParameterResult>>>,
    next_refresh: Instant,
    revision: u64,
    _subscriptions: Vec<Subscription>,
}

impl Panel {
    pub fn new(
        fleet: Entity<Fleet>,
        automation: Entity<super::automation::Automation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let updates = cx.observe(&fleet, |_, _, cx| cx.notify());
        let activation = cx.observe_window_activation(window, |_, window, _| {
            if !window.is_window_active() {
                window.remove_window();
            }
        });
        let atk = fleet
            .read(cx)
            .connected()
            .any(|s| s.identity.driver == "atk-x1-pro");
        cx.spawn(async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(200)).await;
                if this.update(cx, Self::refresh_parameters).is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            fleet,
            performance: [None, None, None, None],
            atk,
            wireless: true,
            automation,
            page: 0,
            parameters: HashMap::new(),
            pending: None,
            next_refresh: Instant::now(),
            revision: mouse::performance::rapoo::revision(),
            _subscriptions: vec![updates, activation],
        }
    }
    fn performance_index(&self) -> usize {
        usize::from(self.atk) * 2 + usize::from(self.wireless)
    }
    fn ensure_performance(&mut self, cx: &mut Context<Self>) {
        let index = self.performance_index();
        if self.performance[index].is_none() {
            let device = if self.atk {
                mouse::performance::Device::AtkX1Pro
            } else {
                mouse::performance::Device::Vt7V2
            };
            self.performance[index] =
                Some(cx.new(|cx| super::performance::Performance::new(device, self.wireless, cx)));
        }
    }
    fn refresh_parameters(&mut self, cx: &mut Context<Self>) {
        if let Some(results) = self.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.pending = None;
            for (id, result) in results {
                let state = self.parameters.entry(id).or_insert(Parameters {
                    settings: None,
                    error: None,
                    received: None,
                });
                match result {
                    Ok(settings) => {
                        state.settings = Some(settings);
                        state.error = None;
                        state.received = Some(Instant::now());
                    }
                    Err(error) => state.error = Some(error),
                }
            }
            self.next_refresh = Instant::now() + Duration::from_secs(15);
            cx.notify();
        }
        let revision = mouse::performance::rapoo::revision();
        if self.page != 0
            || self.pending.is_some()
            || (Instant::now() < self.next_refresh && revision == self.revision)
        {
            return;
        }
        self.revision = revision;
        let fleet = self.fleet.read(cx);
        let ids: Vec<_> = fleet
            .connected()
            .filter(|s| s.availability == Availability::Live && supports_parameters(s))
            .map(|s| s.identity.id.clone())
            .collect();
        self.parameters
            .retain(|id, _| fleet.devices.contains_key(id));
        self.next_refresh = Instant::now() + Duration::from_secs(15);
        if ids.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            let results = ids
                .into_iter()
                .map(|id| {
                    let result =
                        mouse::performance::rapoo::load_device(&id).map_err(|e| format!("{e:#}"));
                    (id, result)
                })
                .collect();
            let _ = tx.send(results);
        });
        cx.notify();
    }
}
fn supports_parameters(state: &DeviceState) -> bool {
    let id = state.identity.id.to_ascii_lowercase();
    state.identity.driver == "rapoo-vt7"
        && ["pid_4660", "pid_1460", "pid_1413"]
            .iter()
            .any(|pid| id.contains(pid))
}

impl Render for Panel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fleet = self.fleet.read(cx);
        let rows = fleet
            .visible()
            .enumerate()
            .map(|(index, state)| {
                device_row(state, index + 1, self.parameters.get(&state.identity.id))
            })
            .collect::<Vec<_>>();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x0011_1822))
            .text_color(rgb(0x00e6_edf3))
            .font_family("Microsoft YaHei UI")
            .text_sm()
            .p_4()
            .gap_3()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_base()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("鼠标控制中心"),
                    )
                    .child(format!("{} 台", rows.len())),
            )
            .child(
                div().flex().gap_2().children(
                    ["设备", "性能", "自动切换"]
                        .into_iter()
                        .enumerate()
                        .map(|(index, label)| {
                            super::performance::choice(
                                ("page", index),
                                label,
                                self.page == index,
                                false,
                            )
                            .on_click(cx.listener(
                                move |view, _, _, cx| {
                                    view.page = index;
                                    if index == 0 {
                                        view.next_refresh = Instant::now();
                                    }
                                    if index == 1 {
                                        view.ensure_performance(cx);
                                    }
                                    cx.notify();
                                },
                            ))
                        }),
                ),
            )
            .when(self.page == 1, |this| {
                this.child(
                    div().flex().gap_2().children(
                        [(false, "VT7 V2"), (true, "ATK X1 PRO")].into_iter().map(
                            |(atk, label)| {
                                super::performance::choice(
                                    ("model", usize::from(atk)),
                                    label,
                                    self.atk == atk,
                                    false,
                                )
                                .on_click(cx.listener(
                                    move |view, _, _, cx| {
                                        view.atk = atk;
                                        view.ensure_performance(cx);
                                        cx.notify();
                                    },
                                ))
                            },
                        ),
                    ),
                )
            })
            .when(self.page == 1, |this| {
                this.child(
                    div().flex().gap_2().children(
                        [(false, "有线"), (true, "无线")]
                            .into_iter()
                            .map(|(wireless, label)| {
                                super::performance::choice(
                                    ("connection", usize::from(wireless)),
                                    label,
                                    self.wireless == wireless,
                                    false,
                                )
                                .on_click(cx.listener(
                                    move |view, _, _, cx| {
                                        view.wireless = wireless;
                                        view.ensure_performance(cx);
                                        cx.notify();
                                    },
                                ))
                            }),
                    ),
                )
            })
            .child(
                div()
                    .id("devices")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(rows.is_empty() && self.page == 0, |this| {
                        this.child(
                            div()
                                .py_6()
                                .text_color(rgb(0x008b_949e))
                                .child("正在查找支持的鼠标"),
                        )
                    })
                    .when(self.page == 0, |this| this.children(rows))
                    .when(self.page == 1, |this| {
                        this.when_some(
                            self.performance[self.performance_index()].clone(),
                            gpui::ParentElement::child,
                        )
                    })
                    .when(self.page == 2, |this| this.child(self.automation.clone()))
                    .when_some(fleet.discovery_error.clone(), |this, error| {
                        this.child(div().py_2().text_color(rgb(0x00f0_b66a)).child(error))
                    }),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_4()
                    .when(self.page == 0, |this| {
                        this.child(
                            div()
                                .id("refresh-devices")
                                .cursor_pointer()
                                .child(if self.pending.is_some() {
                                    "读取参数中…"
                                } else {
                                    "刷新参数"
                                })
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.next_refresh = Instant::now();
                                    view.refresh_parameters(cx);
                                })),
                        )
                    })
                    .child(
                        div()
                            .id("close")
                            .cursor_pointer()
                            .text_color(rgb(0x008b_949e))
                            .on_click(|_, window, _| window.remove_window())
                            .child("关闭"),
                    )
                    .child(
                        div()
                            .id("quit")
                            .cursor_pointer()
                            .text_color(rgb(0x008b_949e))
                            .on_click(|_, _, cx| cx.quit())
                            .child("退出"),
                    ),
            )
    }
}

fn device_row(
    state: &DeviceState,
    index: usize,
    parameters: Option<&Parameters>,
) -> impl IntoElement {
    let fresh = state.availability == Availability::Live;
    let color = if !fresh {
        0x008b_949e
    } else if state
        .reading
        .is_some_and(|reading| reading.battery.percent <= 20)
    {
        0x00ff_7b72
    } else {
        0x007e_e787
    };
    let percent = state.reading.map_or_else(
        || "--".into(),
        |reading| format!("{}%", reading.battery.percent),
    );
    let status = super::presentation::device_summary(state);
    let detail = super::presentation::detail_text(&state.status())
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    div()
        .p_3()
        .mb_3()
        .rounded_lg()
        .bg(rgb(0x001c_2430))
        .border_1()
        .border_color(rgb(0x0030_363d))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(format!("{} · {index}", state.identity.name))
                        .child(div().text_xs().text_color(rgb(0x008b_949e)).child(
                            if state.identity.id.to_ascii_lowercase().contains("pid_4660")
                                || state.identity.id.to_ascii_lowercase().contains("pid_4613")
                            {
                                "USB 有线"
                            } else {
                                "无线接收器"
                            },
                        )),
                )
                .child(div().text_2xl().text_color(rgb(color)).child(percent)),
        )
        .child(
            div()
                .mt_2()
                .text_xs()
                .text_color(rgb(0x008b_949e))
                .child(status),
        )
        .child(
            div()
                .mt_1()
                .text_xs()
                .text_color(rgb(0x008b_949e))
                .child(detail),
        )
        .child(
            div()
                .mt_2()
                .text_xs()
                .child(state.reading.and_then(|r| r.battery.dpi).map_or_else(
                    || "DPI：暂无读数".into(),
                    |(x, y)| format!("{}DPI：{x} × {y}", if fresh { "" } else { "上次 " }),
                )),
        )
        .child(parameter_details(state, parameters))
        .when_some(state.error.clone(), |this, error| {
            this.child(
                div()
                    .mt_1()
                    .text_xs()
                    .text_color(rgb(0x00f0_b66a))
                    .child(error),
            )
        })
        .child(
            div()
                .mt_2()
                .h(px(3.))
                .w_full()
                .rounded_sm()
                .bg(rgb(0x0030_363d))
                .child(
                    div().h_full().rounded_sm().bg(rgb(color)).w(gpui::relative(
                        state
                            .reading
                            .map_or(0., |reading| f32::from(reading.battery.percent) / 100.),
                    )),
                ),
        )
}

fn parameter_details(state: &DeviceState, parameters: Option<&Parameters>) -> impl IntoElement {
    let mut content = div().mt_2().flex().flex_col().gap_1().text_xs();
    if let Some(settings) = parameters.and_then(|p| p.settings.as_ref()) {
        let stale = state.availability != Availability::Live
            || parameters.is_some_and(|p| p.error.is_some());
        content = content
            .child(format!(
                "{}回报率：{} · 模式：{}",
                if stale { "上次读取 · " } else { "" },
                mouse::performance::RATE_LABELS[settings.rate],
                mouse::performance::rapoo::MODES[usize::from(settings.modes[settings.rate])]
            ))
            .child(format!(
                "射频：{} · 配置槽 {}",
                if settings.rf == 0 {
                    "智能调节"
                } else {
                    "满格射频"
                },
                settings.slot + 1
            ));
        if let Some(received) = parameters.and_then(|p| p.received) {
            content = content.child(format!("参数读取于 {} 秒前", received.elapsed().as_secs()));
        }
    } else {
        content = content.child(if supports_parameters(state) {
            "性能参数：等待设备读取"
        } else {
            "性能参数：此设备暂不支持读取"
        });
    }
    if let Some(error) = parameters.and_then(|p| p.error.as_ref()) {
        content = content.child(
            div()
                .text_color(rgb(0x00f0_b66a))
                .child(format!("参数读取失败：{error}")),
        );
    }
    content
}
