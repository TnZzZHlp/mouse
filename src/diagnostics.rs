use anyhow::{Context, Result};
use std::{
    fs::File,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn log(message: &str) {
    if let Ok(mut file) = File::options()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("mouse-battery.log"))
    {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(file, "[{seconds}] {message}");
    }
}

pub fn instance_lock() -> Result<File> {
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(std::env::temp_dir().join("mouse-battery.lock"))
        .context("无法创建单实例锁")?;
    file.try_lock()
        .context("电量托盘已在运行，请查看任务栏右下角")?;
    Ok(file)
}
