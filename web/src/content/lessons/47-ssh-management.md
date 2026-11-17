---
title: SSH 远程管理：VTY、本地用户与传输
order: 47
level: IA
topic: mgmt
lab: mgmt-ssh
---

真实网络里，跑到设备跟前接 Console 线不现实。远程管理走 VTY（虚拟终端）线路，而 **SSH 是生产环境的最低要求**——Telnet 全程明文，口令抓包即得。

## SSH 登录要过三道门

1. **VTY 线路放行**：`line vty 0 4`（华为 `user-interface vty 0 4`）下用 `transport input ssh` / `protocol inbound ssh` 限制只允许 SSH，Telnet 直接被拒。
2. **认证凭据**：`login local`（IOS）或 `authentication-mode aaa`（华为/华三）——让 VTY 用本地用户表或 AAA 验证，而不是不验证。
3. **服务端用户**：`username admin secret cisco123`（IOS）/ `aaa` + `local-user admin`（华为/华三）——**认证发生在服务器端**，没建用户就没人能登录。

这三道门都过了，才有资格谈「能不能连上」：登录本身也是数据面流量。

## 经典坑：配了用户还是登不上

SSH 需要设备生成加密密钥。密钥没生成时，用户名密码都对也登不上。**真实设备**上的排障顺序：

```
1. show ip ssh / display ssh server status   ← 服务启用了吗？
2. 检查 VTY 的 transport / protocol inbound   ← 放行 SSH 了吗？
3. 检查本地用户是否存在                        ← 有凭据可验证吗？
```

> 第 1 步只能在真机做：本模拟器没有实现 `show ip ssh` / `display ssh server status`（SSH 服务默认开启、不模拟密钥生成，因此不存在「密钥没生成」这种故障态）。在模拟器里直接从第 2、3 步验证：发起 `ssh` 登录，`Connection refused` 查 VTY 传输策略，`Permission denied` 查用户与口令。真实设备上密钥未生成（IOS 缺 `crypto key generate rsa`）是「配了用户仍登不上」的头号原因，这一点模拟器覆盖不到。

## 登录也是数据面流量

管理面登录不会绕开转发路径。以下任一情况 `ssh` 都会失败，结论与同一目标上的 `ping` / `tcp <IP> 22` 一致：

- 对端承载管理地址的接口 `shutdown`，或链路断开；
- 中间设备没有到目标网段的路由（有网关不等于有路由）；
- 路径上的防火墙（如 ZBF zone-pair）没有放行该 TCP 端口；
- 目标放行了端口但 VTY 不允许该协议（此时是 `Connection refused`，不是不可达）。

反过来，发往设备**自身**的管理流量不受该设备 zone-pair 策略影响：ZBF 的 self 区域是「缺省拒绝」的唯一例外，`zone-pair ... destination self` 里的拒绝不改变「到本机 SSH 仍可登录」。

## SSH 会话的行为

登录成功后，提示符切换为**远端设备的 CLI**，后续每条命令都在远端执行——`exit` 关闭会话回到本地。这与 Console 的区别只是传输路径，操作体验一致。

## 动手

打开实验「远程管理（SSH/Telnet）」：给路由器建本地用户、限制 VTY 只走 SSH，再从 PC 终端 `ssh 192.168.1.1` 登录进来，提示符变成 `R1#` 说明管理面生效；登录状态下执行 `show ip interface brief` 看的是 R1 的接口，`exit` 回到 `PC1>`。再用 `telnet 192.168.1.1` 试同一个地址，应提示连接被拒绝——**把成功后的提示符和被拒的错误文本放在一起对比**，就是 Telnet 与 SSH 在管理面上的差别。

> 抓包边界：本模拟器的登录是 CLI 层抽象（用户名/口令交互直接切换会话），不模拟 TCP 握手与 SSH 密钥交换，因此链路抓包**看不到**登录过程的报文，也无法用它对比 Telnet 与 SSH 的报文体。抓包面板展示的是真实仿真帧——`ping` 的 ICMP 请求/应答、`tcp <IP> <端口>` 的 SYN/RST。
