---
title: "F5 LTM: Sharing Traffic Across a Server Farm"
order: 14
level: IE
topic: ltm
lab: f5-ltm
---

When one web server cannot keep up, the obvious fix is more servers — but users remember only one address. That is where a **load balancer** comes in: it exposes a virtual IP (VIP) to the outside and distributes connections across a pool of servers.

## The VIP / Pool / Member model

- **Virtual Server (VS)**: the public VIP plus port, for example 10.0.0.100:80;
- **Pool**: a group of back-end servers, for example 10.0.1.10:80 and 10.0.2.11:80;
- **Member**: one server in the pool, which can be marked healthy or taken out individually;
- **Algorithm**: round robin is the simplest; least connections or hashing are alternatives.

Starting with Stage 5A, LTM uses the independent F5 BIG-IP **TMOS 17.x / tmsh** dialect (`Dialect::Tmos`), not the teaching abstraction that used to hang off IOS/VRP/Comware. Official `ltm pool` / `ltm virtual` syntax and module navigation are documented in F5 TMSH Reference v17.

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

Objects live in the default `/Common` partition; `exit` leaves the current module and `quit` leaves tmsh entirely.

## How the addresses are rewritten

Clients know only the VIP. When a connection arrives, the LTM does two things:

- **DNAT**: rewrite the destination from the VIP to the selected member's IP;
- **SNAT / return path**: record the session so the member's replies return to the right client and leave from the VIP.

As a result, servers may see a source address other than the client's real one, while the client always believes it is talking to the VIP. The Sessions section of `show ltm pool` shows the "client → VIP → member" mapping directly.

## Hands-on

Open Lab 19: LB1 has three interfaces for the client and two servers. After configuring the pool and VS with the tmsh sequence from the lab tasks, run `tcp 10.0.0.100 80` three times from PC1 — all should connect. The Sessions list in `show ltm pool` should contain three entries landing on at least two different members, proving round robin. Shut down one server and connect again to see whether traffic lands only on the surviving member.
