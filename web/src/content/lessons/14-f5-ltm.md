---
title: F5 LTM：把流量分给一群服务器
order: 14
level: IE
topic: ltm
lab: f5-ltm
---

一台 Web 服务器扛不住流量时，最直接的办法是加服务器。但用户只会记住一个地址，于是需要**负载均衡器**：对外暴露一个虚拟 IP（VIP），对内把连接分发给服务器池里的成员。

## VIP、Pool、Member 三层模型

- **Virtual Server（VS）**：对外提供服务的 VIP + 端口，例如 10.0.0.100:80；
- **Pool**：一组后端服务器，例如 10.0.1.10:80 与 10.0.2.11:80；
- **Member**：池里的单台服务器，可单独标记健康/下线；
- **负载算法**：轮询（Round Robin）最简单，也可按最少连接、哈希等策略分发。

阶段 5A 起，LTM 命令使用 F5 BIG-IP **TMOS 17.x / tmsh** 的独立方言（`Dialect::Tmos`），不再是挂在 IOS/VRP/Comware 上的教学抽象。官方 `ltm pool` / `ltm virtual` 语法与模块层级见 F5 TMSH Reference v17。

```
(tmos)# ltm
(tmos.ltm)# create pool WEB
(tmos.ltm)# modify pool WEB members add { 10.0.1.10:80 }
(tmos.ltm)# modify pool WEB members add { 10.0.2.11:80 }
(tmos.ltm)# create virtual VS destination 10.0.0.100:80 pool WEB
(tmos.ltm)# list pool
(tmos.ltm)# exit
(tmos)#
```

对象默认位于 `/Common` 分区；`exit` 退出当前模块返回上一级，`quit` 退出整个 tmsh。

## 地址是怎么被改写的

客户端只认识 VIP。LTM 收到连接后做两件事：

- **DNAT**：把目的地址从 VIP 改成选中成员的 IP；
- **SNAT / 回程**：记录会话，让成员的回包能回到正确的客户端，并从 VIP 发出。

所以服务器看到的源地址可能不是客户端真实地址，而客户端始终以为自己在和 VIP 通信。`show ltm pool` 的会话段能直接看到「客户端 → VIP → 成员」的映射。

## 动手

打开实验十九：LB1 三个接口分别接客户端与两台服务器。按课程原文的 tmsh 序列配好池与 VS 后，在 PC1 终端连续执行三次 `tcp 10.0.0.100 80`，三次都应连接成功；`show ltm pool` 的 Sessions 应出现三条会话，且至少落到两个不同成员——轮询生效。把其中一台服务器关机再连接，观察流量是否只落到存活成员。
