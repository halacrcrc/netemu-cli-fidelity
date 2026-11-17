// 三方言的 running/current 配置渲染：从 DevSummary（完整配置快照）实时生成（ADR D12 单一事实源）。
// 覆盖核心段：名称/VLAN/接口（IP·模式·tag·shutdown）/SVI/静态路由/OSPF·BGP/DHCP/ACL/NAT/
// mgmt（本地用户·VTY）/VRRP/VXLAN·EVPN/LTM/M-LAG/堆叠/IS-IS/SRv6/防火墙策略。

use crate::ifname;
use crate::Dialect;
use sim_core::device::{DeviceKind, PortMode};
use sim_core::summary::DevSummary;

/// 区域号 → 点分十进制（VRP area 视图写法，如 1 → 0.0.0.1）。
fn area_to_dotted(area: u32) -> String {
    format!(
        "{}.{}.{}.{}",
        (area >> 24) & 0xff,
        (area >> 16) & 0xff,
        (area >> 8) & 0xff,
        area & 0xff
    )
}

fn is_l3(kind: DeviceKind) -> bool {
    matches!(
        kind,
        DeviceKind::Router
            | DeviceKind::Firewall
            | DeviceKind::LoadBalancer
            | DeviceKind::WlanController
    )
}

fn vlans_of(d: &DevSummary) -> Vec<u16> {
    let mut set: Vec<u16> = Vec::new();
    for p in &d.ports {
        match p.mode {
            sim_core::device::PortMode::Access => {
                if !set.contains(&p.pvid) {
                    set.push(p.pvid);
                }
            }
            sim_core::device::PortMode::Trunk => {
                if let Some(list) = &p.trunk_allowed {
                    for v in list {
                        if !set.contains(v) {
                            set.push(*v);
                        }
                    }
                }
            }
        }
    }
    for s in &d.svis {
        if !set.contains(&s.vlan) {
            set.push(s.vlan);
        }
    }
    set.sort_unstable();
    set
}

fn port_lines_ios(d: &DevSummary, out: &mut Vec<String>) {
    let l3 = is_l3(d.kind);
    for p in &d.ports {
        out.push(format!(
            "interface {}",
            ifname::canonical(Dialect::Ios, d.kind, p.idx)
        ));
        if let Some(ip) = &p.ip {
            let mask = sim_packets::mask_from_prefix(p.prefix);
            out.push(format!(
                " ip address {} {}",
                ip,
                sim_packets::ipv4_str(mask)
            ));
        }
        if let Some(v6) = &p.ipv6 {
            out.push(format!(" ipv6 address {}/{}", v6, p.prefix6));
        }
        if !l3 {
            match p.mode {
                PortMode::Access => {
                    out.push(" switchport mode access".into());
                    out.push(format!(" switchport access vlan {}", p.pvid));
                }
                PortMode::Trunk => {
                    out.push(" switchport mode trunk".into());
                    if let Some(list) = &p.trunk_allowed {
                        let vs: Vec<String> = list.iter().map(|v| v.to_string()).collect();
                        out.push(format!(" switchport trunk allowed vlan {}", vs.join(",")));
                    }
                }
            }
        }
        if let Some(acl) = p.acl_in {
            out.push(format!(" ip access-group {} in", acl));
        }
        if let Some(acl) = p.acl_out {
            out.push(format!(" ip access-group {} out", acl));
        }
        if let Some(z) = &p.zone {
            out.push(format!(" zone-member security {}", z));
        }
        // 阶段 6A：ASA 血统的 `nameif` / `security-level` 不再渲染——命令已从树中
        // 移除，渲染出来会产生无法重执行的配置行（旧存档字段仅作兼容保留）。
        if let Some(g) = p.lag {
            out.push(format!(" channel-group {} mode active", g));
        }
        if !p.admin_up {
            out.push(" shutdown".into());
        }
        out.push(" exit".into());
    }
}

/// 非 IOS 接口段。VRP 与 Comware 的 L2 字面量必须分开渲染（阶段 4A）：
/// `port default vlan` / `allow-pass` 只属于 VRP，
/// `port access vlan` / `permit` 只属于 Comware（官方 E1/E2/E3/E10）。
fn port_lines_vrp(d: &DevSummary, dialect: Dialect, out: &mut Vec<String>) {
    let l3 = is_l3(d.kind);
    let comware = dialect == Dialect::Comware;
    for p in &d.ports {
        out.push(format!(
            "interface {}",
            ifname::canonical(dialect, d.kind, p.idx)
        ));
        if let Some(ip) = &p.ip {
            out.push(format!(" ip address {} {}", ip, p.prefix));
        }
        if let Some(v6) = &p.ipv6 {
            out.push(format!(" ipv6 address {}/{}", v6, p.prefix6));
        }
        if !l3 {
            match p.mode {
                PortMode::Access => {
                    out.push(" port link-type access".into());
                    if comware {
                        out.push(format!(" port access vlan {}", p.pvid));
                    } else {
                        out.push(format!(" port default vlan {}", p.pvid));
                    }
                }
                PortMode::Trunk => {
                    out.push(" port link-type trunk".into());
                    if let Some(list) = &p.trunk_allowed {
                        let vs: Vec<String> = list.iter().map(|v| v.to_string()).collect();
                        if comware {
                            out.push(format!(" port trunk permit vlan {}", vs.join(" ")));
                        } else {
                            out.push(format!(" port trunk allow-pass vlan {}", vs.join(" ")));
                        }
                    }
                }
            }
        }
        // 阶段 6A：安全区域成员关系不再塞进接口视图。
        // `firewall zone <name>` 是系统视图命令（V5），放在接口视图里无法重执行；
        // 区域块由 `security_zone_lines()` 在系统视图下独立渲染。
        if !p.admin_up {
            out.push(" shutdown".into());
        }
        out.push(" quit".into());
    }
}

/// 阶段 6A：安全区域与安全策略的可重执行渲染片段。
///
/// - VRP 基线：`firewall zone <name>` → `add interface <if>` → `quit`；
///   `security-policy` → `rule name <n>` → `source-zone` / `destination-zone` /
///   `action` /（禁用时）`disable` → `quit` → `quit`。
/// - IOS：`zone security <name>` → `exit`（成员关系在接口视图 `zone-member`）；
///   `zone-pair security <n>` → `source` / `destination` / `action` → `exit`。
/// - Comware：安全区域/策略仍属 Stage 4A 边界（安全数据面不在本包），
///   这里不渲染任何 Comware 安全配置，避免冒充未实现的能力。
fn security_lines(d: &DevSummary, dialect: Dialect, out: &mut Vec<String>) {
    match dialect {
        Dialect::Comware | Dialect::Tmos => (),
        Dialect::Ios => {
            for z in &d.security_zones {
                out.push(format!("zone security {}", z.name));
                out.push(" exit".into());
            }
            // 阶段 6B：ZBF class-map → policy-map → zone-pair，逐行可重执行。
            for cm in &d.zbf_class_maps {
                out.push(format!(
                    "class-map type inspect {} {}",
                    if cm.match_any {
                        "match-any"
                    } else {
                        "match-all"
                    },
                    cm.name
                ));
                for m in &cm.matches {
                    if let Some(p) = &m.protocol {
                        out.push(format!(" match protocol {p}"));
                    } else if let Some(n) = m.access_group {
                        out.push(format!(" match access-group {n}"));
                    }
                }
                out.push(" exit".into());
            }
            for pm in &d.zbf_policy_maps {
                out.push(format!("policy-map type inspect {}", pm.name));
                for cl in &pm.classes {
                    if cl.is_default {
                        out.push(" class class-default".into());
                    } else {
                        out.push(format!(" class type inspect {}", cl.class_name));
                    }
                    match &cl.action {
                        Some(sim_core::summary::ZbfActionSummary::Inspect { parameter_map }) => {
                            match parameter_map {
                                Some(p) => out.push(format!("  inspect {p}")),
                                None => out.push("  inspect".into()),
                            }
                        }
                        Some(sim_core::summary::ZbfActionSummary::Pass) => {
                            out.push("  pass".into())
                        }
                        Some(sim_core::summary::ZbfActionSummary::Drop { log }) => {
                            out.push(if *log {
                                "  drop log".into()
                            } else {
                                "  drop".into()
                            })
                        }
                        None => {}
                    }
                    out.push("  exit".into());
                }
                out.push(" exit".into());
            }
            for z in &d.zbf_zone_pairs {
                out.push(format!(
                    "zone-pair security {} source {} destination {}",
                    z.name, z.src_zone, z.dst_zone
                ));
                if let Some(p) = &z.service_policy {
                    out.push(format!(" service-policy type inspect {p}"));
                }
                out.push(" exit".into());
            }
        }
        Dialect::Vrp => {
            for z in &d.security_zones {
                out.push(format!("firewall zone {}", z.name));
                for p in d
                    .ports
                    .iter()
                    .filter(|p| p.zone.as_deref() == Some(&z.name))
                {
                    out.push(format!(
                        " add interface {}",
                        ifname::canonical(dialect, d.kind, p.idx)
                    ));
                }
                out.push(" quit".into());
            }
            if !d.security_policies.is_empty() {
                out.push("security-policy".into());
                for p in &d.security_policies {
                    out.push(format!(" rule name {}", p.name));
                    out.push(format!("  source-zone {}", p.src_zone));
                    out.push(format!("  destination-zone {}", p.dst_zone));
                    out.push(format!(
                        "  action {}",
                        if p.permit { "permit" } else { "deny" }
                    ));
                    if !p.enabled {
                        out.push("  disable".into());
                    }
                    out.push("  quit".into());
                }
                out.push(" quit".into());
            }
        }
    }
}

/// 阶段 5A：TMOS 17.x / tmsh 风格配置视图。
/// 每行都是 root 模块 `(tmos)#` 下可逐行重执行的官方完整路径命令（F1/F2/F3），
/// 对象一律用 `/Common/` 全路径（F6）。不是逐字厂商输出黄金。
pub fn render_tmos(d: &DevSummary) -> String {
    let mut out: Vec<String> = Vec::new();
    if let Some(ltm) = &d.ltm {
        for pool in &ltm.pools {
            out.push(format!("create ltm pool /Common/{}", pool.name));
            for (ip, port) in &pool.members {
                out.push(format!(
                    "modify ltm pool /Common/{} members add {{ {}:{} }}",
                    pool.name, ip, port
                ));
            }
        }
        for v in &ltm.virtuals {
            out.push(format!(
                "create ltm virtual /Common/{} destination {}:{} pool /Common/{}",
                v.name, v.vip, v.port, v.pool
            ));
        }
    }
    out.join("\n")
}

/// 生成 running/current 配置文本（按方言风格）。
/// 阶段 5A：TMOS 走独立 `render_tmos`，不复用任何华为/IOS 风格段落。
pub fn render(d: &DevSummary, dialect: Dialect) -> String {
    if dialect == Dialect::Tmos {
        return render_tmos(d);
    }
    let mut out: Vec<String> = Vec::new();
    match dialect {
        Dialect::Ios => out.push(format!("hostname {}", d.name)),
        _ => out.push(format!("sysname {}", d.name)),
    }
    // VLAN（从端口与 SVI 收集）。
    // 阶段 6A：只有具备二层交换端口的设备才渲染 VLAN 配置；路由器/防火墙的物理口
    // pvid 恒为缺省 VLAN 1，渲染 `vlan batch 1` 会得到不可重执行的配置行
    // （`vlan batch` 只在交换设备上有效）。
    let vlans = if matches!(d.kind, DeviceKind::Switch | DeviceKind::AccessPoint) {
        vlans_of(d)
    } else {
        Vec::new()
    };
    if !vlans.is_empty() {
        match dialect {
            Dialect::Ios => {
                for v in &vlans {
                    out.push(format!("vlan {}", v));
                }
            }
            Dialect::Vrp => {
                let vs: Vec<String> = vlans.iter().map(|v| v.to_string()).collect();
                out.push(format!("vlan batch {}", vs.join(" ")));
            }
            // E1/E3：Comware 官方配置文件每个 VLAN 一行 `vlan <id>`；
            // `vlan batch` 是 VRP 专属，绝不输出给 Comware。
            // E1/E11：`vlan <id>` 会进入 VLAN 视图，故每条后补 `quit` 退回系统视图，
            // 保证 `display current-configuration` 的输出可逐行重执行还原同一状态。
            Dialect::Comware => {
                for v in &vlans {
                    out.push(format!("vlan {}", v));
                    out.push(" quit".into());
                }
            }
            Dialect::Tmos => unreachable!("render_tmos 已提前返回"),
        }
    }
    // 接口
    match dialect {
        Dialect::Ios => port_lines_ios(d, &mut out),
        Dialect::Vrp | Dialect::Comware => port_lines_vrp(d, dialect, &mut out),
        Dialect::Tmos => unreachable!("render_tmos 已提前返回"),
    }
    // 阶段 6A：安全区域与安全策略（必须在接口段之后，区域成员引用接口名）。
    security_lines(d, dialect, &mut out);
    // SVI
    for s in &d.svis {
        match dialect {
            Dialect::Ios => {
                out.push(format!(
                    "interface {}",
                    ifname::svi_canonical(Dialect::Ios, s.vlan)
                ));
                let mask = sim_packets::ipv4_str(sim_packets::mask_from_prefix(s.prefix));
                out.push(format!(" ip address {} {}", s.ip, mask));
                if !s.admin_up {
                    out.push(" shutdown".into());
                }
                out.push(" exit".into());
            }
            _ => {
                out.push(format!(
                    "interface {}",
                    ifname::svi_canonical(dialect, s.vlan)
                ));
                out.push(format!(" ip address {} {}", s.ip, s.prefix));
                if !s.admin_up {
                    out.push(" shutdown".into());
                }
                out.push(" quit".into());
            }
        }
    }
    // 静态路由
    for r in &d.static_routes {
        match dialect {
            Dialect::Ios => {
                let mask = sim_packets::ipv4_str(sim_packets::mask_from_prefix(r.len));
                out.push(format!("ip route {} {} {}", r.dst, mask, r.nexthop));
            }
            _ => out.push(format!("ip route-static {} {} {}", r.dst, r.len, r.nexthop)),
        }
    }
    for r in &d.static_routes6 {
        match dialect {
            Dialect::Ios => out.push(format!("ipv6 route {}/{} {}", r.dst, r.len, r.nexthop)),
            _ => out.push(format!(
                "ipv6 route-static {} {} {}",
                r.dst, r.len, r.nexthop
            )),
        }
    }
    // OSPF（v4）：进程号与区域从真实状态取值，不再硬编码 1 / area 0。
    if !d.ospf_networks.is_empty() {
        let process = d.ospf_process.unwrap_or(1);
        let area = d.ospf_area;
        let area_ip = area_to_dotted(area);
        match dialect {
            Dialect::Ios => {
                out.push(format!("router ospf {}", process));
                for (net, wc) in &d.ospf_networks {
                    out.push(format!(" network {} {} area {}", net, wc, area));
                }
            }
            Dialect::Vrp => {
                out.push(format!("ospf {}", process));
                out.push(format!(" area {}", area_ip));
                for (net, wc) in &d.ospf_networks {
                    out.push(format!(" network {} {}", net, wc));
                }
                out.push(" quit".into());
            }
            // E6/E8：Comware 的 `network` 只在 OSPF 区域视图有效；quit 逐层退出，
            // 两次 quit 才能回到系统视图（区域 → 进程 → 系统），保证渲染文本可重执行。
            Dialect::Comware => {
                out.push(format!("ospf {}", process));
                out.push(format!(" area {}", area_ip));
                for (net, wc) in &d.ospf_networks {
                    out.push(format!(" network {} {}", net, wc));
                }
                out.push(" quit".into());
                out.push(" quit".into());
            }
            Dialect::Tmos => unreachable!("render_tmos 已提前返回"),
        }
    }
    // OSPFv3
    if d.ospfv3_enabled {
        match dialect {
            Dialect::Ios => out.push("ipv6 router ospf".into()),
            _ => out.push("ospfv3 1".into()),
        }
    }
    // Route-Policy / Route-Map：节点按 seq 升序渲染真实匹配与 apply 属性。
    for rp in &d.route_policies {
        let mut nodes: Vec<&sim_core::summary::RoutePolicyNodeSummary> = rp.nodes.iter().collect();
        nodes.sort_by_key(|n| n.seq);
        for n in nodes {
            let action = if n.permit { "permit" } else { "deny" };
            match dialect {
                Dialect::Ios => {
                    out.push(format!("route-map {} {} {}", rp.name, action, n.seq));
                    if let Some(acl) = n.match_acl {
                        out.push(format!(" match ip address {}", acl));
                    }
                    if let Some(lp) = n.set_local_pref {
                        out.push(format!(" set local-preference {}", lp));
                    }
                }
                _ => {
                    out.push(format!(
                        "route-policy {} {} node {}",
                        rp.name, action, n.seq
                    ));
                    if let Some(acl) = n.match_acl {
                        out.push(format!(" if-match acl {}", acl));
                    }
                    if let Some(lp) = n.set_local_pref {
                        out.push(format!(" apply local-preference {}", lp));
                    }
                    out.push(" quit".into());
                }
            }
        }
    }
    // BGP：本端 AS、邻居 AS/地址、network 与邻居策略均取真实状态，按方言用词。
    if let Some(asn) = d.bgp_asn {
        match dialect {
            Dialect::Ios => out.push(format!("router bgp {}", asn)),
            _ => out.push(format!("bgp {}", asn)),
        }
        for (ip, ra) in &d.bgp_neighbors {
            match dialect {
                Dialect::Ios => out.push(format!(" neighbor {} remote-as {}", ip, ra)),
                _ => out.push(format!(" peer {} as-number {}", ip, ra)),
            }
        }
        for (net, len) in &d.bgp_networks {
            let mask = sim_packets::ipv4_str(sim_packets::mask_from_prefix(*len));
            match dialect {
                Dialect::Ios => out.push(format!(" network {} mask {}", net, mask)),
                _ => out.push(format!(" network {} {}", net, mask)),
            }
        }
        for (ip, import, name) in &d.bgp_peer_policies {
            match dialect {
                Dialect::Ios => {
                    let dir = if *import { "in" } else { "out" };
                    out.push(format!(" neighbor {} route-map {} {}", ip, name, dir));
                }
                _ => {
                    let dir = if *import { "import" } else { "export" };
                    out.push(format!(" peer {} route-policy {} {}", ip, name, dir));
                }
            }
        }
        if !matches!(dialect, Dialect::Ios) {
            out.push(" quit".into());
        }
    }
    // DHCP pool：按实际方言渲染，VRP 不得套用 IOS 的 `ip dhcp pool`/`default-router`。
    for (low, high) in &d.dhcp_excluded {
        out.push(format!("ip dhcp excluded-address {} {}", low, high));
    }
    for pool in &d.dhcp_pools {
        let mask = sim_packets::ipv4_str(sim_packets::mask_from_prefix(pool.prefix));
        match dialect {
            Dialect::Ios => {
                out.push(format!("ip dhcp pool {}", pool.name));
                out.push(format!(" network {} {}", pool.network, mask));
                out.push(format!(" default-router {}", pool.gateway));
                if pool.dns != "0.0.0.0" {
                    out.push(format!(" dns-server {}", pool.dns));
                }
            }
            _ => {
                out.push(format!("ip pool {}", pool.name));
                out.push(format!(" network {} mask {}", pool.network, mask));
                out.push(format!(" gateway-list {}", pool.gateway));
                if pool.dns != "0.0.0.0" {
                    out.push(format!(" dns-list {}", pool.dns));
                }
            }
        }
    }
    for idx in &d.dhcp_server_ports {
        out.push(format!(
            "interface {}",
            ifname::canonical(dialect, d.kind, *idx)
        ));
        out.push(" dhcp select global".into());
        out.push(match dialect {
            Dialect::Ios => " exit".into(),
            _ => " quit".into(),
        });
    }
    // ACL：只渲染真实规则与端口，不再输出占位行。
    for acl in &d.acls {
        match dialect {
            Dialect::Ios => {
                for r in &acl.rules {
                    let action = if r.permit { "permit" } else { "deny" };
                    let proto = match r.proto.as_str() {
                        "icmp" => "icmp ",
                        "tcp" => "tcp ",
                        _ => "ip ",
                    };
                    let mut line = format!(
                        "access-list {} {} {}{} {}",
                        acl.number, action, proto, r.src, r.src_wildcard
                    );
                    if r.dst != "0.0.0.0" || r.dst_wildcard != "0.0.0.0" {
                        line.push_str(&format!(" {} {}", r.dst, r.dst_wildcard));
                    }
                    if let Some(p) = &r.src_port {
                        line.push_str(&format!(" eq {}", p));
                    }
                    if let Some(p) = &r.dst_port {
                        line.push_str(&format!(" eq {}", p));
                    }
                    out.push(line);
                }
            }
            _ => {
                out.push(format!("acl number {}", acl.number));
                for (i, r) in acl.rules.iter().enumerate() {
                    let action = if r.permit { "permit" } else { "deny" };
                    let proto = match r.proto.as_str() {
                        "icmp" => " icmp",
                        "tcp" => " tcp",
                        _ => "",
                    };
                    let mut line = format!(
                        " rule {} {}{} source {} {}",
                        5 + i as u32 * 5,
                        action,
                        proto,
                        r.src,
                        r.src_wildcard
                    );
                    if r.dst != "0.0.0.0" || r.dst_wildcard != "0.0.0.0" {
                        line.push_str(&format!(" destination {} {}", r.dst, r.dst_wildcard));
                    }
                    if let Some(p) = &r.dst_port {
                        line.push_str(&format!(" destination-port eq {}", p));
                    }
                    out.push(line);
                }
            }
        }
    }
    // NAT：接口名按真实方言/设备类型派生，角色与静态映射从真实状态渲染。
    for (inside, idx) in d
        .nat_inside
        .iter()
        .map(|i| (true, i))
        .chain(d.nat_outside.iter().map(|i| (false, i)))
    {
        match dialect {
            Dialect::Ios => {
                out.push(format!(
                    "interface {}",
                    ifname::canonical(Dialect::Ios, d.kind, *idx)
                ));
                out.push(if inside {
                    " ip nat inside".into()
                } else {
                    " ip nat outside".into()
                });
                out.push(" exit".into());
            }
            _ => {
                out.push(format!(
                    "interface {}",
                    ifname::canonical(dialect, d.kind, *idx)
                ));
                out.push(if inside {
                    " nat inbound".into()
                } else {
                    " nat outbound".into()
                });
                out.push(" quit".into());
            }
        }
    }
    for (inside, outside) in &d.nat_static {
        match dialect {
            Dialect::Ios => out.push(format!(
                "ip nat inside source static {} {}",
                inside, outside
            )),
            _ => out.push(format!("nat static global {} inside {}", outside, inside)),
        }
    }
    if let Some((acl, _port)) = d.nat_easyip {
        match dialect {
            Dialect::Ios => out.push(format!(
                "ip nat inside source list {} interface {} overload",
                acl,
                ifname::canonical(
                    Dialect::Ios,
                    d.kind,
                    d.nat_outside.first().copied().unwrap_or(0)
                )
            )),
            _ => out.push(format!("nat outbound {}", acl)),
        }
    }
    // mgmt：本地用户与 VTY（口令一律脱敏，不输出明文）
    if dialect == Dialect::Ios && d.mgmt.enable_secret.is_some() {
        out.push("enable secret ***".into());
    }
    if !d.mgmt.users.is_empty()
        || d.mgmt.auth_local
        || d.mgmt.effective_line_auth() != sim_core::device::LineAuthMode::None
    {
        match dialect {
            Dialect::Ios => {
                for u in &d.mgmt.users {
                    out.push(format!(
                        "username {} privilege {} secret ***",
                        u.name, u.privilege
                    ));
                }
                out.push("line vty 0 4".into());
                match d.mgmt.effective_line_auth() {
                    sim_core::device::LineAuthMode::None => {}
                    sim_core::device::LineAuthMode::Line => {
                        out.push(" login".into());
                        if d.mgmt.line_password.is_some() {
                            out.push(" password ***".into());
                        }
                    }
                    sim_core::device::LineAuthMode::Local => out.push(" login local".into()),
                }
                match d.mgmt.transport {
                    sim_core::device::VtyTransport::Ssh => out.push(" transport input ssh".into()),
                    sim_core::device::VtyTransport::Telnet => {
                        out.push(" transport input telnet".into())
                    }
                    sim_core::device::VtyTransport::All => out.push(" transport input all".into()),
                    sim_core::device::VtyTransport::None => {
                        out.push(" transport input none".into())
                    }
                }
            }
            _ => {
                for u in &d.mgmt.users {
                    out.push(format!("local-user {}", u.name));
                }
                if d.mgmt.auth_local {
                    out.push("authentication-mode aaa".into());
                }
                match d.mgmt.transport {
                    sim_core::device::VtyTransport::Ssh => out.push("protocol inbound ssh".into()),
                    sim_core::device::VtyTransport::Telnet => {
                        out.push("protocol inbound telnet".into())
                    }
                    sim_core::device::VtyTransport::All => out.push("protocol inbound all".into()),
                    sim_core::device::VtyTransport::None => {
                        out.push("protocol inbound none".into())
                    }
                }
            }
        }
    }
    // VRRP：真实接口名 + 真实优先级/抢占，不再输出占位拼接行。
    for g in &d.vrrp {
        match dialect {
            Dialect::Ios => {
                out.push(format!(
                    "interface {}",
                    ifname::canonical(Dialect::Ios, d.kind, g.port)
                ));
                out.push(format!(" vrrp {} ip {}", g.vrid, g.vip));
                out.push(format!(" vrrp {} priority {}", g.vrid, g.priority));
                if g.preempt {
                    out.push(format!(" vrrp {} preempt", g.vrid));
                }
                out.push(" exit".into());
            }
            _ => {
                out.push(format!(
                    "interface {}",
                    ifname::canonical(dialect, d.kind, g.port)
                ));
                out.push(format!(" vrrp vrid {} virtual-ip {}", g.vrid, g.vip));
                out.push(format!(" vrrp vrid {} priority {}", g.vrid, g.priority));
                if g.preempt {
                    out.push(format!(" vrrp vrid {} preempt-mode timer delay 0", g.vrid));
                }
                out.push(" quit".into());
            }
        }
    }
    // STP（H2）与 Eth-Trunk（H3）：从真实配置渲染。
    if let Some(priority) = d.stp_priority {
        if !d.stp_enabled {
            out.push(match dialect {
                Dialect::Ios => "no spanning-tree vlan 1".into(),
                _ => "stp disable".into(),
            });
        } else {
            out.push(match dialect {
                Dialect::Ios => format!("spanning-tree vlan 1 priority {}", priority),
                _ => format!("stp priority {}", priority),
            });
        }
    }
    for g in &d.lags {
        match dialect {
            Dialect::Ios => {
                out.push(format!("interface Port-channel{}", g.id));
                out.push(if matches!(g.mode, sim_core::device::LagMode::Static) {
                    " channel-group mode on".into()
                } else {
                    " channel-group mode active".into()
                });
                out.push(" exit".into());
            }
            Dialect::Vrp => {
                out.push(format!("interface Eth-Trunk{}", g.id));
                out.push(match g.mode {
                    sim_core::device::LagMode::Static => " mode manual load-balance".into(),
                    _ => " mode lacp-static".into(),
                });
                out.push(" quit".into());
            }
            // E1/E3：Comware 聚合口是 Bridge-Aggregation，模式为 link-aggregation mode。
            Dialect::Comware => {
                out.push(format!("interface Bridge-Aggregation{}", g.id));
                out.push(match g.mode {
                    sim_core::device::LagMode::Static => " link-aggregation mode static".into(),
                    _ => " link-aggregation mode dynamic".into(),
                });
                out.push(" quit".into());
            }
            Dialect::Tmos => unreachable!("render_tmos 已提前返回"),
        }
    }
    // VXLAN / EVPN
    if let Some(vx) = &d.vxlan {
        if let Some(local) = &vx.local {
            out.push(format!("vxlan local {}", local));
        }
        for (vni, vlan) in &vx.mappings {
            out.push(format!("vxlan vni {} vlan {}", vni, vlan));
        }
        for peer in &vx.peers {
            out.push(format!("vxlan peer {}", peer));
        }
    }
    if let Some(ev) = &d.evpn {
        if ev.enabled {
            out.push("evpn enable".into());
        }
        for peer in &ev.peers {
            out.push(format!("evpn peer {}", peer));
        }
    }
    // 阶段 5A：抽象 `ltm pool ...` / `ltm virtual ...` 已从 IOS/VRP/Comware 三棵树
    // 移除，这里不再为任何非 Tmos 设备渲染这些不可重放的跨厂商伪装命令；
    // TMOS 由 `render_tmos` 在 render() 开头独立输出 tmsh 风格文本。
    // M-LAG / 堆叠
    if let Some(m) = &d.mlag {
        out.push(format!("mlag domain {}", m.domain));
        for p in &m.peer_links {
            out.push(format!(
                "interface GigabitEthernet0/0/{} mlag peer-link",
                p + 1
            ));
        }
    }
    if let Some(st) = &d.stack {
        out.push(format!("stack domain {}", st.domain));
        out.push(format!("stack member {}", st.member));
        out.push(format!("stack priority {}", st.priority));
    }
    // IS-IS / SRv6
    if let Some(net) = &d.isis_net {
        out.push(format!("network {}", net));
    }
    if let Some((loc, len)) = &d.srv6_locator {
        out.push(format!("srv6 locator {}/{}", loc, len));
    }
    for sid in &d.srv6_sids {
        out.push(format!("srv6 sid {}", sid));
    }
    for r in &d.srv6_routes {
        out.push(format!("srv6 route {}/{} sid {}", r.dst, r.len, r.sid));
    }
    // 阶段 6A：这里原本输出的 `security-policy rule <n> <src> -> <dst> permit=` 是
    // 不可重执行的伪命令（三方言共用），已由 `security_lines()` 的基线分叉渲染替代。
    out.join("\n")
}
