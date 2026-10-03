# 雷柏 VT7 鼠标电量协议（逆向结果）

设备：**Rapoo VT7**（VT 二代，Nordic nRF54L15 主控，PAW3398 传感器，800 mAh 电池）
当前连接：2.4G 无线接收器 `USB\VID_24AE&PID_1413`（`REV_0105`，`Rapoo Gaming Device`）

2026-10-03 补充：有线 USB `24AE:4613` 已实机验证，复用相同状态报告；
完整抓包、描述符与模式差异见 [有线 USB 协议](USB.md)。

---

## 1. 一句话结论

**电量不需要发任何查询命令。** 接收器被动广播 **Report ID 7** 的厂商自定义输入报告，
电量就是其中 **偏移 `[8]` 的字节**（0–100 十进制）。广播时机不保证固定，静默时应保留最后读数。

这也解释了为什么 Windows 看不到电量：设备**没有** HID Battery Strength 用途页，也**没有**任何
feature report 电量查询命令，Windows 无法识别出"电池设备"，所以系统 UI 里永远不显示。

---

## 2. 关键区别：报告 ID 是否被算作数据的第 0 字节

这是最容易踩的坑：

| 读取方式 | 缓冲区第 0 字节 | 电量位置 | 充电状态位置 |
|---|---|---|---|
| **Windows 原生 HID**（`ReadFile` / `HidD_GetInputReport`） | **是 report ID = `0x07`** | `buf[8]` | `buf[7]` |
| WebHID（浏览器 `event.data`） | 否（ID 单独在 `event.reportId`） | `data[7]` | `data[6]` |

雷柏官方 Web 驱动用的是 WebHID，所以官方代码里写的是 `o[7]`；
在 PowerShell / C / C# 里用原生 HID API 时必须 **+1**。

---

## 3. 完整数据包格式（19 字节，含 report ID）

实测样本（本机真实抓包）：

```
07 10 03 4C 04 4C 04 00 64 00 03 01 00 13 46 01 02 02 B6
^  ^  ^  ^^^^^ ^^^^^ ^^ ^^ ^^ ^^ ^^ ^^ ^^^^^ ^^ ^^ ^^ ^^
|  |  |    |     |    |  |  |  |  |  |    |   |  |  |  +-- [18] rfConnected / rfRssi 位域
|  |  |    |     |    |  |  |  |  |  |    |   |  |  +----- [17] sensorMode = 2
|  |  |    |     |    |  |  |  |  |  |    |   |  +-------- [16] rfType = 2
|  |  |    |     |    |  |  |  |  |  |    |   +----------- [15] keyType=1, wheelType=0
|  |  |    |     |    |  |  |  |  |  |    +--------------- [13..14] usbPid = 0x4613 (VT7 鼠标侧 PID)
|  |  |    |     |    |  |  |  |  |  +-------------------- [12] cfgSel = 0 (配置档)
|  |  |    |     |    |  |  |  |  +----------------------- [11] rpt = 1 (回报率码)
|  |  |    |     |    |  |  |  +-------------------------- [10] kRate = 3
|  |  |    |     |    |  |  +----------------------------- [9]  blMode = 0
|  |  |    |     |    |  +-------------------------------- [8]  batPercent = 0x64 = 100%  ★电量
|  |  |    |     |    +----------------------------------- [7]  batStatus  = 0 (未充电)
|  |  |    |     +---------------------------------------- [6]  yCpiVal LE16 = 0x044C = 1100
|  |  |    +---------------------------------------------- [5]  yCpiVal 高字节
|  |  |                                                   [3..4] xCpiVal LE16 = 0x044C = 1100
|  |  +--------------------------------------------------- [2]  cpiLev = 3 (第 4 个 DPI 档)
|  +------------------------------------------------------ [1]  conProtocol = 0, sensor = 1 (位域)
+--------------------------------------------------------- [0]  report ID = 0x07
```

### 字段表

| 偏移（含 report ID） | 长度 | 字段 | 说明 |
|---|---|---|---|
| `[0]` | 1 | report ID | 固定 `0x07` |
| `[1]` | 1 | 位域 | 低 4 位 `conProtocol`，高 4 位 `sensor` |
| `[2]` | 1 | `cpiLev` | 当前 DPI 档位索引 |
| `[3..4]` | 2 | `xCpiVal` | X 轴 DPI，**小端 16 位** |
| `[5..6]` | 2 | `yCpiVal` | Y 轴 DPI，**小端 16 位** |
| `[7]` | 1 | `batStatus` | 充电状态，`0` = 未充电 |
| **`[8]`** | 1 | **`batPercent`** | **电量，0–100** |
| `[9]` | 1 | `blMode` | 蓝牙模式 |
| `[10]` | 1 | `kRate` | 按键扫描率 |
| `[11]` | 1 | `rpt` | 回报率码 |
| `[12]` | 1 | `cfgSel` | 当前配置档 |
| `[13..14]` | 2 | `usbPid` | 配对鼠标的 USB PID，**小端 16 位**（`0x4613` = VT7） |
| `[15]` | 1 | 位域 | 低 4 位 `keyType`，高 4 位 `wheelType` |
| `[16]` | 1 | `rfType` | 射频类型 |
| `[17]` | 1 | `sensorMode` | 传感器模式 |
| `[18]` | 1 | `rfConnected / rfRssi` | 高位表示无线连接，低 7 位为信号值；不作为电量校验条件 |

> 2026-10-03 更正：重新核对官方 `Nf` 解码器，原文“只解析到 `[17]`”有误。
> 它还把 payload `[17]`（原生 `[18]`）解析为 `rfConnected = byte >> 7 & 1`、
> `rfRssi = byte & 127`。电量程序不需要这两个字段，不用该字节做校验。
>
> 另外，`[15]` 与 `[16]` 对应官方字段 `keyType/wheelType` 与 `rfType`；官方代码里它们来自
> payload 的 `o[14]`/`o[15]`/`o[16]`，换算成含 report ID 的原生缓冲区即为 `[15]`/`[16]`/`[17]`。

---

## 4. 要打开哪个 HID 集合

设备是 USB 复合设备，含 3 个接口共 12 个 HID 集合。电量所在的集合：

```
usagePage = 0xFF00 (65280)
usage     = 0x0002
集合实例   = HID\VID_24AE&PID_1413&MI_01&COL09\...
接口路径   = \\?\hid#vid_24ae&pid_1413&mi_01&col09#...#{4d1e55b2-...}
输入报告长度 = 19 字节
```

官方驱动里它被描述为 `"Configuration Interface (Secondary)"`，是主配置/状态通道。

### 全部集合一览（含危险区警告）

| 集合 | usage | 长度 | 作用 | 备注 |
|---|---|---|---|---|
| MI_01 COL03 | 0x000C/0x0001 | 3 | 消费者控制 | 系统占用 |
| MI_01 COL04 | 0x0001/0x0080 | 2 | 系统控制 | 系统占用 |
| MI_01 COL05 | 0xFF00/0x000E | 33 | **Nordic 固件升级** | ⚠️ 别写 |
| MI_01 COL06 | 0xFF00/0x000F | 33 feature | OTA 数据（feature id 8） | ⚠️ 别写 |
| MI_01 COL07 | 0xFF00/0x0010 | 33 feature | OTA ACK（feature id 9） | ⚠️ 别写 |
| MI_01 COL08 | 0xFF00/0x0011 | 32 | 固件升级相关 | ⚠️ 别写 |
| **MI_01 COL09** | **0xFF00/0x0002** | **19** | **配置/状态通道** | ✅ **电量在这里** |
| MI_01 COL0A | 0xFF00/0x0012 | 24 | 厂商自定义 | |
| MI_02 | 0xFF00/0x0001 | 513 | **WCH 固件升级 bulk** | ⚠️⚠️ 千万别写，有刷砖风险 |
| MI_00 | 0x0001/0x0002 | — | 鼠标数据 | 被系统独占，无法打开 |

`MI_02` 的 513 字节 = 512 字节负载 + 1 字节 report ID，是 WCH OTA 的大块传输通道，
**与电量无关**。本次逆向过程中曾向其发送过 3 个短命令（无响应），未触发任何升级流程，无副作用。

---

## 5. 为什么可以确信这个结论

三重独立印证：

1. **DPI 自校验**：包内 `[3..4]` 与 `[5..6]` 解出 `0x044C` = **1100**，与用户实际配置的 DPI
   完全一致（用户独立确认"1100 就是 dpi"）。这说明字节偏移的切分是正确的。

2. **包内自证型号**：`[13..14]` = `0x4613`。雷柏官方网页驱动的 manifest 中，
   VT7 条目写的是 `productId:[17939, 5139]` = `[0x4613, 0x1413]` ——
   `0x1413` 正是本机接收器 PID，`0x4613` 是配对鼠标侧的 PID。**双向指向同一条 VT7 记录。**

3. **官方实现对照**：雷柏自家 Web 驱动（`https://hid.rapoo.cn/AHUB_WEB/`）的未混淆代码里，
   解码器逐字为：
   ```js
   function Nf(o){const t=o[0]??0,
     e={conProtocol:t&15, sensor:t>>4&15, cpiLev:o[1]??0,
        xCpiVal:H0(o,2), yCpiVal:H0(o,4),
        batStatus:o[6]??0, batPercent:o[7]??0};
   ```
   且电量只在 `handleReport(e===7)` 中被动解析，**没有任何轮询电量的命令**。
   这与本机实测偏移逐一对应（WebHID 不含 report ID，故比原生 HID 少 1）。

---

## 6. 使用方式

```powershell
# 读一次电量（最多等 30 秒，移动鼠标可加快上报）
cargo run --locked -- --probe --device rapoo-vt7

# 启动持续托盘监控
Start-Process .\target\release\mouse.exe

# 同时被动抓取有线/接收器状态报告
cargo run --locked --example capture_vt7 -- 25

# 使用协议样本验证解码
cargo test --locked devices::rapoo::vt7
```

### 注意事项

- **浏览器会独占设备**：如果 Chrome/Edge 标签页正开着雷柏网页驱动（WebHID），
  本工具会打不开集合（或收不到广播）。先关掉该标签页再运行。
- 接收器曾观察到约 7 秒广播，但实测也存在长时间静默，USB 模式未确认固定周期。
  官方只在**数值变化时**向 UI 发电量通知；首次读取可能需要等待或移动鼠标。
- 本工具**只读**，不发送任何命令，不会改变鼠标设置，也不能误触固件升级。

---

## 7. 相关文件

| 文件 | 作用 |
|---|---|
| `src/devices/rapoo/vt7.rs` | 集合匹配和纯电量解码，包含真实报告测试 |
| `src/transport/hid.rs` | HID 枚举、自动选择与被动状态读取 |
| `src/monitor.rs` | 上报时间、读数缓存与重连 |
| `examples/capture_vt7.rs` | 同时被动监听 VT7 有线与接收器状态通道 |
| `examples/inspect_hid.rs` | 指定 VID/PID 的 HID 元数据、重建描述符和可选被动监听 |
| `docs/Rapoo VT7/USB.md` | 有线 USB 实机抓包与连接差异 |

本文初次逆向时使用过临时 PowerShell 探测脚本；它们不属于当前发布内容。
当前版本的可复现工具和启动步骤以上述 Rust 文件与 README 为准。

## 8. 参考来源

- 雷柏官方网页驱动（协议权威来源）：<https://hub.rapoo.cn/>，实现代码
  <https://hid.rapoo.cn/AHUB_WEB/dist/assets/js/index-BSl-zvvI.js>
- 雷柏官方桌面驱动 **A HUB**（Electron，与网页驱动共用协议实现；含 "VT7 双8K版"）：
  下载页 <https://www.rapoo.cn/downloadcenter>，
  Windows `https://rpd.rapoo.cn/download/A%20HUB-Game-Win_V1.1.0.exe`（约 349 MB），
  macOS `https://rpd.rapoo.cn/download/A%20HUB_Game_MAC_v1.1.0.zip`
- 社区实现 ClickSync（偏移结论一致）：<https://github.com/Nuitfanee/ClickSync>
- Linux 内核 `hid-rapoo.c`（**仅支持 PID 0x2015，无电量支持**，与本设备无关）：
  <https://github.com/torvalds/linux/blob/master/drivers/hid/hid-rapoo.c>

### 关于"没有公开资料"这一点

调研确认：**不存在**任何针对雷柏 HID 协议的第三方规格文档、Wireshark 解析器或泄露笔记
（已检索 Chiphell / 知乎 / B站 / CSDN / 吾爱破解 / 数码之家 / 恩山 / 什么值得买，
只有拆机评测内容）。libratbag **完全不支持**雷柏；`usb.ids` 的 0x24ae 条目中**没有 0x1413**；
linux-hardware.org 也查不到该设备。因此本文档的字节布局属于**从官方实现推导并经本机实测验证**，
而非引用现成的社区文档。

同时确认一个**否定结论**：这套协议**不是** Logitech HID++ 兼容。HID++ 使用 report ID
`0x10`(短,7字节)/`0x11`(长,20字节) 并带 deviceIndex/featureIndex 寻址；本设备的厂商
report ID 是 `0x07`/`0x08`/`0x09`，且 `0x10` 是 payload 的位域（`conProtocol | sensor<<4`）
而非 report ID。表面相似属巧合。
