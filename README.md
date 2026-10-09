# Mouse Battery

Windows 鼠标电量托盘程序。当前支持 **Rapoo VT7 有线 USB**（24AE:4613）
和 **2.4G 接收器**（24AE:1413），以及 **ATK X1 PRO USB**（373B:1016）
和已配对的 **ATK 1K / 8K 接收器**（373B:101A / 373B:101B，配对型号 2/38）。

## 使用

从 [GitHub Releases](https://github.com/TnZzZHlp/mouse/releases/latest) 下载 Windows x64 程序，
双击 `mouse.exe`（或下载的 `.exe`）启动。便携 ZIP 解压后即可运行，不需要安装 Rust。

从源码构建：

```powershell
git clone https://github.com/TnZzZHlp/mouse.git
cd mouse
cargo build --release --locked
Start-Process .\target\release\mouse.exe
```

也可以直接双击 `target\release\mouse.exe`。正式版没有控制台窗口。
任务栏右下角图标显示电量数字；充电时显示绿色数字和闪电，20% 以下显示红色。
悬停或点击、右键打开状态面板，显示设备名称、电量、充电状态和上次报告时间（本地时间）。
雷柏显示“上次广播”，ATK 显示“上次上报”；失败的查询不会改写时间。
点击“退出”结束程序；界面不显示 DPI 或驱动信息。
面板使用普通窗口和按钮，打开期间也能实时刷新，充电线拔插不会进入 Windows 原生菜单跟踪循环。
点击面板外或“关闭面板”收起面板，电量托盘继续运行。
Windows 可能把新图标放进隐藏图标面板，可以手动将它拖到任务栏上。

启动自动枚举并识别受支持的鼠标，不需要填写 VID/PID 或设备路径。
首次上报可能需要等待或移动鼠标，没有保证的固定广播周期。超过 30 秒没有新报告会重新扫描，
期间保留上次电量、充电状态和广播时间，托盘数字不变成问号。
读取失败或报告过期时，面板和悬停提示将读数标为“上次状态”，并提示“状态未更新，正在重新连接”；
此时充电状态是上次收到的记录，不能据此判断现在是否正在充电。收到新报告后恢复正常显示。
设备断开后每 3 秒重试，也保留最后读数；收到有效新报告才更新数据和时间，电量不变也会更新时间。
首次尚未收到广播显示“未知”和“尚未收到”。缓存按设备实例隔离，仅保留在本次程序运行中。
重复启动不会产生第二个图标。

雷柏网页驱动可能占用状态通道。无法收到电量时，请关闭对应浏览器标签页并移动鼠标。
雷柏仅被动读取广播；ATK 只发送官方电量 GET 和接收器型号 GET，连接后立即读取电量，随后每 15 秒读取。
两种模式都不修改鼠标配置。
ATK 接充电器时仍通过无线接收器读取状态；接收器无响应时只能保留上次记录。

## 诊断

开发版可以在 PowerShell 中查看输出：

```powershell
cargo run -- --list
cargo run -- --probe --device rapoo-vt7
cargo run -- --probe --device atk-x1-pro
cargo run -- --demo
cargo run -- --smoke-test
```

`--device` 接受驱动 ID 或 `--list` 输出的完整 HID 路径，仅用于可选的手动指定。
默认自动选择能打开的受支持设备，VT7 有线与接收器同时存在时先尝试有线；
无广播时重新扫描并尝试下一个候选设备，仍显示最后实际收到的读数及原广播时间；
当前一个程序实例显示一只鼠标；同时显示多只鼠标尚未实现。
`--demo` 使用模拟数据，不读取设备；`--smoke-test` 使用真实设备，30 秒后自动退出。
诊断日志在 `%TEMP%\mouse-battery.log`。正式版 GUI 程序的命令行输出可能不可见，优先使用开发版或查看日志。

VT7 的电量和充电状态分别读取原生报告 `[8]`、`[7]`。
充电码 `0` 兼容协议文档的实测未充电样本，`1` 对应官方 UI 的正常状态，`2` 对应充电状态；
其他值显示“未知充电状态”，不会假定非零都在充电。VT7 没有确认的独立“充满”状态码，100% 不改变充电状态。
有线 USB 已实机验证，抓包和连接差异见 [USB 协议](docs/Rapoo%20VT7/USB.md)。
蓝牙连接尚未验证。

ATK X1 PRO 的原生 Report 8 响应中 `[6]` 是电量，`[7]` 是充电状态：
`0` 未充电，`1` 充电，其他码显示未知。程序验证命令状态和校验和后才接受读数。
当前支持本机已验证的 `373B:1016`、`373B:101A` 与 `373B:101B` 的 `FF02:0002` 集合；
接收器建立连接时须通过 X1 PRO 配对型号核验。协议和实机抓包见
[ATK X1 PRO 协议](docs/ATK%20X1%20PRO/PROTOCOL.md)。
无线抓包与型号识别见 [1K / 8K 接收器协议](docs/ATK%20X1%20PRO/WIRELESS.md)。

## 开发与扩展

目录、接口和新增鼠标步骤见 [架构说明](docs/ARCHITECTURE.md)。

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

项目源码禁止 `unsafe`，原生 HID 与托盘细节由依赖封装。
构建需要 Windows、Rust 2024 工具链（Rust 1.89+）以及 MSVC 链接工具。
更新编译前先从状态面板退出程序，避免 Windows 锁住正在运行的 `mouse.exe`。
