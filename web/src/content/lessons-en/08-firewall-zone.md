---
title: "Firewalls: Security Zones and Stateful Policy"
order: 8
level: IP
topic: security
lab: firewall-zone
---

A router ACL is stateless: permit the outbound direction and you still need a rule for the reply. The big upgrade in a firewall is **security zones plus stateful sessions** — you describe only who may initiate to whom, and replies are permitted automatically by the session table.

> **The baseline for this lesson is the Huawei VRP security policy** (USG / FW documentation baseline), with a real Cisco IOS XE 17.x ZBF contrast. Both policy models are implemented in the simulator's data plane: zone and policy commands, the session table, rule enable/disable/reorder (VRP), and class-map / policy-map / zone-pair / service-policy (IOS) all execute. Cisco ASA `nameif` / `security-level` (security-level fallback) is not part of either baseline; the simulator removed it from the command tree and mentions it here only as a contrast.

## Zones are the first abstraction

Interfaces are grouped into zones by trust level. Three common ones:

- **trust**: the internal network, highest trust;
- **untrust**: the Internet, lowest trust;
- **dmz**: servers published to the outside, in between.

The zone commands differ between platforms, but membership is the same state: an interface belongs to at most one zone.

```
Huawei VRP (this lesson's baseline):
firewall zone trust
 add interface g0/0/0
 quit
firewall zone untrust
 add interface g0/0/1
 quit

Cisco IOS XE 17.x ZBF:
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

**Traffic between different zones is denied by default.** The exact defaults differ and must not be mixed up:

- VRP: the default security policy `default` (match any, action deny) applies; same-zone forwarding also needs a policy (the official documentation states that by default intra-zone policy, for example trust to trust, is denied).
- IOS ZBF: traffic is dropped when no policy is configured on the zone pair; **traffic within the same zone is allowed by default**; a zoned interface and a non-zoned interface always drop; the `self` zone is the only exception to the default deny (traffic to or from the device is allowed by default).

## Security policy and stateful sessions

VRP policies are matched in order and the **first enabled rule that matches** wins; if nothing matches, the default policy (deny) applies. After permitting `trust -> untrust`, the first packet of an inside-initiated connection creates a **session entry**; **later packets hit the session table and are forwarded without re-checking the policy**, so no reverse policy is needed for replies.

```
Huawei VRP (this lesson's baseline):
security-policy
 rule name allow-out
  source-zone trust
  destination-zone untrust
  action permit
```

IOS ZBF has no zone-pair subcommands such as `source` / `destination` / `action permit`. A policy is three object layers: **class-map matches traffic → policy-map attaches an action to a class → zone-pair selects the direction and attaches the policy-map**. Official modes and prompts:

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

The three actions differ exactly where the data plane differs:

- `inspect`: stateful. The first packet creates a session; replies are permitted by the session table, and no reverse zone-pair is required.
- `pass`: bypasses the firewall check entirely and **creates no session**. Reverse traffic needs a mirrored pass policy or it is dropped.
- `drop`: matched traffic is dropped. If a policy-map has no `class-default`, or `class-default` has no action, unmatched traffic is dropped by the documented default as well.

`match-any` / `match-all` in a class-map decide whether multiple `match` statements are ORed or ANDed; `class-default` is the system class and always comes last.

### What "the session table does not re-check the policy" means

This is documented behaviour and the most commonly misunderstood part of operations:

- **Disabling / deleting / reordering** a policy only affects **new flows**; established sessions keep forwarding through the session table until they idle out.
- To make a change take effect **immediately** on existing flows you must clear the session table: `reset firewall session table`. Connections that depend on it are torn down.
- So "the policy changed but traffic still flows" is baseline semantics, not a simulator bug.

Rule maintenance (VRP): `enable` / `disable` under `rule name <n>` (enabled by default), `rule move <n1> before|after <n2>` under `security-policy`, and `undo rule name <n>`. On IOS you edit the class-map / policy-map / zone-pair itself; established inspect sessions are not reclassified either.

Verify with:

```
Huawei VRP:
display firewall zone
display security-policy
display security-policy rule name allow-out
display firewall session table

Cisco IOS XE 17.x ZBF:
show zone security
show zone-pair security
show policy-map type inspect zone-pair
show policy-map type inspect zone-pair session
show policy-map type inspect zone-pair IN-OUT session
```

> Simulator boundaries (stated honestly): IOS ZBF `match protocol` only implements L4 matching for ICMP, TCP and common TCP service ports (http/ftp/smtp/pop3/imap/dns). UDP and L7/ALG inspection (H.323, SIP, SCCP, ...) have no data plane: the configuration is accepted but never matches. `parameter-map type inspect` parameters, hierarchical policy-maps and `platform inspect match-statistics` are not simulated (structured result: "recognized only / not simulated"). The ordering of ZBF and NAT is not calibrated either. In this simulator VRP `display` is only available in the user view; from the system view run `return` first.

## Hands-on

Open Lab 10: PC1 is in trust and SRV is in untrust. First have SRV ping PC1 — it should fail; then PC1 pings SRV — it should succeed, with the reply riding the session table. Then `disable` the `allow-out` rule and `reset firewall session table` and ping again: new flows no longer match the disabled rule and traffic stops immediately. That ties "policy, session, clear" together.