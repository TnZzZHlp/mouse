use gpui::{Context, Entity, IntoElement, Render, Subscription, Window, div, prelude::*, px, rgb};
use mouse::fleet::{Availability, DeviceState, Fleet};

pub struct Panel {
    fleet: Entity<Fleet>,
    performance: [Option<Entity<super::performance::Performance>>; 4],
    atk: bool,
    automation: Entity<super::automation::Automation>,
    page: usize,
    wireless: bool,
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
        Self {
            fleet,
            performance: [None, None, None, None],
            atk,
            wireless: true,
            automation,
            page: 0,
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
}

impl Render for Panel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fleet = self.fleet.read(cx);
        let rows = fleet
            .connected()
            .enumerate()
            .map(|(index, state)| device_row(state, index + 1))
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
                    ["电量", "性能", "自动切换"]
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

fn device_row(state: &DeviceState, index: usize) -> impl IntoElement {
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
