# ATK X1 PRO 电量协议

日期：2026-10-03，Windows 实机验证。
设备管理器/PnP 的总线产品名称为 **ATK X1 PRO**，HID 厂商字符串为 **Compx**。
本次验证的 USB 接口为 `VID_373B / PID_1016`，版本 `REV_0301`。
没有根据名称推断其他接收器 PID 的支持情况。

后续已实机验证 `373B:101A` 的已配对 1K 接收器和 `373B:101B` 的已配对 8K 接收器；型号核验和无线抓包见
[1K / 8K 接收器协议](WIRELESS.md)。

## 读取结论

本次被动监听 25 秒没有收到状态输入，不能假设它像雷柏 VT7 一样持续广播。
官方驱动通过 Compx `GetBatteryLevel` 查询电量，本机发送同一条 GET 后收到真实响应：
**95%，正在充电**。读取没有更改配置、配对或固件。

电量通道为 `MI_01 / Col05`，usage page `FF02`，usage `0002`。
原生输入与输出都是 17 字节：Report ID 8 加 16 字节 payload。
官方 WebHID 操作的 payload 不含报告 ID，本文原生偏移统一包含 ID。

## GET 请求和响应

本轮 USB 直连发送的电量读取请求：

```text
08 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 49
```

本机捕获响应：

```text
08 04 00 00 00 02 5F 01 10 50 00 00 00 00 00 00 87
```

| 原生偏移 | 含义 | 实机值 |
|---|---|---|
| `[0]` | HID Report ID | `08` |
| `[1]` | 命令 ID，`GetBatteryLevel` | `04` |
| `[2]` | 命令状态 | `00`，成功 |
| `[3..4]` | EEPROM 地址头字段；电量 GET 请求置零 | `00 00` |
| `[5]` | 有效数据长度 | `02` |
| `[6]` | 电量百分比 | `5F` = 95 |
| `[7]` | 充电码 | `01`，正在充电 |
| `[8..9]` | 官方 getter 的电压字段 | `10 50`；有效长度为 2，本程序不使用该字段 |
| `[16]` | 校验字节 | 请求 `49`，响应 `87` |

校验方式：所有 17 字节按 8 位回绕相加，应得到 `55`。
请求校验字节为 `55 - sum(前16字节)`，保留低 8 位。
响应必须同时满足长度、Report ID、命令 ID、成功状态、有效长度、电量范围和校验和，
错误或损坏响应不能更新显示值或接收时间。

充电码与 VT7 不同：官方 Compx 映射 `0` 为未充电，`1` 为正在充电。
程序保留其他值为未知，不套用雷柏的 `2 = 充电` 定义。
没有独立充满码的证据，100% 不会强制改变实际充电状态。

## 实测 HID 集合

| usage page / usage | 集合 | 报告类型 | 电量用途 |
|---|---|---|---|
| `FF05:0000` | `MI_01 / Col01` | Report 16，8 字节 Input | 不使用 |
| `FF03:0000` | `MI_01 / Col02` | Report 2，8 字节 Input | 不使用 |
| `FF02:0002` | `MI_01 / Col05` | Report 8，17 字节 Input / Output | GET 请求与响应 |
| `FF04:0002` | `MI_01 / Col06` | Report 6，8 字节 Feature | 官方设备选择过滤器；本程序不在这里查询电量 |
| `FF06:0002` | `MI_01 / Col07` | Report 9，49 字节 Input / Output | 不使用，未探测其命令 |

还有系统键盘、消费者控制、系统控制和鼠标输入集合；不读取它们的用户输入。
Windows HIDAPI 从 preparsed data 重建的电量集合描述符为：

```text
06 02 FF 09 02 A1 01 85 08 09 02 15 00 26 FF 00 75 08
95 10 81 00 09 02 15 00 26 FF 00 75 08 95 10 91 00 C0
```

此描述符是 Windows 已解析集合的重建结果，不是 USB 总线上原始描述符抓包。
`85 08` 定义 Report ID 8，`75 08 / 95 10` 定义 16 字节 payload。

## 官方来源与复现

静态读取 [ATK 官方网页驱动](https://hub.atk.pro/) 所引用的
[3.2.27 主脚本](https://bpcdn.atkgear.com/hub-v3/production/3.2.27/static/index-F-MYsmWR.js)。
现场下载文件 SHA-256 为 `275EC10D7BF0916FEA8F27F6CC6415C3EFF6E02E682C8164E286626F0F394256`。
只下载源码，没有打开 WebHID 设备或执行官方初始化、配置写入和升级流程。

关键对应关系：manifest 中 `vendorId=14139`、`productId=4118` 对应 `373B:1016`；
`hM.get` 使用 `GetBatteryLevel=4`；`Mo` 的 payload 长度为 16、数据基址为 5；
`bM.transferForResult` 使用 HID Report 8；`Ih.createBattery` 读取百分比和充电码。
官方选设备时过滤 `FF04:0002`，WebHID 按物理 HID 接口暴露集合；
Windows 原生发送 Report 8 时需要打开实际有 Input/Output 8 的 `FF02:0002` 集合。

```powershell
# 读取元数据和重建描述符
cargo run --locked --example inspect_hid -- 373b 1016
# 被动监听 Report 8 通道
cargo run --locked --example inspect_hid -- 373b 1016 ff02 2 25
# 仅发送已确认的 GetBatteryLevel
cargo run --locked --example query_atk
# 使用正式传输后端读取一次
cargo run --locked -- --probe --device atk-x1-pro
```

现场证据留在 `target/atk-hid-descriptors.txt`、`target/atk-passive-capture.txt`、
`target/atk-battery-query.txt`、`target/atk-official.js`。

## 实现边界

驱动在 `src/devices/atk/x1_pro.rs`，只做精确集合匹配和响应解码。
`src/transport/compx.rs` 实现独立主动读取连接，只允许固定电量 GET 和接收器型号 GET；
连接后立即查询，之后每 15 秒查询，3 秒内未收到合法响应则交由监控层重连。
读失败时保留上次真实读数和时间，不制造默认 100% 或充电值。

雷柏被动通道不发送命令。两类连接都复用 `BatteryConnection`、`Session`、缓存和通用 UI。
ATK 时间显示“上次上报”，雷柏显示“上次广播”，都只记录实际收到合法报告的时间。
当前是单活动鼠标显示；另一只鼠标的读数不会混入当前设备会话。
已验证的无线接收器仅限 `373B:101A` / `373B:101B` 且配对型号为 `2/38`；
其他接收器 PID、蓝牙、充满码和配置/升级协议尚未验证。
