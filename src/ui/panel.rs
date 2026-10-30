use gpui::{Context, Entity, IntoElement, Render, Subscription, Window, div, prelude::*, px, rgb};
use mouse::fleet::{Availability, DeviceState, Fleet};

pub struct Panel {
    fleet: Entity<Fleet>,
    _subscriptions: Vec<Subscription>,
}

impl Panel {
    pub fn new(fleet: Entity<Fleet>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let updates = cx.observe(&fleet, |_, _, cx| cx.notify());
        let activation = cx.observe_window_activation(window, |_, window, _| {
            if !window.is_window_active() {
                window.remove_window();
            }
        });
        Self {
            fleet,
            _subscriptions: vec![updates, activation],
        }
    }
}

impl Render for Panel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fleet = self.fleet.read(cx);
        let rows = fleet
            .devices
            .values()
            .enumerate()
            .map(|(index, state)| device_row(state, index + 1))
            .collect::<Vec<_>>();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x0016_1b22))
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
                            .child("鼠标电量"),
                    )
                    .child(format!("{} 台", fleet.devices.len())),
            )
            .child(
                div()
                    .id("devices")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows)
                    .when(fleet.devices.is_empty(), |this| {
                        this.child(
                            div()
                                .py_6()
                                .text_color(rgb(0x008b_949e))
                                .child("正在查找支持的鼠标"),
                        )
                    })
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
        .py_3()
        .border_b_1()
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
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x008b_949e))
                                .child(super::presentation::connection_label(state)),
                        ),
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
