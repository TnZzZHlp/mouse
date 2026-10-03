#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ui;
use anyhow::Result;
use mouse::{
    diagnostics::log,
    monitor,
    transport::hid::{self, HidTransport},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        log(&format!("ERROR: {error:#}"));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut mode = String::new();
    let mut selector = None;
    while let Some(arg) = args.next() {
        if arg == "--device" {
            selector =
                Some(args.next().ok_or_else(|| {
                    anyhow::anyhow!("--device 需要驱动 ID 或 --list 中的设备路径")
                })?);
        } else if mode.is_empty() {
            mode = arg;
        } else {
            anyhow::bail!("未知参数：{arg}");
        }
    }
    match mode.as_str() {
        "" => ui::tray::run(None, false, selector),
        "--probe" => monitor::probe(HidTransport::default(), selector.as_deref()),
        "--demo" => ui::tray::run(None, true, selector),
        "--smoke-test" => ui::tray::run(Some(std::time::Duration::from_secs(30)), false, selector),
        "--list" => {
            for device in hid::list()? {
                println!("{} [{}]\n{}", device.name, device.driver, device.id);
            }
            Ok(())
        }
        "--help" | "-h" => {
            println!(
                "鼠标电量托盘\n双击启动，右键托盘退出。\n--probe       等待最多 30 秒并输出真实电量\n--list        列出支持且已连接的设备\n--device ID   指定驱动 ID（如 rapoo-vt7）或设备路径\n--demo        演示正常、低电量、充电和离线状态\n--smoke-test  运行真实托盘 30 秒后自动退出"
            );
            Ok(())
        }
        _ => anyhow::bail!("未知参数；使用 --help 查看帮助"),
    }
}
