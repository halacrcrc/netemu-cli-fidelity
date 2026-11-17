//! 实验检查的 JSON 协议：把 Web `LabCheckDef.kind` 映射到 `lab_checks::CheckKind`。
//!
//! 这里只做「声明 → 检查项」的转换，**不复制任何判断语义**；判分仍由
//! `lab_checks::run_read_only_checks` / `run_active_checks` 共用同一套 `run_one` 逻辑完成。
//! 设备名按真实引擎的 `Device::name` 解析，不新建隐藏设备、不推进协议。

use lab_checks::{CheckGroup, CheckKind, LabCheck, LabCheckResult};
use serde_json::Value;
use sim_core::device::{MuxRole, RouteSrc, StpRole, VtyTransport};
use sim_core::engine::Engine;
use sim_packets::{ipv4_from_str, ipv6_from_str};

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value, String> {
    v.get(key).ok_or_else(|| format!("检查项缺少字段 {key}"))
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    field(v, key)?
        .as_str()
        .ok_or_else(|| format!("检查项字段 {key} 必须是字符串"))
}

fn number(v: &Value, key: &str) -> Result<u64, String> {
    field(v, key)?
        .as_u64()
        .ok_or_else(|| format!("检查项字段 {key} 必须是非负整数"))
}

fn flag(v: &Value, key: &str, default: bool) -> Result<bool, String> {
    match v.get(key) {
        None => Ok(default),
        Some(x) => x
            .as_bool()
            .ok_or_else(|| format!("检查项字段 {key} 必须是布尔值")),
    }
}

fn dev(eng: &Engine, v: &Value, key: &str) -> Result<u32, String> {
    let name = text(v, key)?;
    eng.net
        .devices
        .iter()
        .find(|d| d.name == name)
        .map(|d| d.id)
        .ok_or_else(|| format!("找不到设备 {name}"))
}

fn ipv4(v: &Value, key: &str) -> Result<sim_packets::Ipv4Addr, String> {
    ipv4_from_str(text(v, key)?).ok_or_else(|| format!("{key} 不是合法 IPv4 地址"))
}

fn ipv6(v: &Value, key: &str) -> Result<sim_packets::Ipv6Addr, String> {
    ipv6_from_str(text(v, key)?).ok_or_else(|| format!("{key} 不是合法 IPv6 地址"))
}

fn net(v: &Value, key: &str) -> Result<(sim_packets::Ipv4Addr, u8), String> {
    lab_checks::parse_net(text(v, key)?).ok_or_else(|| format!("{key} 必须是 a.b.c.d/len"))
}

fn route_exists(eng: &Engine, v: &Value, src: Option<RouteSrc>) -> Result<CheckKind, String> {
    let (dst, len) = net(v, "net")?;
    Ok(CheckKind::RouteExists {
        dev: dev(eng, v, "dev")?,
        dst,
        len,
        src,
    })
}

fn stp_role(v: &Value, key: &str) -> Result<StpRole, String> {
    match text(v, key)? {
        "Designated" | "DESI" => Ok(StpRole::Designated),
        "Root" | "ROOT" => Ok(StpRole::Root),
        "Alternate" | "ALTE" => Ok(StpRole::Alternate),
        other => Err(format!("未知 STP 端口角色 {other}")),
    }
}

fn mux_role(v: &Value) -> Result<MuxRole, String> {
    match text(v, "role")? {
        "principal" => Ok(MuxRole::Principal),
        "separate" => Ok(MuxRole::Separate),
        "group" => Ok(MuxRole::Group(number(v, "group")? as u16)),
        other => Err(format!("未知 MUX VLAN 角色 {other}")),
    }
}

fn vty(v: &Value) -> Result<VtyTransport, String> {
    match text(v, "transport")?.to_ascii_lowercase().as_str() {
        "all" => Ok(VtyTransport::All),
        "ssh" => Ok(VtyTransport::Ssh),
        "telnet" => Ok(VtyTransport::Telnet),
        "none" => Ok(VtyTransport::None),
        other => Err(format!("未知 VTY 接入协议 {other}")),
    }
}

fn mgmt_proto(v: &Value) -> Result<sim_core::device::MgmtProto, String> {
    match text(v, "proto")?.to_ascii_lowercase().as_str() {
        "ssh" => Ok(sim_core::device::MgmtProto::Ssh),
        "telnet" => Ok(sim_core::device::MgmtProto::Telnet),
        other => Err(format!("未知登录协议 {other}")),
    }
}

/// 单个声明 → 检查组（`routeAbsent` 取反、`portRoleEither` 二选一）。
fn group_from_json(eng: &Engine, v: &Value) -> Result<CheckGroup, String> {
    use CheckKind as K;
    let single = |k: K| CheckGroup::Single(k);
    let g = match text(v, "type")? {
        // 主动探测（负向 expect 同样是主动行为）
        "ping" => single(K::Ping {
            from: dev(eng, v, "from")?,
            to: ipv4(v, "to")?,
            expect: flag(v, "expect", true)?,
        }),
        "ping6" => single(K::Ping6 {
            from: dev(eng, v, "from")?,
            to: ipv6(v, "to")?,
            expect: flag(v, "expect", true)?,
        }),
        "tcpProbe" => single(K::TcpProbe {
            from: dev(eng, v, "from")?,
            to: ipv4(v, "to")?,
            port: number(v, "port")? as u16,
            expect: flag(v, "expect", true)?,
        }),
        "login" => single(K::LoginAttempt {
            from: dev(eng, v, "from")?,
            to: ipv4(v, "to")?,
            proto: mgmt_proto(v)?,
            user: text(v, "user")?.to_string(),
            pass: text(v, "pass")?.to_string(),
            expect: flag(v, "expect", true)?,
        }),

        // 纯状态读取
        "sviIp" => single(K::SviIp {
            dev: dev(eng, v, "dev")?,
            vlan: number(v, "vlan")? as u16,
            ip: ipv4(v, "ip")?,
        }),
        "accessVlan" => single(K::AccessVlan {
            dev: dev(eng, v, "dev")?,
            port: number(v, "port")? as usize,
            vlan: number(v, "vlan")? as u16,
        }),
        "trunkAllows" => single(K::TrunkAllows {
            dev: dev(eng, v, "dev")?,
            port: number(v, "port")? as usize,
            vlan: number(v, "vlan")? as u16,
        }),
        "vlanExists" => single(K::VlanExists {
            dev: dev(eng, v, "dev")?,
            vlan: number(v, "vlan")? as u16,
        }),
        "staticRoute" => single(route_exists(eng, v, Some(RouteSrc::Static))?),
        "routeExists" => single(route_exists(eng, v, None)?),
        "routeAbsent" => CheckGroup::Negate(route_exists(eng, v, None)?),
        "portIp" => single(K::PortIp {
            dev: dev(eng, v, "dev")?,
            port: number(v, "port")? as usize,
            ip: ipv4(v, "ip")?,
        }),
        "mplsLsp" => single(K::MplsLsp {
            dev: dev(eng, v, "dev")?,
            name: text(v, "name")?.to_string(),
        }),
        "vrrpMaster" => single(K::VrrpMaster {
            dev: dev(eng, v, "dev")?,
            vrid: number(v, "vrid")? as u8,
        }),
        "ospfNeighbors" => single(K::OspfNeighbors {
            dev: dev(eng, v, "dev")?,
            min: number(v, "min")? as usize,
        }),
        "ospfv3Neighbors" => single(K::Ospfv3Neighbors {
            dev: dev(eng, v, "dev")?,
            min: number(v, "min")? as usize,
        }),
        "securityZone" => single(K::SecurityZone {
            dev: dev(eng, v, "dev")?,
            port: number(v, "port")? as usize,
            zone: text(v, "zone")?.to_string(),
        }),
        "securityPolicy" => single(K::SecurityPolicy {
            dev: dev(eng, v, "dev")?,
            name: text(v, "name")?.to_string(),
            src_zone: text(v, "srcZone")?.to_string(),
            dst_zone: text(v, "dstZone")?.to_string(),
            permit: flag(v, "permit", false)?,
        }),
        "ipsecTunnel" => single(K::IpsecTunnel {
            dev: dev(eng, v, "dev")?,
            name: text(v, "name")?.to_string(),
        }),
        "vpnConnected" => single(K::VpnConnected {
            dev: dev(eng, v, "dev")?,
        }),
        "wlanApJoined" => single(K::WlanApJoined {
            ac: dev(eng, v, "ac")?,
            ap: dev(eng, v, "ap")?,
        }),
        "wlanSsid" => single(K::WlanSsid {
            ac: dev(eng, v, "ac")?,
            ssid: text(v, "ssid")?.to_string(),
        }),
        "wifiConnected" => single(K::WifiConnected {
            dev: dev(eng, v, "dev")?,
        }),
        "wifiOnAp" => single(K::WifiOnAp {
            dev: dev(eng, v, "dev")?,
            ap: dev(eng, v, "ap")?,
        }),
        "vxlanVni" => single(K::VxlanVni {
            dev: dev(eng, v, "dev")?,
            vlan: number(v, "vlan")? as u16,
            vni: number(v, "vni")? as u32,
        }),
        "stackMember" => single(K::StackMember {
            dev: dev(eng, v, "dev")?,
            domain: number(v, "domain")? as u16,
            member: number(v, "member")? as u8,
        }),
        "mlagDomain" => single(K::MlagDomain {
            dev: dev(eng, v, "dev")?,
            domain: number(v, "domain")? as u16,
        }),
        "evpnRoute" => single(K::EvpnRoute {
            dev: dev(eng, v, "dev")?,
            vtep: ipv4(v, "vtep")?,
            vni: number(v, "vni")? as u32,
        }),
        "ltmVirtual" => single(K::LtmVirtual {
            dev: dev(eng, v, "dev")?,
            name: text(v, "name")?.to_string(),
            vip: ipv4(v, "vip")?,
            port: number(v, "port")? as u16,
        }),
        // 阶段 5A：主动验证多个 TCP 会话确实分配到不同 Member。
        "ltmDistribution" => single(K::LtmDistribution {
            dev: dev(eng, v, "dev")?,
            from: dev(eng, v, "from")?,
            to: ipv4(v, "to")?,
            port: number(v, "port")? as u16,
            sessions: number(v, "sessions")?.max(2) as usize,
            min_distinct: number(v, "minDistinct")?.max(2) as usize,
        }),
        "portIsolate" => single(K::PortIsolate {
            dev: dev(eng, v, "dev")?,
            port: number(v, "port")? as usize,
            group: number(v, "group")? as u16,
        }),
        "muxRole" => single(K::MuxRole {
            dev: dev(eng, v, "dev")?,
            port: number(v, "port")? as usize,
            role: mux_role(v)?,
        }),
        "srv6Route" => {
            let raw = text(v, "dst")?;
            let (addr, len) = raw
                .split_once('/')
                .ok_or_else(|| "srv6Route.dst 必须是 IPv6 前缀 /len".to_string())?;
            single(K::Srv6Route {
                dev: dev(eng, v, "dev")?,
                dst: ipv6_from_str(addr)
                    .ok_or_else(|| "srv6Route.dst 不是合法 IPv6 地址".to_string())?,
                len: len
                    .parse()
                    .map_err(|_| "srv6Route.dst 前缀长度非法".to_string())?,
            })
        }
        "rootBridge" => single(K::RootBridge {
            dev: dev(eng, v, "dev")?,
        }),
        "portRole" => single(K::PortRole {
            dev: dev(eng, v, "dev")?,
            port: number(v, "port")? as usize,
            role: stp_role(v, "role")?,
        }),
        "portRoleEither" => {
            let d = dev(eng, v, "dev")?;
            CheckGroup::Either(
                K::PortRole {
                    dev: d,
                    port: number(v, "portA")? as usize,
                    role: stp_role(v, "roleA")?,
                },
                K::PortRole {
                    dev: d,
                    port: number(v, "portB")? as usize,
                    role: stp_role(v, "roleB")?,
                },
            )
        }
        "lagMember" => single(K::LagMember {
            dev: dev(eng, v, "dev")?,
            group: number(v, "group")? as u16,
            port: number(v, "port")? as usize,
        }),
        "lagSelected" => single(K::LagSelected {
            dev: dev(eng, v, "dev")?,
            group: number(v, "group")? as u16,
            count: number(v, "count")? as usize,
        }),
        "mgmtUser" => single(K::MgmtUser {
            dev: dev(eng, v, "dev")?,
            name: text(v, "name")?.to_string(),
        }),
        "vtyTransport" => single(K::VtyTransport {
            dev: dev(eng, v, "dev")?,
            transport: vty(v)?,
        }),
        other => return Err(format!("未知检查类型 {other}")),
    };
    Ok(g)
}

/// 解析 Web 侧实验检查声明；不创建设备、不推进引擎。
pub fn parse_lab_checks(eng: &Engine, json: &str) -> Result<Vec<LabCheck>, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("实验检查 JSON 非法: {e}"))?;
    let arr = value
        .as_array()
        .ok_or_else(|| "实验检查声明必须是数组".to_string())?;
    arr.iter()
        .map(|item| {
            Ok(LabCheck {
                label: text(item, "label")?.to_string(),
                group: group_from_json(eng, field(item, "kind")?)?,
            })
        })
        .collect()
}

/// 结果含行为来源与 skipped，供 UI 区分纯读 / 主动实验。
pub fn results_json(results: &[LabCheckResult]) -> String {
    Value::Array(
        results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "label": r.label,
                    "passed": r.passed,
                    "detail": r.detail,
                    "behavior": r.behavior.label(),
                    "skipped": r.skipped,
                })
            })
            .collect(),
    )
    .to_string()
}
