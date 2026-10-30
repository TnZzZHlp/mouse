use super::{icon, panel::Panel, presentation};
use anyhow::Result;
use gpui::{
    App, AppContext, Application, Context, Entity, IntoElement, Render, Timer, Window,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, div, px, size,
};
use mouse::{
    diagnostics::{instance_lock, log},
    fleet::Fleet,
    model::Status,
    monitor,
    transport::hid::HidTransport,
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

enum Event {
    Snapshot(Fleet),
    Popup { x: i32, y: i32 },
}

struct Monitor {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Monitor {
    fn start(sender: Sender<Event>, demo: bool, selector: Option<String>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            if demo {
                let mut step = 0;
                while !worker_stop.load(Ordering::Relaxed) {
                    if sender
                        .send(Event::Snapshot(super::demo::snapshot(step)))
                        .is_err()
                    {
                        return;
                    }
                    step += 1;
                    for _ in 0..40 {
                        if worker_stop.load(Ordering::Relaxed) {
                            return;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            } else {
                monitor::watch_all(
                    HidTransport::default,
                    selector.as_deref(),
                    &worker_stop,
                    |fleet| {
                        let _ = sender.send(Event::Snapshot(fleet));
                    },
                );
            }
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        log("Tray exited");
    }
}

struct Runtime {
    fleet: Entity<Fleet>,
    tray: TrayIcon,
    icon_status: Status,
    receiver: Receiver<Event>,
    popup: Option<WindowHandle<Panel>>,
    deadline: Option<Instant>,
}

impl Runtime {
    fn tick(&mut self, cx: &mut Context<Self>) -> Result<()> {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Snapshot(fleet) => {
                    let status = fleet
                        .lowest_live()
                        .map_or(Status::Searching, mouse::fleet::DeviceState::status);
                    let tooltip = presentation::fleet_tooltip(&fleet);
                    self.tray.set_tooltip(Some(&tooltip))?;
                    if status.reading().map(|reading| reading.battery)
                        != self.icon_status.reading().map(|reading| reading.battery)
                    {
                        self.tray.set_icon(Some(icon::render(&status)))?;
                        log(&presentation::status_text(&status));
                    }
                    self.icon_status = status;
                    self.fleet.update(cx, |state, cx| {
                        *state = fleet;
                        cx.notify();
                    });
                }
                Event::Popup { x, y } => self.show_popup(x, y, cx)?,
            }
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            cx.quit();
        }
        Ok(())
    }

    fn show_popup(&mut self, x: i32, y: i32, cx: &mut Context<Self>) -> Result<()> {
        if let Some(popup) = self.popup.take() {
            let _ = popup.update(cx, |_, window, _| window.remove_window());
        }
        let fleet = self.fleet.clone();
        let count = fleet.read(cx).devices.len();
        let height = f32::from(u16::try_from(count.clamp(1, 4))?).mul_add(134., 112.);
        let (bounds, display_id) =
            super::position::popup_bounds(x, y, size(px(380.), px(height)), cx)?;
        self.popup = Some(cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                kind: WindowKind::PopUp,
                is_resizable: false,
                is_minimizable: false,
                display_id,
                ..Default::default()
            },
            |window, cx| {
                window.activate_window();
                cx.new(|cx| Panel::new(fleet, window, cx))
            },
        )?);
        Ok(())
    }
}

impl Render for Runtime {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

pub fn run(duration: Option<Duration>, demo: bool, selector: Option<String>) -> Result<()> {
    let _lock = instance_lock()?;
    let (sender, receiver) = mpsc::channel();
    let tray_sender = sender.clone();
    TrayIconEvent::set_event_handler(Some(move |event| {
        if let TrayIconEvent::Click {
            position,
            button: MouseButton::Left | MouseButton::Right,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let position = position.cast::<i32>();
            let _ = tray_sender.send(Event::Popup {
                x: position.x,
                y: position.y,
            });
        }
    }));
    let _monitor = Monitor::start(sender.clone(), demo, selector);
    let error = Rc::new(RefCell::new(None));
    let launch_error = Rc::clone(&error);
    Application::new().run(move |cx| {
        if let Err(error) = launch(receiver, duration, Rc::clone(&launch_error), cx) {
            *launch_error.borrow_mut() = Some(error);
            cx.quit();
            return;
        }
        if demo {
            match winsafe::GetCursorPos() {
                Ok(position) => {
                    let _ = sender.send(Event::Popup {
                        x: position.x,
                        y: position.y,
                    });
                }
                Err(error) => log(&format!("Demo cursor position unavailable: {error}")),
            }
        }
    });
    TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
    error.borrow_mut().take().map_or(Ok(()), Err)
}

fn launch(
    receiver: Receiver<Event>,
    duration: Option<Duration>,
    runtime_error: Rc<RefCell<Option<anyhow::Error>>>,
    cx: &mut App,
) -> Result<()> {
    let tray = TrayIconBuilder::new()
        .with_tooltip("正在查找支持的鼠标")
        .with_icon(icon::render(&Status::Searching))
        .build()?;
    let fleet = cx.new(|_| Fleet::default());
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1.), px(1.)), cx)),
            show: false,
            focus: false,
            titlebar: None,
            kind: WindowKind::PopUp,
            ..Default::default()
        },
        |_, cx| {
            cx.new(|cx: &mut Context<Runtime>| {
                cx.spawn(async move |this, cx| {
                    loop {
                        Timer::after(Duration::from_millis(100)).await;
                        let result = this.update(cx, Runtime::tick);
                        match result {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                log(&format!("GPUI tray update failed: {error:#}"));
                                *runtime_error.borrow_mut() = Some(error);
                                let _ = cx.update(|cx| cx.quit());
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                })
                .detach();
                Runtime {
                    fleet,
                    tray,
                    icon_status: Status::Searching,
                    receiver,
                    popup: None,
                    deadline: duration.map(|time| Instant::now() + time),
                }
            })
        },
    )?;
    log("GPUI tray ready; concurrent device monitoring enabled");
    Ok(())
}
