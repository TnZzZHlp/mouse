# 目录和扩展接口

## 当前目录

```text
src/
  lib.rs                     可复用核心入口
  model.rs                   电量、充电状态、广播读数和时间、设备身份、监控事件
  devices/
    mod.rs                   MouseDriver 接口和驱动注册表
    rapoo/
      mod.rs
      vt7.rs                 VT7 集合匹配和纯报告解码
    atk/
      mod.rs
      x1_pro.rs              ATK X1 PRO 集合匹配和纯响应解码
    protocols/
      compx.rs               共享 Compx 响应校验与配对型号解码
  transport/
    mod.rs                   BatteryTransport / BatteryConnection 接口
    hid.rs                   Windows HID 枚举、设备选择、连接后端分派、被动读取
    compx.rs                 固定电量 GET、响应等待和轮询节奏
  monitor.rs                 连接、重试、按设备缓存读数、报告时效、状态去重
  diagnostics.rs             日志和进程单实例锁
  ui/
    mod.rs
    presentation.rs          通用状态文字
    icon.rs                  通用电量图标绘制
    tray.rs                  Windows 托盘、非模态状态面板、事件循环
  main.rs                    参数解析和组件组装
docs/
  ARCHITECTURE.md
  Rapoo VT7/PROTOCOL.md       已有协议证据
  Rapoo VT7/USB.md            有线 USB 抓包、描述符及模式差异
  ATK X1 PRO/PROTOCOL.md      主动电量 GET、字段及真实响应证据
  ATK X1 PRO/WIRELESS.md      1K / 8K 接收器、配对型号校验及无线报告
examples/
  capture_vt7.rs              同时被动监听 VT7 有线与接收器状态集合
  inspect_hid.rs              指定 VID/PID 的描述符检查及可选被动监听
  query_atk.rs                固定 ATK 电量 GET 的独立复现工具
```

依赖方向是 `main → UI / monitor → transport → devices → model`；`model` 不依赖 Windows 或 HID。
`monitor` 只调用传输接口，不识别厂商、VID/PID、报告 ID 或字节偏移。
UI 只接收 `Status`，显示设备名称、电量、充电状态和上次合法报告的本地时间，不引用具体型号驱动。
设备名称来自 `DeviceIdentity.name`；缓存读数继续显示其所属设备名称，不冒用正在探测的其他设备。
设备协议代码保留在各厂商目录，协议文档按型号保留原有目录。

Windows 托盘不挂载原生 `TrackPopupMenu` 菜单。点击托盘后显示程序自己的非模态状态面板，
面板和托盘共用普通 Windows 消息循环，因此充电线拔插导致的输入设备变化不涉及原生菜单捕获。
状态通过通道从后台发送到 UI，窗口定时消息每 100 ms 处理新事件；
面板失焦或点击“关闭面板”后隐藏，点击“退出”后停止监控线程并销毁托盘。
Win32 窗口、Label、Button 和消息循环使用 WinSafe 的安全 GUI 封装，业务源码仍禁止 `unsafe`。

## 已落地的接口

| 接口 | 职责 | 扩展约定 |
|---|---|---|
| `MouseDriver` | `id`、`name`、`matches`、`connection_priority`、`read_method`、报告长度、`decode` | 唯一稳定驱动 ID；精确匹配电量集合；默认被动读；主动型号指定独立连接后端，驱动本身没有设备句柄 |
| `HidDescriptor` | VID/PID、usage page、usage | 纯值类型，驱动匹配可以脱离设备做测试 |
| `Battery` | 百分比、通用充电状态、原始充电码、可选 DPI | UI 不读取原始码；DPI 不是所有鼠标都必需的字段 |
| `Reading` | `Battery`、`received_at: SystemTime`、`ReportKind` | 实际收到合法报告才记录时间；Broadcast / QueryResponse 区分广播和查询响应；UI 不显示 DPI |
| `DeviceIdentity` | 实例路径、驱动 ID、显示型号 | 相同型号多个实例用实例路径区分 |
| `BatteryTransport` | `connect(selector)` | 枚举、选择、打开连接由传输层负责 |
| `BatteryConnection` | `identity()`、`read()`、`report_kind()` | 有限等待；无数据返回 `None`；连接失效返回错误，触发监控层重连；协议时序由连接负责 |
| `Status` | Searching / Waiting / Live / Stale / Offline | Live / Stale 携带 Reading；界面以实际广播时间说明读数时间 |
| `Session` | 报告时效与恢复 | 每次合法报告刷新时间，即使电量数值不变；无报告不改写读数或接收时间 |

内置驱动在 `devices::DRIVERS` 注册。HID 连接保存对应驱动并只调用其解码函数，
驱动没有设备句柄，无法调用 HID 写入或升级命令。
传输层按 `ReadMethod` 选择被动 HID 连接或独立 Compx 连接；
被动连接只读取 Input，Compx 连接只发送固定电量 GET 和配对型号 GET，不提供任意命令入口。
`ReadMethod::CompxBatteryQuery` 携带可选的预期配对型号；通用接收器必须通过该核验后才接受连接。
Compx 每 15 秒查询、响应超时 3 秒、单次输入等待 1 秒；失败交给监控层处理，缓存照常保留。

## 添加第二款被动 HID 鼠标

1. 将协议证据放到 `docs/<型号>/PROTOCOL.md`，明确连接方式、集合、报告布局和充电枚举。
2. 新增 `src/devices/<厂商>/<型号>.rs` 并实现 `MouseDriver`。不得只按 VID 匹配整个复合设备。
3. 在厂商 `mod.rs` 导出模块，在 `devices::DRIVERS` 增加驱动实例。
4. 用真实捕获样本验证解码、错误报告、0/100 边界、充电状态和未知值；检查集合匹配不会进入升级通道。
5. 运行 `--list`、`--probe --device <驱动ID>`，再启动托盘验证电量和充电变化。

不需要修改 `monitor`、图标或菜单代码。默认自动枚举，选择首个可打开的支持设备；
断开或 30 秒没有合法报告时自动重新扫描，轮换尝试候选集合，避免一个静默接收器阻塞其他鼠标；
缓存按 `DeviceIdentity.id` 隔离，静默、重新扫描、断线和重连期间保留之前的 `Reading`；
未收到广播的新设备不会继承另一只设备的读数；扫描期间仍显示上一只实际报告的设备，
身份和接收时间保持属于该设备。收到新设备广播后才切换展示。缓存仅在本次运行中保留。
VT7 同时匹配有线 `4613` 和接收器 `1413`；两者共用解码，有线优先级较高。
ATK X1 PRO 精确匹配 `373B:1016`、`101A` 或 `101B` 的 `FF02:0002`，通过 `ReadMethod::CompxBatteryQuery` 选择主动后端。
`101A` / `101B` 还必须实际返回配对 CID/MID `2/38`；有线优先级较高。
其他同 VID 型号和接收器 PID 未验证，不扩宽匹配范围。
`--device` 可选择驱动 ID 或具体实例路径。若多个驱动将来共享相同集合，必须先完善型号识别，
不能依赖注册顺序猜测型号。

## 后续能力边界

当前是单实例、单活动设备、Windows HID 被动广播与 Compx 主动电量 GET。以下为后续扩展方向：

- **其他主动查询协议**：新增独立连接后端，将请求、响应和超时留在后端；
  在枚举/组合层分派，不给现有被动驱动增加随意写入能力。
- **蓝牙或其他平台**：在 `transport` 下新增后端；更换 UI 平台模块，保留 `model` 和监控逻辑。
- **多只鼠标同时显示**：为每个 `DeviceIdentity.id` 创建独立监控会话和托盘实例，并增加设备集合事件；
  不把多个设备读数混入一个 `Status::Live`。
- **配置和自动启动**：新增配置模块，由组合层处理路径和选项；不要把注册表或配置 IO 放到设备驱动中。

独立库/多 crate 仅在增加第二种传输或第二个平台后有实际需要时再拆分，当前保持单 package 的清晰模块边界。
