use super::{
    icon,
    presentation::{detail_text, status_text},
};
use anyhow::Result;
use mouse::{
    diagnostics::{instance_lock, log},
    model::{Battery, Charging, DeviceIdentity, Reading, ReportKind, Status},
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
    time::{Duration, Instant, SystemTime},
};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use winsafe::{co, gui, prelude::*};

enum Event {
    Status(Status),
    Popup { x: i32, y: i32 },
}

struct App {
    window: gui::WindowMain,
    status_label: gui::Label,
    detail_label: gui::Label,
    tray: Option<TrayIcon>,
    status: Option<Status>,
    receiver: Receiver<Event>,
    sender: Sender<Event>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    deadline: Option<Instant>,
    demo: bool,
    demo_opened: bool,
    selector: Option<String>,
}

impl App {
    fn initialize(&mut self) -> Result<()> {
        // No native popup menu: regular modeless controls remain responsive
        // when the charging cable changes the mouse's USB/input devices.
        self.tray = Some(
            TrayIconBuilder::new()
                .with_tooltip(format!(
                    "{}\n{}",
                    status_text(&Status::Searching),
                    detail_text(&Status::Searching)
                ))
                .with_icon(icon::render(&Status::Searching))
                .build()?,
        );
        self.window.hwnd().SetTimer(1, 100, None)?;
        log("Tray created; modeless popup ready; automatic discovery enabled");
        let sender = self.sender.clone();
        let stop = Arc::clone(&self.stop);
        let selector = self.selector.clone();
        let demo = self.demo;
        self.worker = Some(thread::spawn(move || {
            if demo {
                for state in demo_states().into_iter().cycle() {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let _ = sender.send(Event::Status(state));
                    for _ in 0..40 {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            } else {
                monitor::watch(
                    HidTransport::default(),
                    selector.as_deref(),
                    &stop,
                    |state| {
                        let _ = sender.send(Event::Status(state));
                    },
                );
            }
        }));
        Ok(())
    }

    fn update(&mut self, status: Status) -> Result<()> {
        if self.status.as_ref() == Some(&status) {
            return Ok(());
        }
        let text = status_text(&status);
        let detail = detail_text(&status);
        if self.status.as_ref().is_some_and(|previous| {
            status_text(previous) == text && detail_text(previous) == detail
        }) {
            self.status = Some(status);
            return Ok(());
        }
        self.status_label.hwnd().SetWindowText(&text)?;
        self.detail_label.hwnd().SetWindowText(&detail)?;
        if let Some(tray) = &self.tray {
            tray.set_tooltip(Some(&format!("{text}\n{detail}")))?;
            if self
                .status
                .as_ref()
                .and_then(Status::reading)
                .map(|reading| reading.battery)
                != status.reading().map(|reading| reading.battery)
            {
                tray.set_icon(Some(icon::render(&status)))?;
            }
        }
        log(&format!("Popup/tray updated: {text}; {detail}"));
        self.status = Some(status);
        Ok(())
    }

    fn show_popup(&self, x: i32, y: i32) -> Result<()> {
        let rect = self.window.hwnd().GetWindowRect()?;
        self.window.hwnd().SetWindowPos(
            winsafe::HwndPlace::Place(co::HWND_PLACE::TOPMOST),
            winsafe::POINT {
                x: (x - (rect.right - rect.left)).max(0),
                y: (y - (rect.bottom - rect.top) - 8).max(0),
            },
            winsafe::SIZE::default(),
            co::SWP::NOSIZE,
        )?;
        self.window.hwnd().ShowWindow(co::SW::SHOW);
        self.window.hwnd().SetForegroundWindow();
        log(&format!(
            "Modeless popup shown; visible={}",
            self.window.hwnd().IsWindowVisible()
        ));
        Ok(())
    }

    fn tick(&mut self) -> Result<bool> {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Status(status) => self.update(status)?,
                Event::Popup { x, y } => self.show_popup(x, y)?,
            }
        }
        if self.demo && !self.demo_opened {
            self.demo_opened = true;
            let point = winsafe::GetCursorPos()?;
            self.show_popup(point.x, point.y)?;
        }
        Ok(self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline))
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.tray.take();
        log("Tray exited");
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
    let window = gui::WindowMain::new(gui::WindowMainOpts {
        title: "鼠标电量",
        size: gui::dpi(360, 160),
        style: co::WS::CAPTION | co::WS::SYSMENU | co::WS::BORDER | co::WS::CLIPCHILDREN,
        ex_style: co::WS_EX::TOPMOST,
        ..Default::default()
    });
    let status_label = gui::Label::new(
        &window,
        gui::LabelOpts {
            text: "设备：未识别\n电量：未知 · 充电状态：未知",
            position: gui::dpi(16, 16),
            size: gui::dpi(328, 48),
            ..Default::default()
        },
    );
    let detail_label = gui::Label::new(
        &window,
        gui::LabelOpts {
            text: "上次广播：尚未收到",
            position: gui::dpi(16, 68),
            size: gui::dpi(328, 34),
            ..Default::default()
        },
    );
    let close_button = gui::Button::new(
        &window,
        gui::ButtonOpts {
            text: "关闭面板",
            position: gui::dpi(160, 116),
            ..Default::default()
        },
    );
    let exit_button = gui::Button::new(
        &window,
        gui::ButtonOpts {
            text: "退出",
            position: gui::dpi(256, 116),
            ..Default::default()
        },
    );
    let app = Rc::new(RefCell::new(App {
        window: window.clone(),
        status_label,
        detail_label,
        tray: None,
        status: None,
        receiver,
        sender,
        stop: Arc::new(AtomicBool::new(false)),
        worker: None,
        deadline: duration.map(|time| Instant::now() + time),
        demo,
        demo_opened: false,
        selector,
    }));
    let app_create = Rc::clone(&app);
    window.on().wm_create(move |_| {
        app_create.borrow_mut().initialize()?;
        Ok(0)
    });
    let app_timer = Rc::clone(&app);
    let timer_window = window.clone();
    window.on().wm_timer(1, move || {
        let exit = app_timer.borrow_mut().tick()?;
        if exit {
            timer_window.hwnd().DestroyWindow()?;
        }
        Ok(())
    });
    let close_window = window.clone();
    close_button.on().bn_clicked(move || {
        close_window.hwnd().ShowWindow(co::SW::HIDE);
        log("Popup closed");
        Ok(())
    });
    let exit_window = window.clone();
    exit_button.on().bn_clicked(move || {
        log("Exit button clicked");
        exit_window.hwnd().DestroyWindow()?;
        Ok(())
    });
    let inactive_window = window.clone();
    window.on().wm_activate(move |params| {
        if params.event == co::WA::INACTIVE {
            inactive_window.hwnd().ShowWindow(co::SW::HIDE);
        }
        Ok(())
    });
    let hide_window = window.clone();
    window.on().wm_close(move || {
        hide_window.hwnd().ShowWindow(co::SW::HIDE);
        Ok(())
    });
    let app_destroy = Rc::clone(&app);
    window.on().wm_destroy(move || {
        app_destroy.borrow_mut().shutdown();
        Ok(())
    });
    window
        .run_main(Some(co::SW::HIDE))
        .map_err(|error| anyhow::anyhow!("Windows 界面错误：{error}"))?;
    Ok(())
}

fn demo_states() -> Vec<Status> {
    let identity = DeviceIdentity {
        id: "demo".into(),
        driver: "demo",
        name: "演示鼠标",
    };
    let mut states = vec![Status::Waiting(identity.clone())];
    for (percent, charging) in [
        (76, Charging::Discharging),
        (18, Charging::Discharging),
        (42, Charging::Active),
        (42, Charging::Discharging),
        (100, Charging::Full),
    ] {
        states.push(Status::Live(
            identity.clone(),
            Reading {
                battery: Battery {
                    percent,
                    charging,
                    raw_charging: 0,
                    dpi: None,
                },
                received_at: SystemTime::now(),
                kind: ReportKind::Broadcast,
            },
        ));
    }
    states.push(Status::Stale(identity, None));
    states.push(Status::Offline("演示：设备断开".into()));
    states
}
