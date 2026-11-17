---
title: 防火墙：安全区域与状态化策略
order: 8
level: IP
topic: security
lab: firewall-zone
---

路由器的 ACL 是"无状态"的：去程放行，回程还得再写一条。防火墙的核心升级是**安全区域 + 状态化会话**：你只需要描述"谁可以主动访问谁"，回包由会话表自动放行。

> **本课基线是华为 VRP 安全策略**（USG / FW 系列文档基线），并同时给出 Cisco IOS XE 17.x ZBF 的真实对照。模拟器两套策略都已实现数据面（按官方语义）：区域与策略命令、会话表、规则启用/禁用/改序（VRP）、class-map / policy-map / zone-pair / service-policy（IOS）都可执行。Cisco ASA 的 `nameif` / `security-level`（安全级别回退）不是任何一条基线，模拟器已把它从命令树移除，只作为对照概念在本页说明。

## 区域是第一道抽象

把接口按信任程度划进区域，常见三类：

- **trust**：内网，信任度最高；
- **untrust**：互联网，信任度最低；
- **dmz**：对外发布服务器，介于两者之间。

区域命令两套平台写法不同，但成员关系是同一个状态：一个接口只属于一个区域。

```
华为 VRP（本课基线）：
firewall zone trust
 add interface g0/0/0
 quit
firewall zone untrust
 add interface g0/0/1
 quit

思科 IOS XE 17.x ZBF：
zone security INSIDE
 exit
zone security OUTSIDE
 exit
interface g0/0
 zone-member security INSIDE
 exit
interface g0/1
 zone-member security OUTSIDE
```

**不同区域之间默认拒绝**。具体缺省行为两平台不同，必须分清：

- VRP：缺省安全策略 `default`（匹配条件 any、动作 deny）；同区域转发同样要有策略（官方："缺省情况下域内安全策略（如 trust→trust）都是禁止的"）。
- IOS ZBF：区域对没有配置策略就丢弃；**同一区域内的流量缺省放行**；一个已划区域口和一个未划区域口之间始终丢弃；`self` 区域是缺省拒绝的唯一例外（设备自身收发流量缺省允许）。

## 安全策略与状态会话

VRP 策略按顺序匹配，**首条启用且命中的规则**生效；没有命中就走缺省策略（拒绝）。放行 `trust → untrust` 后，内网发起的连接首包会生成一条**会话表项**；**后续报文命中会话表就直接转发，不再复查安全策略**，所以回包不需要反向策略。

```
华为 VRP（本课基线）：
security-policy
 rule name allow-out
  source-zone trust
  destination-zone untrust
  action permit
```

IOS ZBF 没有 `source` / `destination` / `action permit` 这类 zone-pair 子命令。策略是三层对象：**class-map 定义匹配 → policy-map 给匹配挂动作 → zone-pair 指定方向并挂 policy-map**。官方模式与提示符：

```
R1(config)# class-map type inspect match-any ICMP
R1(config-cmap)# match protocol icmp
R1(config-cmap)# exit
R1(config)# policy-map type inspect PMAP
R1(config-pmap)# class type inspect ICMP
R1(config-pmap-c)# inspect
R1(config-pmap-c)# exit
R1(config-pmap)# exit
R1(config)# zone-pair security IN-OUT source INSIDE destination OUTSIDE
R1(config-sec-zone-pair)# service-policy type inspect PMAP
```

三种动作的差别就是数据面的差别：

- `inspect`：状态化检查。首包建立会话，回包命中会话表放行；反向不需要 zone-pair。
- `pass`：完全绕过防火墙检查、**不建会话**。反向流量必须有镜像的 pass 策略，否则被丢弃。
- `drop`：命中即丢弃。policy-map 里没有 `class-default`、或 `class-default` 未配置动作时，未命中流量也按官方缺省丢弃。

`class-map` 的 `match-any` / `match-all` 决定多条 `match` 之间是"或"还是"与"；`class-default` 是系统类，始终排在最后。

### 会话表不复查策略，意味着什么

这是官方行为，也是运维中最容易被误解的一点：

- **禁用 / 删除 / 改序**策略，只影响**新流**；已经建立的会话会继续按会话表转发，直到空闲老化。
- 要让变更对既有流**立即生效**，必须清除会话表：`reset firewall session table`。清除后所有依赖会话表转发的连接会被强行中断。
- 因此"策略改了但业务还通"不是模拟器 bug，而是基线语义。

规则维护命令（VRP）：`rule name <n>` 下的 `enable` / `disable`（缺省启用）、`security-policy` 下的 `rule move <n1> before|after <n2>`、`undo rule name <n>`。IOS 侧对应的是修改 class-map / policy-map / zone-pair 本身；已建立的 inspect 会话同样不会被重新分类。

验证：

```
华为 VRP：
display firewall zone
display security-policy
display security-policy rule name allow-out
display firewall session table

思科 IOS XE 17.x ZBF：
show zone security
show zone-pair security
show policy-map type inspect zone-pair
show policy-map type inspect zone-pair session
show policy-map type inspect zone-pair IN-OUT session
```

> 模拟器边界（如实说明）：IOS ZBF 的 `match protocol` 只承载 ICMP、TCP 与常见 TCP 服务端口（http/ftp/smtp/pop3/imap/dns）的 L4 匹配；UDP 与 L7/ALG（H.323、SIP、SCCP 等）没有数据面，配置可接受但不会命中。`parameter-map type inspect` 的参数、层次化 policy-map 与 `platform inspect match-statistics` 属未模拟命令（结构化结果为「仅识别 / 未模拟」）。ZBF 与 NAT 的先后顺序也尚未校准。VRP 的 `display` 只在用户视图提供，系统视图下请先 `return`。

## 动手

打开实验十：PC1 在 trust、SRV 在 untrust。先让 SRV 主动 ping PC1——应失败；再让 PC1 ping SRV——应成功，且 SRV 的回包靠会话表通过。然后把 `allow-out` 规则 `disable` 并 `reset firewall session table`，重新 ping：新流不再命中已禁用规则，业务立即中断。这正好把"策略、会话、清表"三件事串起来。