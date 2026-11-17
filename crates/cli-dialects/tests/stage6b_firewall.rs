//! 阶段 6B：Cisco IOS XE 17.x ZBF 完整数据面、子模式提示符与主方案 §14.3 验收场景。
//!
//! 证据：`CISCO_ZBF_REF`（官方《Security and VPN Configuration Guide, Cisco IOS XE 17.x》
//! Zone-Based Policy Firewalls 全文核验，2026-11-16）。无带版本设备样本，全部为
//! `DocumentationOnly`：只做状态、结构、提示符与数据面断言，不建立逐字输出黄金。
//!
//! 边界（如实登记）：
//! - `match protocol` 只承载 ICMP / TCP 与常见 TCP 服务端口的 L4 匹配；UDP 与 L7/ALG 不命中；
//! - `parameter-map`、层次化 policy-map、`match class-map`、`platform inspect ...` 为未模拟；
//! - `self` 区域：本模拟器的设备自发流量不经过转发路径，因此天然满足官方例外。

use cli_dialects::{exec_and_run, inspect_command, prompt, specs, trees};
use cli_engine::spec::{Effect, SemanticSupport};
use cli_engine::{parse, Mode, Session};
use sim_core::device::{port_mac, DeviceKind, DeviceState, Dialect};
use sim_core::engine::{Engine, PING_HORIZON_NS};
use sim_core::profile::EvidenceLevel;
use sim_core::Net;
use sim_packets::{
    ipv4_from_str, EthernetFrame, IcmpPacket, Ipv4Packet, TcpPacket, ETHERTYPE_IPV4, L3, TCP_ACK,
};

fn ip(s: &str) -> u32 {
    ipv4_from_str(s).unwrap()
}

// ---------------------------------------------------------------- 拓扑与工具

struct Topo {
    eng: Engine,
    pc: u32,
    r1: u32,
    srv: u32,
}

/// PC1 — R1(IOS Router) — SRV。ZBF 的官方主线设备是路由器。
fn topo() -> Topo {
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Ios);
    let srv = net.add_device("SRV".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc, r1).unwrap(); // R1 p0
    net.add_link(r1, srv).unwrap(); // R1 p1
    net.set_host(pc, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    net.set_host(srv, Some(ip("203.0.113.10")), 24, Some(ip("203.0.113.1")));
    Topo {
        eng: Engine::new(net),
        pc,
        r1,
        srv,
    }
}

fn run(t: &mut Topo, s: &mut Session, lines: &[&str]) {
    for line in lines {
        let out = exec_and_run(&mut t.eng, s, t.r1, line);
        assert!(
            !out.contains("Error:") && !out.contains("% ") && !out.contains("Wrong parameter"),
            "{}: {}",
            line,
            out
        );
    }
}

/// 从用户视图执行一段配置（重置会话模式，确保与调用方残留模式无关）。
fn cfg(t: &mut Topo, s: &mut Session, lines: &[&str]) {
    *s = Session::default();
    run(t, s, &["enable", "configure terminal"]);
    run(t, s, lines);
}

/// 只读查询（每次从用户视图重新开始）。
fn sh(t: &mut Topo, s: &mut Session, line: &str) -> String {
    *s = Session::default();
    exec_and_run(&mut t.eng, s, t.r1, line)
}

/// 接口编址 + 区域成员（不配置任何 zone-pair）。
fn base_zones(t: &mut Topo, s: &mut Session) {
    cfg(
        t,
        s,
        &[
            "interface g0/0",
            "ip address 192.168.1.1 255.255.255.0",
            "no shutdown",
            "zone-member security INSIDE",
            "exit",
            "interface g0/1",
            "ip address 203.0.113.1 255.255.255.0",
            "no shutdown",
            "zone-member security OUTSIDE",
            "exit",
        ],
    );
}

/// 标准 ZBF 对象：class-map ICMP/TCP → policy-map PMAP → zone-pair IN-OUT。
fn zbf_inspect(t: &mut Topo, s: &mut Session) {
    cfg(
        t,
        s,
        &[
            "class-map type inspect match-any TRAFFIC",
            "match protocol icmp",
            "match protocol tcp",
            "exit",
            "policy-map type inspect PMAP",
            "class type inspect TRAFFIC",
            "inspect",
            "exit",
            "exit",
            "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
            "service-policy type inspect PMAP",
            "exit",
        ],
    );
}

fn received_of(eng: &Engine, from: u32) -> u32 {
    match &eng.net.device(from).unwrap().state {
        DeviceState::Host(h) => h.pings.last().map(|p| p.received()).unwrap_or(0),
        _ => 0,
    }
}

fn ping_pc(t: &mut Topo, target: &str) -> u32 {
    t.eng.start_ping(t.pc, ip(target));
    t.eng.run_to_quiescence(PING_HORIZON_NS);
    received_of(&t.eng, t.pc)
}

fn ping_srv(t: &mut Topo, target: &str) -> u32 {
    t.eng.start_ping(t.srv, ip(target));
    t.eng.run_to_quiescence(PING_HORIZON_NS);
    received_of(&t.eng, t.srv)
}

fn icmp_sessions(t: &Topo) -> usize {
    match &t.eng.net.device(t.r1).unwrap().state {
        DeviceState::Router(rs) => rs.security_sessions.len(),
        _ => 0,
    }
}

fn tcp_flows(t: &Topo) -> usize {
    match &t.eng.net.device(t.r1).unwrap().state {
        DeviceState::Router(rs) => rs.tcp_flows.len(),
        _ => 0,
    }
}

/// 注入原始 ICMP（构造「既有会话」与「非会话回包」）。
fn inject_icmp(t: &mut Topo, from_pc: bool, src: &str, dst: &str, id: u16, is_reply: bool) {
    let (dev, port, dst_dev, dst_port) = if from_pc {
        (t.pc, 0, t.r1, 0)
    } else {
        (t.srv, 0, t.r1, 1)
    };
    let frame = EthernetFrame {
        dst: port_mac(dst_dev, dst_port),
        src: port_mac(dev, port),
        vlan: None,
        ethertype: ETHERTYPE_IPV4,
        payload: L3::Ipv4(Ipv4Packet {
            src: ip(src),
            dst: ip(dst),
            ttl: 64,
            icmp: IcmpPacket {
                id,
                seq: 1,
                is_reply,
            },
        }),
    };
    t.eng.transmit_frame(dev, port, frame);
    t.eng.run_to_quiescence(PING_HORIZON_NS);
}

fn inject_tcp(t: &mut Topo, src: &str, dst: &str, sport: u16, dport: u16, flags: u8) {
    let frame = EthernetFrame {
        dst: port_mac(t.r1, 0),
        src: port_mac(t.pc, 0),
        vlan: None,
        ethertype: ETHERTYPE_IPV4,
        payload: L3::Tcp(TcpPacket {
            src: ip(src),
            dst: ip(dst),
            ttl: 64,
            src_port: sport,
            dst_port: dport,
            flags,
            seq: 1,
            ack: 0,
        }),
    };
    t.eng.transmit_frame(t.pc, 0, frame);
    t.eng.run_to_quiescence(PING_HORIZON_NS);
}

// ---------------------------------------------------------------- 规格与 Profile

#[test]
fn stage6b_zbf_specs_are_implemented_and_lint_is_clean() {
    let report = specs::lint_report(Dialect::Ios);
    assert!(report.duplicates.is_empty(), "{report:?}");
    assert!(report.dangling.is_empty(), "{report:?}");
    assert!(report.drift.is_empty(), "{report:?}");
    assert!(report.strict_ok(), "{report:?}");

    let five = [
        "config/class-map/type/inspect/match-any/@0word",
        "config/policy-map/type/inspect/@0word",
        "config/zone-pair/security/@0word/source/@0word/destination/@0word",
        "security_rule/service-policy/type/inspect/@0word",
        "exec/show/policy-map/type/inspect/zone-pair",
    ];
    for id in five {
        let r = specs::find(Dialect::Ios, id).unwrap_or_else(|| panic!("缺少规格 {id}"));
        assert_eq!(r.semantics, SemanticSupport::Implemented, "{id}");
        assert_eq!(r.evidence, EvidenceLevel::DocumentationOnly, "{id}");
        assert!(r.strict, "{id}");
        assert!(
            r.references.iter().any(|x| x.contains("cisco.com")),
            "{id} 必须引用 17.x 官方正文"
        );
    }
    // O-1：`[zone-pair-name] [sessions]` 组合形式与复数别名。
    for id in [
        "exec/show/policy-map/type/inspect/zone-pair/@0word/session",
        "exec/show/policy-map/type/inspect/zone-pair/@0word/sessions",
        "exec/show/policy-map/type/inspect/zone-pair/sessions",
    ] {
        let r = specs::find(Dialect::Ios, id).unwrap_or_else(|| panic!("缺少规格 {id}"));
        assert_eq!(r.semantics, SemanticSupport::Implemented, "{id}");
        assert!(r.strict, "{id}");
        assert!(r.references.iter().any(|x| x.contains("cisco.com")), "{id}");
    }
    // 未模拟邻接命令必须显式登记，不冒充已实现。
    for (id, effect) in [
        (
            "config/parameter-map/type/inspect/@0word",
            Effect::Unimplemented,
        ),
        ("class_map/match/class-map/@0word", Effect::Unimplemented),
        (
            "security_rule/platform/inspect/match-statistics/per-filter",
            Effect::Unimplemented,
        ),
    ] {
        let r = specs::find(Dialect::Ios, id).unwrap_or_else(|| panic!("缺少规格 {id}"));
        assert_eq!(r.semantics, SemanticSupport::NotSimulated, "{id}");
        assert_eq!(r.effect, effect, "{id}");
    }
    // Profile 能力：IOS Router 明确具备区域化防火墙能力。
    let p = sim_core::profile::profile_for(Dialect::Ios, DeviceKind::Router);
    assert_eq!(
        p.capability(sim_core::profile::Capability::ZoneFirewall),
        sim_core::profile::CapabilityVerdict::Supported
    );
}

// ---------------------------------------------------------------- 子模式与提示符

#[test]
fn stage6b_submode_prompts_and_exit_hierarchy_match_official() {
    let mut t = topo();
    let mut s = Session::default();
    run(&mut t, &mut s, &["enable", "configure terminal"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config)#");

    run(&mut t, &mut s, &["zone security INSIDE"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config-sec-zone)#");
    run(&mut t, &mut s, &["exit", "zone security OUTSIDE"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config-sec-zone)#");
    run(&mut t, &mut s, &["exit"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config)#");

    run(&mut t, &mut s, &["class-map type inspect match-any WEB"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config-cmap)#");
    run(&mut t, &mut s, &["match protocol icmp", "exit"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config)#");

    run(&mut t, &mut s, &["policy-map type inspect PMAP"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config-pmap)#");
    run(&mut t, &mut s, &["class type inspect WEB"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config-pmap-c)#");
    run(&mut t, &mut s, &["inspect", "exit"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config-pmap)#");
    run(&mut t, &mut s, &["exit"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config)#");

    run(
        &mut t,
        &mut s,
        &["zone-pair security IN-OUT source INSIDE destination OUTSIDE"],
    );
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config-sec-zone-pair)#");
    run(&mut t, &mut s, &["service-policy type inspect PMAP"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1(config-sec-zone-pair)#");
    run(&mut t, &mut s, &["end"]);
    assert_eq!(prompt(&t.eng, &s, t.r1), "R1#");
}

#[test]
fn stage6b_zbf_dialect_matrix_is_bidirectional() {
    // IOS 接受完整 ZBF 语法。
    for (mode, line) in [
        (Mode::Config, "class-map type inspect match-any WEB"),
        (Mode::ClassMap, "match protocol icmp"),
        (Mode::Config, "policy-map type inspect PMAP"),
        (Mode::PolicyMap, "class type inspect WEB"),
        (Mode::PolicyMapClass, "inspect"),
        (
            Mode::Config,
            "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
        ),
        (Mode::SecurityRule, "service-policy type inspect PMAP"),
    ] {
        assert!(
            matches!(parse(trees(Dialect::Ios).root_for(mode), line), Ok(Some(_))),
            "IOS 必须接受 {line} @ {mode:?}"
        );
    }
    // VRP / Comware 不得接受 IOS ZBF 语法（命令错误，不是「仅识别」）。
    for (d, profile) in [
        (Dialect::Vrp, sim_core::profile::VRP_ROUTER_PROFILE),
        (Dialect::Comware, sim_core::profile::COMWARE_ROUTER_PROFILE),
    ] {
        for (mode, line) in [
            (Mode::Config, "class-map type inspect match-any WEB"),
            (Mode::Config, "policy-map type inspect PMAP"),
            (
                Mode::Config,
                "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
            ),
        ] {
            assert!(
                !matches!(parse(trees(d).root_for(mode), line), Ok(Some(_))),
                "{d:?} 不得接受 IOS ZBF 命令 {line}"
            );
            assert_eq!(
                inspect_command(profile, mode, line).support.label(),
                "invalid-command",
                "{d:?} / {line}"
            );
        }
    }
    // 反向：VRP 的 security-policy 不得混入 IOS。
    assert!(!matches!(
        parse(
            trees(Dialect::Ios).root_for(Mode::Config),
            "security-policy"
        ),
        Ok(Some(_))
    ));
}

// ---------------------------------------------------------------- §14.3 场景

/// §14.3-1/2：无 zone-pair 缺省拒绝；同区域缺省放行；区域↔非区域始终丢弃。
#[test]
fn stage6b_zone_defaults_and_same_zone_boundaries() {
    let mut t = topo();
    let mut s = Session::default();
    base_zones(&mut t, &mut s);

    assert_eq!(
        ping_srv(&mut t, "192.168.1.10"),
        0,
        "无 zone-pair 外→内拒绝"
    );
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 0, "无 zone-pair 内→外拒绝");
    assert_eq!(icmp_sessions(&t), 0, "被拒绝流量不得建会话");

    // 同区域：官方缺省放行（无状态，不建会话）。
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Ios);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc, r1).unwrap();
    net.add_link(r1, pc2).unwrap();
    net.set_host(pc, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    net.set_host(pc2, Some(ip("192.168.2.10")), 24, Some(ip("192.168.2.1")));
    let mut t2 = Topo {
        eng: Engine::new(net),
        pc,
        r1,
        srv: pc2,
    };
    cfg(
        &mut t2,
        &mut s,
        &[
            "interface g0/0",
            "ip address 192.168.1.1 255.255.255.0",
            "no shutdown",
            "zone-member security SAME",
            "exit",
            "interface g0/1",
            "ip address 192.168.2.1 255.255.255.0",
            "no shutdown",
            "zone-member security SAME",
            "exit",
        ],
    );
    assert_eq!(
        ping_pc(&mut t2, "192.168.2.10"),
        5,
        "同区域缺省放行（官方）"
    );
    assert_eq!(icmp_sessions(&t2), 0, "同区域缺省放行不建会话");

    // 同区域显式 zone-pair drop 后，新流被丢弃。
    cfg(
        &mut t2,
        &mut s,
        &[
            "policy-map type inspect DROPALL",
            "class class-default",
            "drop",
            "exit",
            "exit",
            "zone-pair security SAME-TO-SAME source SAME destination SAME",
            "service-policy type inspect DROPALL",
            "exit",
        ],
    );
    assert_eq!(ping_pc(&mut t2, "192.168.2.10"), 0, "同区域显式 drop 生效");

    // 区域↔非区域：移除一侧成员后，所有跨边界流量丢弃。
    let mut t3 = topo();
    cfg(
        &mut t3,
        &mut s,
        &[
            "interface g0/0",
            "ip address 192.168.1.1 255.255.255.0",
            "no shutdown",
            "zone-member security INSIDE",
            "exit",
            "interface g0/1",
            "ip address 203.0.113.1 255.255.255.0",
            "no shutdown",
            "exit",
        ],
    );
    assert_eq!(ping_pc(&mut t3, "203.0.113.10"), 0, "区域→非区域始终丢弃");
    assert_eq!(ping_srv(&mut t3, "192.168.1.10"), 0, "非区域→区域始终丢弃");
    // 两侧都未划区域：ZBF 不参与。
    cfg(
        &mut t3,
        &mut s,
        &["interface g0/0", "no zone-member security INSIDE", "exit"],
    );
    assert_eq!(ping_pc(&mut t3, "203.0.113.10"), 5, "两侧未划区域直通");
}

/// §14.3-3/4：inspect 正向 + 回包走会话表；非会话回包/未授权新会话拒绝。
#[test]
fn stage6b_inspect_is_stateful_and_rejects_non_session_replies() {
    let mut t = topo();
    let mut s = Session::default();
    base_zones(&mut t, &mut s);
    zbf_inspect(&mut t, &mut s);

    // (a) 无会话的 echo reply 被丢弃。
    let before = t.eng.dropped;
    inject_icmp(&mut t, false, "203.0.113.10", "192.168.1.10", 4242, true);
    assert!(t.eng.dropped > before, "非会话回包必须丢弃");
    assert_eq!(icmp_sessions(&t), 0, "回包不得建会话");

    // (b) 未授权新会话（反向 echo request）拒绝。
    let before = t.eng.dropped;
    inject_icmp(&mut t, false, "203.0.113.10", "192.168.1.10", 4243, false);
    assert!(t.eng.dropped > before, "反向新会话必须拒绝");

    // (c) 反向先探测：没有会话时拒绝；再验证 inspect 正向 5/5 且只建一条会话。
    assert_eq!(ping_srv(&mut t, "192.168.1.10"), 0, "反向无策略拒绝");
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5, "inspect 正向放行");
    assert_eq!(icmp_sessions(&t), 1, "一条 ICMP 会话支撑 5 个回包");

    // (d) 非 SYN 且无会话的 TCP 被丢弃。
    let before = t.eng.dropped;
    inject_tcp(&mut t, "192.168.1.10", "203.0.113.10", 40000, 80, TCP_ACK);
    assert!(t.eng.dropped > before, "非会话 TCP 必须丢弃");
    assert_eq!(tcp_flows(&t), 0);

    // (e) TCP SYN 命中 inspect 建流，回包不需反向策略。
    let mut pcs = Session::default();
    let out = exec_and_run(&mut t.eng, &mut pcs, t.pc, "tcp 203.0.113.10 80");
    assert!(out.contains("连接成功"), "{out}");
    assert_eq!(tcp_flows(&t), 1, "一条 TCP inspect 会话");

    // (f) 查询结构：zone-pair、policy-map 与命中/会话计数。
    let pairs = sh(&mut t, &mut s, "show zone-pair security");
    assert!(
        pairs.contains("IN-OUT") && pairs.contains("PMAP"),
        "{pairs}"
    );
    let pm = sh(&mut t, &mut s, "show policy-map type inspect zone-pair");
    assert!(pm.contains("Service-policy inspect : PMAP"), "{pm}");
    assert!(pm.contains("Class-map: TRAFFIC (match-any)"), "{pm}");
    assert!(pm.contains("Match: protocol icmp"), "{pm}");
    assert!(pm.contains("inspect"), "{pm}");
    assert!(pm.contains("packets"), "{pm}");
    let sess = sh(
        &mut t,
        &mut s,
        "show policy-map type inspect zone-pair session",
    );
    assert!(sess.contains("sessions:"), "{sess}");
    // O-1：官方设计指南语法 `[zone-pair-name] [sessions]` 是可组合形式。
    for line in [
        "show policy-map type inspect zone-pair IN-OUT session",
        "show policy-map type inspect zone-pair IN-OUT sessions",
        "show policy-map type inspect zone-pair sessions",
    ] {
        let out = sh(&mut t, &mut s, line);
        assert!(
            out.contains("Zone-pair: IN-OUT") && out.contains("sessions:"),
            "{line} 组合/复数形式必须可用: {out}"
        );
    }
}

/// §14.3-4b：pass 无状态 —— 不建会话，反向必须有镜像策略。
#[test]
fn stage6b_pass_is_stateless_and_needs_mirrored_policy() {
    let mut t = topo();
    let mut s = Session::default();
    base_zones(&mut t, &mut s);
    cfg(
        &mut t,
        &mut s,
        &[
            "class-map type inspect match-any ICMP",
            "match protocol icmp",
            "exit",
            "policy-map type inspect PASSMAP",
            "class type inspect ICMP",
            "pass",
            "exit",
            "exit",
            "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
            "service-policy type inspect PASSMAP",
            "exit",
        ],
    );
    // 只有正向 pass：请求放行，但回包在反向无策略 → 丢弃；全程无会话。
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 0, "无镜像 pass 时回包丢弃");
    assert_eq!(icmp_sessions(&t), 0, "pass 不得建立会话");

    // 追加反向 pass 后双向放行，仍无会话。
    cfg(
        &mut t,
        &mut s,
        &[
            "zone-pair security OUT-IN source OUTSIDE destination INSIDE",
            "service-policy type inspect PASSMAP",
            "exit",
        ],
    );
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5, "镜像 pass 后回包放行");
    assert_eq!(icmp_sessions(&t), 0, "pass 始终无状态");
}

/// §14.3-5/6：策略删除/改序只影响新流；既有 inspect 会话不被重新分类。
#[test]
fn stage6b_policy_change_affects_new_flows_only() {
    let mut t = topo();
    let mut s = Session::default();
    base_zones(&mut t, &mut s);
    cfg(
        &mut t,
        &mut s,
        &[
            "class-map type inspect match-any ICMP",
            "match protocol icmp",
            "exit",
            "policy-map type inspect PMAP",
            "class type inspect ICMP",
            "inspect",
            "exit",
            "exit",
            "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
            "service-policy type inspect PMAP",
            "exit",
        ],
    );
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5);
    let (sess_id, src) = match &t.eng.net.device(t.r1).unwrap().state {
        DeviceState::Router(rs) => (rs.security_sessions[0].icmp_id, rs.security_sessions[0].src),
        _ => panic!("缺少会话"),
    };
    assert_eq!(src, ip("192.168.1.10"));

    // 通过删除 + 重新添加，把 drop 类排到 inspect 类之前（IOS 无 rule move；
    // ZBF 的等价物是 class 在 policy-map 中的先后顺序）。
    cfg(
        &mut t,
        &mut s,
        &[
            "class-map type inspect match-any BLOCK",
            "match protocol icmp",
            "exit",
            "policy-map type inspect PMAP",
            "no class type inspect ICMP",
            "class type inspect BLOCK",
            "drop",
            "exit",
            "class type inspect ICMP",
            "inspect",
            "exit",
            "exit",
        ],
    );
    let order = match &t.eng.net.device(t.r1).unwrap().state {
        DeviceState::Router(rs) => rs.zbf_policy_maps[0]
            .classes
            .iter()
            .map(|c| c.class_name.clone())
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    assert_eq!(
        order,
        vec!["BLOCK".to_string(), "ICMP".to_string()],
        "drop 类必须排在 inspect 类之前"
    );

    // 新流（不同 echo id）被首条 drop 拒绝。
    let before = t.eng.dropped;
    inject_icmp(
        &mut t,
        true,
        "192.168.1.10",
        "203.0.113.10",
        sess_id + 1,
        false,
    );
    assert!(t.eng.dropped > before, "新流命中首条 drop");

    // 既有会话不被重新分类（官方：连接建立后报文不再分类）。
    let before = t.eng.dropped;
    inject_icmp(&mut t, true, "192.168.1.10", "203.0.113.10", sess_id, false);
    assert_eq!(t.eng.dropped, before, "既有会话继续转发");

    // 删除首条 drop 类后，新流恢复命中 inspect。
    cfg(
        &mut t,
        &mut s,
        &[
            "policy-map type inspect PMAP",
            "no class type inspect BLOCK",
            "exit",
        ],
    );
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5, "删除 drop 类后恢复");
}

/// §14.3-7：show 与真实状态一致；配置视图逐行重执行还原。
#[test]
fn stage6b_show_matches_state_and_config_view_is_replayable() {
    let mut t = topo();
    let mut s = Session::default();
    base_zones(&mut t, &mut s);
    zbf_inspect(&mut t, &mut s);

    let zones = sh(&mut t, &mut s, "show zone security");
    assert!(
        zones.contains("INSIDE") && zones.contains("OUTSIDE"),
        "{zones}"
    );
    let pairs = sh(&mut t, &mut s, "show zone-pair security");
    assert!(
        pairs.contains("IN-OUT") && pairs.contains("PMAP"),
        "{pairs}"
    );

    // 删除策略挂接后 show 立即反映。
    cfg(
        &mut t,
        &mut s,
        &[
            "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
            "no service-policy type inspect PMAP",
            "exit",
        ],
    );
    let pairs = sh(&mut t, &mut s, "show zone-pair security");
    assert!(pairs.contains("<none>"), "{pairs}");
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 0, "未挂策略即丢弃");

    // 重新挂接并确认配置视图可逐行重执行。
    cfg(
        &mut t,
        &mut s,
        &[
            "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
            "service-policy type inspect PMAP",
            "exit",
        ],
    );
    let rendered = sh(&mut t, &mut s, "show running-config");
    for needle in [
        "zone security INSIDE",
        "class-map type inspect match-any TRAFFIC",
        "match protocol icmp",
        "policy-map type inspect PMAP",
        "class type inspect TRAFFIC",
        " inspect",
        "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
        "service-policy type inspect PMAP",
    ] {
        assert!(
            rendered.contains(needle),
            "配置视图缺少 {needle}: {rendered}"
        );
    }
    assert!(!rendered.contains("->"), "不得残留伪命令: {rendered}");

    let mut t2 = topo();
    let mut s2 = Session::default();
    exec_and_run(&mut t2.eng, &mut s2, t2.r1, "enable");
    exec_and_run(&mut t2.eng, &mut s2, t2.r1, "configure terminal");
    for line in rendered.lines() {
        let out = exec_and_run(&mut t2.eng, &mut s2, t2.r1, line.trim());
        assert!(
            !out.contains("%") && !out.contains("Error:"),
            "渲染行不可重执行: {line} => {out}"
        );
    }
    // 回到用户视图比较三组查询（命中计数先不参与比较）。
    let mut q1 = Session::default();
    let mut q2 = Session::default();
    for line in ["show zone security", "show zone-pair security"] {
        assert_eq!(
            exec_and_run(&mut t2.eng, &mut q2, t2.r1, line),
            exec_and_run(&mut t.eng, &mut q1, t.r1, line),
            "重执行后 {line} 必须一致"
        );
    }
}

/// 存档往返：class-map / policy-map / zone-pair 与动作完整保留。
#[test]
fn stage6b_summary_round_trip_keeps_zbf_objects() {
    let mut t = topo();
    let mut s = Session::default();
    base_zones(&mut t, &mut s);
    cfg(
        &mut t,
        &mut s,
        &[
            "class-map type inspect match-all BOTH",
            "match protocol icmp",
            "match access-group 100",
            "exit",
            "policy-map type inspect PMAP",
            "class type inspect BOTH",
            "pass",
            "exit",
            "class class-default",
            "drop log",
            "exit",
            "exit",
            "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
            "service-policy type inspect PMAP",
            "exit",
        ],
    );
    let sum = t.eng.net.summary();
    let mut net2 = Net::new();
    net2.apply_summary(&sum).unwrap();
    let mut eng2 = Engine::new(net2);
    let rs = match &eng2.net.device(t.r1).unwrap().state {
        DeviceState::Router(rs) => rs,
        _ => panic!(),
    };
    assert_eq!(rs.zbf_class_maps.len(), 1);
    assert_eq!(rs.zbf_class_maps[0].matches.len(), 2);
    assert_eq!(
        rs.zbf_class_maps[0].match_type,
        sim_core::device::ZbfMatchType::All
    );
    assert_eq!(rs.zbf_policy_maps.len(), 1);
    assert_eq!(rs.zbf_policy_maps[0].classes.len(), 2);
    assert_eq!(
        rs.zbf_policy_maps[0].classes[0].action,
        Some(sim_core::device::ZbfAction::Pass)
    );
    assert_eq!(
        rs.zbf_policy_maps[0].classes[1].action,
        Some(sim_core::device::ZbfAction::Drop { log: true })
    );
    assert_eq!(rs.zbf_zone_pairs.len(), 1);
    assert_eq!(rs.zbf_zone_pairs[0].service_policy.as_deref(), Some("PMAP"));
    // 重载后数据面仍按同一策略工作：pass 正向放行，无反向策略时回包丢弃。
    eng2.start_ping(t.pc, ip("203.0.113.10"));
    eng2.run_to_quiescence(PING_HORIZON_NS);
    assert_eq!(received_of(&eng2, t.pc), 0, "pass 无镜像策略时回包丢弃");
}

// ---------------------------------------------------------------- 讲义 08 原文

const LESSON_CN: &str = include_str!("../../../web/src/content/lessons/08-firewall-zone.md");
const LESSON_EN: &str = include_str!("../../../web/src/content/lessons-en/08-firewall-zone.md");

/// 从真实讲义解析一个 IOS 代码块：从锚点起至代码块闭合。
/// 块内若带 `R1(config...)#` 提示符则去掉提示符；zone 块无提示符，原样保留。
fn lesson_block(md: &str, marker: &str) -> Vec<String> {
    let a = md.find(marker).expect("讲义缺少 IOS 演示锚点");
    let rest = &md[a..];
    let end = rest.find("\n```").expect("讲义 IOS 演示段未闭合");
    rest[..end]
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                return None;
            }
            match l.find(")# ") {
                Some(p) => Some(l[p + 3..].trim().to_string()),
                None => Some(l.to_string()),
            }
        })
        .filter(|l| !l.is_empty())
        .collect()
}

fn lesson_ios_sequences(md: &str) -> (Vec<String>, Vec<String>) {
    let zones = lesson_block(md, "zone security INSIDE");
    let policy = lesson_block(md, "R1(config)# class-map type inspect");
    (zones, policy)
}

#[test]
fn stage6b_lesson_ios_demo_sequence_runs_and_is_stateful() {
    let (cn_zones, cn_policy) = lesson_ios_sequences(LESSON_CN);
    let (en_zones, en_policy) = lesson_ios_sequences(LESSON_EN);
    assert!(!cn_zones.is_empty(), "中文讲义必须包含 IOS zone 块");
    assert!(!cn_policy.is_empty(), "中文讲义必须包含 IOS ZBF 策略块");
    assert_eq!(cn_zones, en_zones, "中英文 IOS zone 块必须一致");
    assert_eq!(cn_policy, en_policy, "中英文 IOS ZBF 策略块必须一致");
    assert_eq!(cn_zones[0], "zone security INSIDE");
    assert!(
        !cn_zones.iter().any(|l| l.contains("firewall zone trust")),
        "解析不得误入前面的 VRP 块"
    );
    assert!(cn_zones.iter().any(|l| l == "interface g0/0"));
    assert!(cn_zones.iter().any(|l| l == "interface g0/1"));
    assert!(!cn_zones.iter().any(|l| l == "interface g0/0/0"));
    assert!(cn_policy.iter().any(|l| l == "inspect"));
    assert!(cn_policy
        .iter()
        .any(|l| l.starts_with("service-policy type inspect")));

    // 测试夹具只配置接口地址；zone 与 ZBF 全部来自讲义原文。
    let mut t = topo();
    let mut s = Session::default();
    cfg(
        &mut t,
        &mut s,
        &[
            "interface g0/0",
            "ip address 192.168.1.1 255.255.255.0",
            "no shutdown",
            "exit",
            "interface g0/1",
            "ip address 203.0.113.1 255.255.255.0",
            "no shutdown",
            "exit",
        ],
    );
    // zone 块最后一条停在 config-if；两段代码之间补一个会话边界 `exit`（不是讲义命令）。
    let mut seq = cn_zones.clone();
    seq.push("exit".into());
    seq.extend(cn_policy.clone());

    let mut fw = Session::default();
    exec_and_run(&mut t.eng, &mut fw, t.r1, "enable");
    exec_and_run(&mut t.eng, &mut fw, t.r1, "configure terminal");
    let mut prompts = Vec::new();
    let mut failures = 0usize;
    for line in &seq {
        let out = exec_and_run(&mut t.eng, &mut fw, t.r1, line);
        if out.contains("%") || out.contains("Error:") {
            failures += 1;
            println!("FAIL `{line}` => {out}");
        }
        prompts.push(prompt(&t.eng, &fw, t.r1));
    }
    assert_eq!(failures, 0, "讲义原文逐条执行必须 0 失败");

    let idx = |needle: &str| {
        seq.iter()
            .position(|l| l == needle)
            .unwrap_or_else(|| panic!("讲义序列缺少 {needle}"))
    };
    for (needle, expected) in [
        ("zone security INSIDE", "R1(config-sec-zone)#"),
        ("zone security OUTSIDE", "R1(config-sec-zone)#"),
        ("interface g0/0", "R1(config-if)#"),
        ("zone-member security INSIDE", "R1(config-if)#"),
        ("interface g0/1", "R1(config-if)#"),
        ("zone-member security OUTSIDE", "R1(config-if)#"),
        ("class-map type inspect match-any ICMP", "R1(config-cmap)#"),
        ("policy-map type inspect PMAP", "R1(config-pmap)#"),
        ("class type inspect ICMP", "R1(config-pmap-c)#"),
        ("inspect", "R1(config-pmap-c)#"),
        (
            "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
            "R1(config-sec-zone-pair)#",
        ),
        (
            "service-policy type inspect PMAP",
            "R1(config-sec-zone-pair)#",
        ),
    ] {
        assert_eq!(prompts[idx(needle)], expected, "{needle} 的提示符");
    }
    // exit 层级：zone → config；pmap-c → pmap → config。
    assert_eq!(prompts[idx("zone security INSIDE") + 1], "R1(config)#");
    let c = idx("class type inspect ICMP");
    assert_eq!(prompts[c + 1], "R1(config-pmap-c)#");
    assert_eq!(prompts[c + 2], "R1(config-pmap)#");
    assert_eq!(prompts[c + 3], "R1(config)#");
    exec_and_run(&mut t.eng, &mut fw, t.r1, "end");
    assert_eq!(prompt(&t.eng, &fw, t.r1), "R1#");

    // 先验证反向无会话拒绝，再验证正向 inspect（ICMP 会话键为双向三元组，
    // 正向建会话后同 id 的反向探测会被当作回包；这是 6A 既有的 L4 简化边界）。
    assert_eq!(ping_srv(&mut t, "192.168.1.10"), 0, "反向无策略拒绝");
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5, "讲义序列应放行正向");
    assert_eq!(icmp_sessions(&t), 1, "inspect 建一条会话");
}

/// 引用完整性：被引用的 class-map / policy-map / 区域不得被静默删除。
#[test]
fn stage6b_referential_integrity_guards() {
    let mut t = topo();
    let mut s = Session::default();
    base_zones(&mut t, &mut s);
    zbf_inspect(&mut t, &mut s);

    let mut cfg_sess = Session::default();
    exec_and_run(&mut t.eng, &mut cfg_sess, t.r1, "enable");
    exec_and_run(&mut t.eng, &mut cfg_sess, t.r1, "configure terminal");
    for (line, what, needle) in [
        ("no class-map TRAFFIC", "class-map", "referenced"),
        ("no policy-map PMAP", "policy-map", "attached"),
        ("no zone security INSIDE", "zone", "referenced"),
    ] {
        let out = exec_and_run(&mut t.eng, &mut cfg_sess, t.r1, line);
        assert!(
            out.contains(needle),
            "{what} 被引用时必须拒绝删除: {line} => {out}"
        );
    }
    let rs = match &t.eng.net.device(t.r1).unwrap().state {
        DeviceState::Router(rs) => rs,
        _ => panic!(),
    };
    assert_eq!(rs.zbf_class_maps.len(), 1);
    assert_eq!(rs.zbf_policy_maps.len(), 1);
    assert_eq!(rs.zbf_zone_pairs.len(), 1);
    assert!(rs.security_zones.iter().any(|z| z.name == "INSIDE"));

    // 先解除挂接、删除 zone-pair，再删除区域才允许。
    for line in [
        "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
        "no service-policy type inspect PMAP",
        "exit",
        "no zone-pair security IN-OUT",
        "no zone security INSIDE",
    ] {
        let out = exec_and_run(&mut t.eng, &mut cfg_sess, t.r1, line);
        assert!(!out.contains("%"), "{line} => {out}");
    }
    let rs = match &t.eng.net.device(t.r1).unwrap().state {
        DeviceState::Router(rs) => rs,
        _ => panic!(),
    };
    assert!(rs.zbf_zone_pairs.is_empty());
    assert!(!rs.security_zones.iter().any(|z| z.name == "INSIDE"));
    assert!(t
        .eng
        .net
        .device(t.r1)
        .unwrap()
        .port(0)
        .unwrap()
        .zone
        .is_none());
}
