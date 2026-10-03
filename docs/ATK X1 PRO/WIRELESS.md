# ATK X1 PRO：8K 接收器电量协议

2026-10-03，Windows 实机验证。用户将 X1 PRO 从 USB 直连切换到已配对接收器。
当前枚举到 **ATK Mouse 8K Dongle**：`373B:101B / REV_0216`，HID 厂商 `Compx`。
鼠标本体 USB `373B:1016` 已不在当前设备列表中。

接收器复用 USB 直连的 `MI_01 / Col05 / FF02:0002` 通道：
原生 Report 8，输入和输出各 17 字节。重建描述符与直连模式一致。
完整集合元数据记录在 `target/atk-dongle-hid-descriptors.txt`。

## 配对型号验证

`101B` 是通用接收器 PID，不能据此认定配对鼠标一定是 X1 PRO。
官方驱动先用 `GetMouseCIDMID`（命令 `10`）获取配对型号。
现场只发送该型号 GET 和电量 GET，没有发送配对、退出配对、配置或升级命令。

```text
GetMouseCIDMID 请求：
08 10 00 00 00 00 00 00 00 00 00 00 00 00 00 00 3D

真实响应：
08 10 00 00 00 03 02 26 2D 00 00 00 00 00 00 00 E5
```

原生 `[6]` 为 CID，`[7]` 为 MID。返回 **2 / 38**，
与 [ATK 官方驱动](https://hub.atk.pro/) 的 X1 PRO `mouseCidMid = "2,38"` 一致。
同一份官方 manifest 把十进制 `productId=4123`（十六进制 `101B`）列为 8K Dongle。
源码地址和下载哈希见 [USB/通用协议](PROTOCOL.md)。

型号响应必须满足 Report 8、命令 `10`、成功状态、有效长度和校验和。
连接后端只在 CID/MID 为 `2/38` 时接受此接收器用于 X1 PRO；
其他型号、错误响应或超时会拒绝当前连接并继续扫描，不冒用 X1 PRO 身份。
本次只实现建立连接时的型号核验，不声明覆盖运行中重新配对到其他型号的所有情况。

## 真实电量响应

```text
GetBatteryLevel 请求：
08 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 49

真实无线响应：
08 04 00 00 00 02 5F 00 10 05 00 00 00 00 00 00 D3
```

`[6] = 5F` 为 **95%**，`[7] = 00` 为 **未充电**。
此前 USB 直连响应 `[7] = 01` 为正在充电；切换连接方式后必须采用新响应，不能继续沿用旧充电码。
所有 17 字节回绕相加为 `55`；百分比、状态和字段偏移与直连协议一致。

## 程序接入

- `src/devices/atk/x1_pro.rs` 精确支持 `1016` 和已验证的 `101B` 状态集合。
- 有线优先级为 2，接收器为 1；有线消失后自动重连可选接收器。
- `ReadMethod` 携带接收器预期型号，由独立 Compx 后端执行型号 GET。
- `src/devices/protocols/compx.rs` 提供共享响应校验与型号解码，具体鼠标驱动保持纯解码。
- 固定允许电量 GET 和配对型号 GET，不提供任意命令、配对或升级入口。
- 每 15 秒读取电量；失败时缓存和原接收时间保持不变，界面只显示电量、充电状态及上次上报时间。

```powershell
cargo run --locked --example inspect_hid -- 373b 101b
cargo run --locked --example query_atk -- 101b
cargo run --locked -- --probe --device atk-x1-pro
```

原始 GET 请求/响应保留在 `target/atk-receiver-query.txt`。
不扩宽支持到其他 1K/4K/8K 接收器 PID 或其他配对型号；蓝牙仍未验证。
