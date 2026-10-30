use anyhow::{Context as _, Result};
use gpui::{App, Bounds, DisplayId, Pixels, Size, point, px, size};
use winsafe::{HDC, HMONITOR, POINT, co, prelude::Handle};

#[expect(clippy::cast_precision_loss)]
pub fn popup_bounds(
    x: i32,
    y: i32,
    requested: Size<Pixels>,
    cx: &App,
) -> Result<(Bounds<Pixels>, Option<DisplayId>)> {
    let monitor = HMONITOR::MonitorFromPoint(POINT { x, y }, co::MONITOR::DEFAULTTONEAREST);
    let info = monitor.GetMonitorInfo()?;
    let mut index = 0_u32;
    let mut monitor_index = None;
    HDC::NULL.EnumDisplayMonitors(None, |candidate, _, _| {
        if candidate == monitor {
            monitor_index = Some(index);
        }
        index += 1;
        true
    })?;
    let display = cx
        .displays()
        .into_iter()
        .find(|display| Some(u32::from(display.id())) == monitor_index)
        .context("无法定位面板所在显示器")?;
    let scale = (info.rcMonitor.right - info.rcMonitor.left) as f32
        / f32::from(display.bounds().size.width);
    let left = info.rcWork.left as f32 / scale;
    let top = info.rcWork.top as f32 / scale;
    let right = info.rcWork.right as f32 / scale;
    let bottom = info.rcWork.bottom as f32 / scale;
    let width = f32::from(requested.width).min((right - left - 16.).max(1.));
    let height = f32::from(requested.height).min((bottom - top - 16.).max(1.));
    let origin = point(
        px((x as f32 / scale - width).clamp(left + 8., (right - width - 8.).max(left + 8.))),
        px((y as f32 / scale - height - 8.).clamp(top + 8., (bottom - height - 8.).max(top + 8.))),
    );
    Ok((
        Bounds::new(origin, size(px(width), px(height))),
        Some(display.id()),
    ))
}
