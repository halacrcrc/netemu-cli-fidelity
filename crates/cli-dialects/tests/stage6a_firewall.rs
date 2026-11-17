//! 阶段 6A：防火墙基线（华为 VRP 安全策略）血统拆分、ICMP/TCP 状态机校准与
//! 主方案 §14.3 最低验收场景全表闭环。
//!
//! 证据编号 V1–V9 见 `sim_core::profile` 常量与本文件每项断言的注释；
//! 评估矩阵与决策见 `docs/cli-fidelity/stage6a-implementation-record.md` §2。
//! 无带版本设备样本：全部为 `DocumentationOnly`，只做状态/结构/数据面断言，
//! 不建立逐字输出黄金。
//!
//! L2 边界（如实记录）：本模拟器的 VRP `display` 命令只在用户视图的树中出现，
//! 未实现官方「所有视图可用」；因此测试与课程用 `return` 显式回到用户视图再查询。

use cli_dialects::{exec_and_run, inspect_command, prompt, specs, trees};
use cli_engine::spec::SemanticSupport;
use cli_engine::{parse, Mode, Session};
use sim_core::device::{port_mac, DeviceKind, DeviceState, Dialect, TcpFlowState};
use sim_core::engine::{Engine, PING_HORIZON_NS};
use sim_core::profile::{
    EvidenceLevel, COMWARE_FIREWALL_PROFILE, TMOS_LB_PROFILE, VRP_FIREWALL_PROFILE,
};
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
    fw: u32,
    srv: u32,
}

/// PC1(trust) — FW1 — SRV(untrust)，与 lab10 拓扑一致。
fn topo(dialect: Dialect) -> Topo {
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let fw = net.add_device("FW1".into(), DeviceKind::Firewall, dialect);
    let srv = net.add_device("SRV".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc, fw).unwrap(); // FW p0
    net.add_link(fw, srv).unwrap(); // FW p1
    net.set_host(pc, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    net.set_host(srv, Some(ip("203.0.113.10")), 24, Some(ip("203.0.113.1")));
    Topo {
        eng: Engine::new(net),
        pc,
        fw,
        srv,
    }
}

/// 在防火墙上连续执行一段命令（不重置会话模式）。
fn run(t: &mut Topo, s: &mut Session, lines: &[&str]) {
    let dev = t.fw;
    for line in lines {
        let out = exec_and_run(&mut t.eng, s, dev, line);
        assert!(
            !out.contains("Error:") && !out.contains("% ") && !out.contains("Wrong parameter"),
            "{}: {}",
            line,
            out
        );
    }
}

/// 从用户视图重新进入系统视图后执行一段配置（与调用方残留模式无关）。
fn cfg(t: &mut Topo, s: &mut Session, lines: &[&str]) {
    *s = Session::default();
    run(t, s, &["system-view"]);
    run(t, s, lines);
}

/// 从用户视图执行一条查询/运维命令。
fn sh(t: &mut Topo, s: &mut Session, line: &str) -> String {
    *s = Session::default();
    exec_and_run(&mut t.eng, s, t.fw, line)
}

/// 基础配置：接口编址 + 区域划分（不配策略）。
fn base_config(t: &mut Topo, s: &mut Session) {
    cfg(
        t,
        s,
        &[
            "interface g0/0/0",
            "ip address 192.168.1.1 24",
            "undo shutdown",
            "quit",
            "interface g0/0/1",
            "ip address 203.0.113.1 24",
            "undo shutdown",
            "quit",
            "firewall zone trust",
            "add interface g0/0/0",
            "quit",
            "firewall zone untrust",
            "add interface g0/0/1",
            "quit",
        ],
    );
}

fn add_policy(t: &mut Topo, s: &mut Session, name: &str, src: &str, dst: &str, action: &str) {
    let rule = format!("rule name {name}");
    let srcz = format!("source-zone {src}");
    let dstz = format!("destination-zone {dst}");
    let act = format!("action {action}");
    cfg(
        t,
        s,
        &["security-policy", &rule, &srcz, &dstz, &act, "quit", "quit"],
    );
}

fn received_of(eng: &Engine, from: u32) -> u32 {
    let d = eng.net.device(from).unwrap();
    match &d.state {
        DeviceState::Host(h) => h.pings.last().map(|p| p.received()).unwrap_or(0),
        _ => 0,
    }
}

fn ping_pc(t: &mut Topo, target: &str) -> u32 {
    let from = t.pc;
    t.eng.start_ping(from, ip(target));
    t.eng.run_to_quiescence(PING_HORIZON_NS);
    received_of(&t.eng, from)
}

fn ping_srv(t: &mut Topo, target: &str) -> u32 {
    let from = t.srv;
    t.eng.start_ping(from, ip(target));
    t.eng.run_to_quiescence(PING_HORIZON_NS);
    received_of(&t.eng, from)
}

fn icmp_sessions(t: &Topo) -> usize {
    match &t.eng.net.device(t.fw).unwrap().state {
        DeviceState::Router(rs) => rs.security_sessions.len(),
        _ => 0,
    }
}

fn tcp_flows(t: &Topo) -> Vec<sim_core::device::TcpFlow> {
    match &t.eng.net.device(t.fw).unwrap().state {
        DeviceState::Router(rs) => rs.tcp_flows.clone(),
        _ => vec![],
    }
}

/// 注入一个原始 IPv4/ICMP 报文（用于「既有流」「非会话回包」这类 CLI 无法构造的场景）。
fn inject_icmp(t: &mut Topo, from_pc: bool, src: &str, dst: &str, id: u16, is_reply: bool) {
    let (dev, port, dst_dev, dst_port) = if from_pc {
        (t.pc, 0, t.fw, 0)
    } else {
        (t.srv, 0, t.fw, 1)
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
    let dev = t.pc;
    let fw = t.fw;
    let frame = EthernetFrame {
        dst: port_mac(fw, 0),
        src: port_mac(dev, 0),
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
    t.eng.transmit_frame(dev, 0, frame);
    t.eng.run_to_quiescence(PING_HORIZON_NS);
}

// ---------------------------------------------------------------- 规格与 Profile

#[test]
fn stage6a_vrp_firewall_spec_lint_is_clean_and_first_batch_is_strict() {
    let report = specs::lint_report(Dialect::Vrp);
    assert!(report.duplicates.is_empty(), "{report:?}");
    assert!(report.dangling.is_empty(), "{report:?}");
    assert!(report.drift.is_empty(), "{report:?}");
    assert!(report.strict_ok(), "{report:?}");

    // 首批基线命令：Implemented + DocumentationOnly + strict，且引用华为官方正文。
    let ids = [
        "config/firewall/zone/@0word",
        "zone/add/interface/@0if-name",
        "zone/undo/add/interface/@0if-name",
        "config/security-policy",
        "security_policy/rule/name/@0word",
        "security_policy/rule/move/@0word/@0word/@0word",
        "security_policy/undo/rule/name/@0word",
        "security_rule/source-zone/@0word",
        "security_rule/destination-zone/@0word",
        "security_rule/action/@0word",
        "security_rule/enable",
        "security_rule/disable",
        "exec/display/firewall/zone",
        "exec/display/firewall/session/table",
        "exec/reset/firewall/session/table",
        "exec/display/security-policy",
        "exec/display/security-policy/rule/name/@0word",
    ];
    for id in ids {
        let row = specs::find(Dialect::Vrp, id).unwrap_or_else(|| panic!("缺少规格 {id}"));
        assert_eq!(row.semantics, SemanticSupport::Implemented, "{id}");
        assert_eq!(row.evidence, EvidenceLevel::DocumentationOnly, "{id}");
        assert!(row.strict, "{id} 必须进入严格门禁");
        assert!(
            row.references
                .iter()
                .any(|r| r.contains("huawei") || r.contains("Huawei")),
            "{id} 必须引用华为官方证据"
        );
    }
    // Comware 安全数据面不在本包：其规格 lint 必须仍然干净。
    let comware_report = specs::lint_report(Dialect::Comware);
    assert!(comware_report.strict_ok(), "{comware_report:?}");
}

#[test]
fn stage6a_firewall_profiles_are_split_and_evidence_bound() {
    // 阶段 6A：DeviceKind::Firewall 按基线拆出独立 Profile，不再沿用路由器 Profile。
    use sim_core::profile::{profile_by_id, profile_for, profile_id_for};
    let vrp = profile_for(Dialect::Vrp, DeviceKind::Firewall);
    assert_eq!(vrp.id, VRP_FIREWALL_PROFILE);
    assert_eq!(profile_id_for(Dialect::Vrp, DeviceKind::Firewall), vrp.id);
    assert!(vrp.os_baseline.contains("Firewall"));
    assert_eq!(
        profile_by_id(COMWARE_FIREWALL_PROFILE).unwrap().id,
        COMWARE_FIREWALL_PROFILE
    );
    // 路由器 Profile 不被防火墙占用，反之亦然。
    assert_ne!(
        profile_for(Dialect::Vrp, DeviceKind::Router).id,
        vrp.id,
        "防火墙与路由器必须使用不同稳定 Profile ID"
    );
    // 基线数据面：域间与域内缺省都拒绝（V1/V4）。
    assert!(!vrp.policy.firewall.interzone_default_permit);
    assert!(!vrp.policy.firewall.intra_zone_default_permit);
}

// ---------------------------------------------------------------- 方言矩阵

#[test]
fn stage6a_firewall_dialect_matrix_is_bidirectional() {
    // (1) VRP 独占语法：必须只在 VRP 树中可解析，其他方言是**命令错误**而非「仅识别」。
    let vrp_only: &[(Mode, &str)] = &[
        (Mode::Config, "firewall zone trust"),
        (Mode::Zone, "add interface g0/0/0"),
        (Mode::Zone, "undo add interface g0/0/0"),
        (Mode::Exec, "display firewall zone"),
        (Mode::Exec, "display firewall session table"),
        (Mode::Exec, "reset firewall session table"),
    ];
    for (mode, line) in vrp_only {
        assert!(
            matches!(
                parse(trees(Dialect::Vrp).root_for(*mode), line),
                Ok(Some(_))
            ),
            "VRP 应当接受 {line} @ {mode:?}"
        );
        for (other, profile) in [
            (Dialect::Comware, COMWARE_FIREWALL_PROFILE),
            (Dialect::Tmos, TMOS_LB_PROFILE),
        ] {
            assert!(
                !matches!(parse(trees(other).root_for(*mode), line), Ok(Some(_))),
                "{other:?} 不得接受 VRP 独占命令 {line} @ {mode:?}"
            );
            let i = inspect_command(profile, *mode, line);
            assert_eq!(
                i.support.label(),
                "invalid-command",
                "{other:?} 的 {line} 必须是命令错误而不是「仅识别」"
            );
        }
    }

    // (2) VRP 与 Comware 共有的安全策略语法：VRP 基线侧必须接受并已实现；
    //     这里只主张 VRP 接受性，不主张 Comware 拒绝（Comware 安全数据面不在本包）。
    for (mode, line) in [
        (Mode::Config, "security-policy"),
        (Mode::SecurityPolicy, "rule name allow-out"),
        (Mode::SecurityPolicy, "rule move allow-out before deny-all"),
        (Mode::SecurityPolicy, "undo rule name allow-out"),
        (Mode::SecurityRule, "source-zone trust"),
        (Mode::SecurityRule, "destination-zone untrust"),
        (Mode::SecurityRule, "action permit"),
        (Mode::SecurityRule, "disable"),
        (Mode::SecurityRule, "enable"),
        (Mode::SecurityRule, "undo source-zone"),
        (Mode::Exec, "display security-policy"),
    ] {
        assert!(
            matches!(parse(trees(Dialect::Vrp).root_for(mode), line), Ok(Some(_))),
            "VRP 基线必须接受 {line} @ {mode:?}"
        );
    }
    // `rule move` / `undo rule name` / `disable` 不属于 IOS 教学抽象的形状。
    for (mode, line) in [
        (Mode::SecurityPolicy, "rule move allow-out before deny-all"),
        (Mode::SecurityPolicy, "undo rule name allow-out"),
        (Mode::SecurityRule, "disable"),
    ] {
        assert!(
            !matches!(parse(trees(Dialect::Ios).root_for(mode), line), Ok(Some(_))),
            "IOS 教学抽象不得接受 VRP 基线命令 {line} @ {mode:?}"
        );
    }

    // (3) Comware 原生写法不得混入 VRP 基线。
    for (mode, line) in [
        (Mode::Config, "security-zone name trust"),
        (Mode::Zone, "import interface g1/0/1"),
        (Mode::Exec, "display security-zone"),
    ] {
        assert!(
            !matches!(parse(trees(Dialect::Vrp).root_for(mode), line), Ok(Some(_))),
            "VRP 不得接受 Comware 专属写法 {line}"
        );
    }

    // (4) ASA 落选血统：任何方言都不再接受（`nameif` / `security-level`）。
    for d in [Dialect::Ios, Dialect::Vrp, Dialect::Comware] {
        for (mode, line) in [
            (Mode::If, "nameif inside"),
            (Mode::If, "security-level 100"),
        ] {
            assert!(
                !matches!(parse(trees(d).root_for(mode), line), Ok(Some(_))),
                "{d:?} 不得接受 ASA 命令 {line}"
            );
        }
    }

    // (5) 设备级门禁：非防火墙设备执行 VRP 基线命令必须被拒绝且状态不变。
    let mut net = Net::new();
    let r = net.add_device("R1".into(), DeviceKind::Router, Dialect::Vrp);
    let mut eng = Engine::new(net);
    let mut s = Session::default();
    exec_and_run(&mut eng, &mut s, r, "system-view");
    let before = cli_dialects::state_fingerprint(&eng);
    let out = exec_and_run(&mut eng, &mut s, r, "firewall zone trust");
    assert!(!out.is_empty(), "非防火墙设备必须给出设备侧错误: {out}");
    assert_eq!(
        before,
        cli_dialects::state_fingerprint(&eng),
        "状态不得改变"
    );
}

// ---------------------------------------------------------------- §14.3 场景

/// §14.3-1：外到内未命中策略时拒绝；§14.3-2：内到外命中允许策略时通过。
#[test]
fn stage6a_interzone_default_deny_and_permit_are_both_exercised() {
    let mut t = topo(Dialect::Vrp);
    let mut s = Session::default();
    base_config(&mut t, &mut s);

    // 未配策略：双向都拒绝（V1 缺省策略 any/any → deny）。
    assert_eq!(ping_srv(&mut t, "192.168.1.10"), 0, "外→内默认拒绝");
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 0, "内→外同样默认拒绝");
    assert_eq!(icmp_sessions(&t), 0, "被拒绝的流量不得建立会话");

    add_policy(&mut t, &mut s, "allow-out", "trust", "untrust", "permit");
    assert_eq!(ping_srv(&mut t, "192.168.1.10"), 0, "外→内仍拒绝");
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5, "内→外放行（V1/V9）");
}

/// §14.3-3：已承诺协议的状态化回包通过；§14.3-7：会话表与真实状态一致。
#[test]
fn stage6a_stateful_reply_passes_and_session_table_matches_state() {
    let mut t = topo(Dialect::Vrp);
    let mut s = Session::default();
    base_config(&mut t, &mut s);
    add_policy(&mut t, &mut s, "allow-out", "trust", "untrust", "permit");

    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5);
    // 一条 ICMP 会话即足以让 5 个探测的回包通过（V2：回包不再复查策略）。
    assert_eq!(icmp_sessions(&t), 1, "同一 echo id 只建立一条会话");

    let table = sh(&mut t, &mut s, "display firewall session table");
    assert!(table.contains("icmp"), "{table}");
    assert!(table.contains("192.168.1.10"), "{table}");
    assert!(table.contains("203.0.113.10"), "{table}");

    // TCP 状态化：SYN 建流，SYN-ACK/ACK 命中会话，回程不需要反向策略。
    let pc = t.pc;
    let mut pcs = Session::default();
    let out = exec_and_run(&mut t.eng, &mut pcs, pc, "tcp 203.0.113.10 80");
    assert!(out.contains("连接成功"), "{out}");
    let flows = tcp_flows(&t);
    assert_eq!(flows.len(), 1, "一条 TCP 会话: {flows:?}");
    assert!(matches!(
        flows[0].state,
        TcpFlowState::FinWait | TcpFlowState::Closed | TcpFlowState::Established
    ));
    let table = sh(&mut t, &mut s, "display firewall session table");
    assert!(table.contains("tcp"), "{table}");
}

/// §14.3-4：非会话回包与未授权新会话被正确拒绝。
#[test]
fn stage6a_non_session_reply_and_unauthorized_new_session_are_dropped() {
    let mut t = topo(Dialect::Vrp);
    let mut s = Session::default();
    base_config(&mut t, &mut s);
    add_policy(&mut t, &mut s, "allow-out", "trust", "untrust", "permit");

    // (a) 无会话的 echo reply（untrust → trust）不得凭策略新建会话。
    let before = t.eng.dropped;
    inject_icmp(&mut t, false, "203.0.113.10", "192.168.1.10", 4242, true);
    assert!(t.eng.dropped > before, "无会话回包必须被丢弃");
    assert_eq!(icmp_sessions(&t), 0, "回包不得建立会话");

    // (b) 非 SYN 的 TCP 报文在无会话时必须丢弃（防止绕过策略）。
    let before = t.eng.dropped;
    inject_tcp(&mut t, "192.168.1.10", "203.0.113.10", 40000, 80, TCP_ACK);
    assert!(t.eng.dropped > before, "非会话 TCP 报文必须被丢弃");
    assert!(tcp_flows(&t).is_empty(), "非 SYN 不得建流");

    // (c) 未授权新会话：untrust → trust 方向没有策略，echo request 也被拒绝。
    let before = t.eng.dropped;
    inject_icmp(&mut t, false, "203.0.113.10", "192.168.1.10", 4243, false);
    assert!(t.eng.dropped > before, "未授权新会话必须被拒绝");
}

/// §14.3-5：同区域转发有明确基线（V4：域内安全策略缺省禁止）与正、负用例。
#[test]
fn stage6a_same_zone_traffic_requires_a_policy_positive_and_negative() {
    // 两个接口放进同一区域，路由可通（同一台防火墙的两个三层口）。
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let fw = net.add_device("FW1".into(), DeviceKind::Firewall, Dialect::Vrp);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc, fw).unwrap();
    net.add_link(fw, pc2).unwrap();
    net.set_host(pc, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    net.set_host(pc2, Some(ip("192.168.2.10")), 24, Some(ip("192.168.2.1")));
    let mut t = Topo {
        eng: Engine::new(net),
        pc,
        fw,
        srv: pc2,
    };
    let mut s = Session::default();
    cfg(
        &mut t,
        &mut s,
        &[
            "interface g0/0/0",
            "ip address 192.168.1.1 24",
            "undo shutdown",
            "quit",
            "interface g0/0/1",
            "ip address 192.168.2.1 24",
            "undo shutdown",
            "quit",
            "firewall zone trust",
            "add interface g0/0/0",
            "add interface g0/0/1",
            "quit",
        ],
    );
    // 负向：同区域、无策略 → 按域内缺省拒绝（V4），不得无条件放行。
    assert_eq!(
        ping_pc(&mut t, "192.168.2.10"),
        0,
        "同区域无策略必须拒绝（V4）"
    );
    // 正向：同区域显式 permit 后放行，且回包走会话表。
    add_policy(&mut t, &mut s, "same-zone", "trust", "trust", "permit");
    assert_eq!(ping_pc(&mut t, "192.168.2.10"), 5, "同区域显式策略应放行");
    // 反向：同一区域对已被策略覆盖，因此同样通过。
    assert_eq!(ping_srv(&mut t, "192.168.1.10"), 5);
}

/// §14.3-6a：策略禁用 —— 新流拒绝，既有流继续；清会话表后既有流立即中断。
#[test]
fn stage6a_policy_disable_affects_new_flows_only_until_session_cleared() {
    let mut t = topo(Dialect::Vrp);
    let mut s = Session::default();
    base_config(&mut t, &mut s);
    add_policy(&mut t, &mut s, "allow-out", "trust", "untrust", "permit");
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5);

    // 记录既有会话的 echo id，用于「同一会话再来一个报文」。
    let (sess_id, src) = match &t.eng.net.device(t.fw).unwrap().state {
        DeviceState::Router(rs) => (rs.security_sessions[0].icmp_id, rs.security_sessions[0].src),
        _ => panic!("缺少会话"),
    };
    assert_eq!(src, ip("192.168.1.10"));
    let sessions_before = icmp_sessions(&t);

    // 禁用规则（V7）：display 的 STATE 变为 disable。
    cfg(
        &mut t,
        &mut s,
        &[
            "security-policy",
            "rule name allow-out",
            "disable",
            "quit",
            "quit",
        ],
    );
    let table = sh(&mut t, &mut s, "display security-policy");
    assert!(table.contains("disable"), "{table}");

    // 既有流继续：相同 echo id 的报文命中会话表，不再复查策略（V2）。
    let before = t.eng.dropped;
    inject_icmp(&mut t, true, "192.168.1.10", "203.0.113.10", sess_id, false);
    assert_eq!(t.eng.dropped, before, "既有流在规则禁用后仍应转发");
    assert_eq!(
        icmp_sessions(&t),
        sessions_before,
        "既有会话不得被策略变更删除"
    );

    // 新流拒绝：不同 echo id 需要重新匹配策略，已禁用规则不再命中。
    let before = t.eng.dropped;
    inject_icmp(
        &mut t,
        true,
        "192.168.1.10",
        "203.0.113.10",
        sess_id + 1,
        false,
    );
    assert!(t.eng.dropped > before, "新流必须被拒绝");

    // 清会话表（V3）后既有流立即中断，这正是官方给出的「立即生效」路径。
    sh(&mut t, &mut s, "reset firewall session table");
    assert_eq!(icmp_sessions(&t), 0, "reset 必须清空会话表");
    let before = t.eng.dropped;
    inject_icmp(&mut t, true, "192.168.1.10", "203.0.113.10", sess_id, false);
    assert!(t.eng.dropped > before, "清表后旧会话的报文必须被拒绝");

    // 重新启用后新流恢复。
    cfg(
        &mut t,
        &mut s,
        &[
            "security-policy",
            "rule name allow-out",
            "enable",
            "quit",
            "quit",
        ],
    );
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5);
}

/// §14.3-6b：策略删除与改序 —— 新流按新规则集处理。
#[test]
fn stage6a_policy_delete_and_reorder_change_only_future_matching() {
    let mut t = topo(Dialect::Vrp);
    let mut s = Session::default();
    base_config(&mut t, &mut s);
    add_policy(&mut t, &mut s, "allow-out", "trust", "untrust", "permit");
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5);

    // 追加一条 deny 规则，按顺序排在 permit 之后 → 仍命中 permit。
    add_policy(&mut t, &mut s, "deny-out", "trust", "untrust", "deny");
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5, "首条命中：permit 在前");

    // 改序（V7 `rule move ... before ...`）→ deny 变成首条 → 新流被拒绝。
    cfg(
        &mut t,
        &mut s,
        &[
            "security-policy",
            "rule move deny-out before allow-out",
            "quit",
            "quit",
        ],
    );
    let order = match &t.eng.net.device(t.fw).unwrap().state {
        DeviceState::Router(rs) => rs
            .security_policies
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    assert_eq!(order, vec!["deny-out".to_string(), "allow-out".to_string()]);
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 0, "改序后新流命中 deny");

    // 删除 deny 规则 → 回到 permit 首条 → 新流恢复。
    cfg(
        &mut t,
        &mut s,
        &["security-policy", "undo rule name deny-out", "quit", "quit"],
    );
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 5, "删除后恢复");
    let names = match &t.eng.net.device(t.fw).unwrap().state {
        DeviceState::Router(rs) => rs
            .security_policies
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    assert_eq!(names, vec!["allow-out".to_string()]);
}

/// §14.3-7：`display` 与配置及会话状态一致；配置视图可逐行重执行还原同一状态。
#[test]
fn stage6a_display_reflects_configuration_and_config_view_is_replayable() {
    let mut t = topo(Dialect::Vrp);
    let mut s = Session::default();
    base_config(&mut t, &mut s);
    add_policy(&mut t, &mut s, "allow-out", "trust", "untrust", "permit");

    let zones = sh(&mut t, &mut s, "display firewall zone");
    assert!(zones.contains("trust"), "{zones}");
    assert!(zones.contains("untrust"), "{zones}");
    assert!(zones.contains("85"), "trust 缺省优先级 85: {zones}");
    assert!(zones.contains("GigabitEthernet0/0/0"), "{zones}");

    let pol = sh(&mut t, &mut s, "display security-policy");
    assert!(pol.contains("allow-out"), "{pol}");
    assert!(pol.contains("permit"), "{pol}");
    assert!(pol.contains("default"), "必须显示平台缺省策略: {pol}");
    // 区域匹配条件在官方明细视图里（V9），表格视图不重复展示。
    let detail = sh(
        &mut t,
        &mut s,
        "display security-policy rule name allow-out",
    );
    assert!(detail.contains("source-zone trust"), "{detail}");
    assert!(detail.contains("destination-zone untrust"), "{detail}");
    assert!(detail.contains("action permit"), "{detail}");

    // 区域成员撤销后 display 同步。
    cfg(
        &mut t,
        &mut s,
        &["firewall zone untrust", "undo add interface g0/0/1", "quit"],
    );
    let zones = sh(&mut t, &mut s, "display firewall zone");
    assert!(
        !zones.contains("GigabitEthernet0/0/1"),
        "撤销后不得再显示该接口: {zones}"
    );
    let p = t.eng.net.device(t.fw).unwrap().port(1).unwrap();
    assert!(p.zone.is_none());

    // 配置视图逐行重执行还原同一状态（ADR D12 契约）。
    let mut t2 = topo(Dialect::Vrp);
    let mut s2 = Session::default();
    base_config(&mut t2, &mut s2);
    add_policy(&mut t2, &mut s2, "allow-out", "trust", "untrust", "permit");
    let rendered = sh(&mut t2, &mut s2, "display current-configuration");
    assert!(rendered.contains("firewall zone trust"), "{rendered}");
    assert!(rendered.contains("security-policy"), "{rendered}");
    // 不再渲染 ASA 血统与不可重执行的伪命令。
    assert!(!rendered.contains("nameif"), "{rendered}");
    assert!(!rendered.contains("security-level"), "{rendered}");
    assert!(
        !rendered.contains("->"),
        "不得残留 `src -> dst permit=` 伪命令: {rendered}"
    );

    // 在干净设备上逐行重执行渲染结果，末态必须与源设备一致。
    let mut t3 = topo(Dialect::Vrp);
    let mut s3 = Session::default();
    exec_and_run(&mut t3.eng, &mut s3, t3.fw, "system-view");
    for line in rendered.lines() {
        let out = exec_and_run(&mut t3.eng, &mut s3, t3.fw, line.trim());
        assert!(
            !out.contains("Error:") && !out.contains("Wrong parameter"),
            "渲染行不可重执行: {line} => {out}"
        );
    }
    let a = exec_and_run(&mut t3.eng, &mut s3, t3.fw, "display firewall zone");
    let b = sh(&mut t2, &mut s2, "display firewall zone");
    assert_eq!(a, b, "重执行后区域状态必须一致");
    let a = exec_and_run(&mut t3.eng, &mut s3, t3.fw, "display security-policy");
    let b = sh(&mut t2, &mut s2, "display security-policy");
    assert_eq!(a, b, "重执行后策略状态必须一致");
}

/// 存档往返：`security_zones` 与规则 `enabled` 参与摘要序列化，不丢状态。
#[test]
fn stage6a_summary_round_trip_keeps_zones_and_rule_state() {
    let mut t = topo(Dialect::Vrp);
    let mut s = Session::default();
    base_config(&mut t, &mut s);
    add_policy(&mut t, &mut s, "allow-out", "trust", "untrust", "permit");
    cfg(
        &mut t,
        &mut s,
        &[
            "security-policy",
            "rule name allow-out",
            "disable",
            "quit",
            "quit",
        ],
    );
    assert_eq!(ping_pc(&mut t, "203.0.113.10"), 0, "禁用规则不得放行");

    let sum = t.eng.net.summary();
    let mut net2 = Net::new();
    net2.apply_summary(&sum).unwrap();
    let mut eng2 = Engine::new(net2);
    let rs = match &eng2.net.device(t.fw).unwrap().state {
        DeviceState::Router(rs) => rs,
        _ => panic!(),
    };
    assert_eq!(
        rs.security_zones
            .iter()
            .map(|z| z.name.clone())
            .collect::<Vec<_>>(),
        vec!["trust".to_string(), "untrust".to_string()]
    );
    assert_eq!(rs.security_policies.len(), 1);
    assert!(!rs.security_policies[0].enabled, "禁用状态必须往返保留");

    // 重载后仍不放行。
    let pc = t.pc;
    eng2.start_ping(pc, ip("203.0.113.10"));
    eng2.run_to_quiescence(PING_HORIZON_NS);
    assert_eq!(received_of(&eng2, pc), 0, "重载后禁用规则不得放行");
}

#[test]
fn stage6a_prompts_use_vrp_hierarchy() {
    let mut t = topo(Dialect::Vrp);
    let mut s = Session::default();
    exec_and_run(&mut t.eng, &mut s, t.fw, "system-view");
    exec_and_run(&mut t.eng, &mut s, t.fw, "firewall zone trust");
    assert_eq!(prompt(&t.eng, &s, t.fw), "[FW1-zone-trust]");
    exec_and_run(&mut t.eng, &mut s, t.fw, "quit");
    exec_and_run(&mut t.eng, &mut s, t.fw, "security-policy");
    assert_eq!(prompt(&t.eng, &s, t.fw), "[FW1-security-policy]");
    exec_and_run(&mut t.eng, &mut s, t.fw, "rule name r1");
    assert_eq!(prompt(&t.eng, &s, t.fw), "[FW1-security-policy-rule-r1]");
}

// ---------------------------------------------------------------- 课程原文三层（Rust 层）

const LABS_CN: &str = include_str!("../../../web/src/lib/labs.ts");
const LABS_EN: &str = include_str!("../../../web/src/content/labs-en.ts");

fn strip_task_notes(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '（' | '(' => {
                depth += 1;
                out.push(' ');
            }
            '）' | ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

/// 与 Stage 4A/5A 相同的课程原文解析规则：去括号附注 → 只在**第一个箭头之前**
/// 且后跟空格的标签冒号处切标签 → 按 `→` / `->` 切分。
fn task_label_and_cli(task: &str) -> (String, Vec<String>) {
    let clean = strip_task_notes(task);
    let arrow = clean
        .find("->")
        .or_else(|| clean.find('→'))
        .unwrap_or(clean.len());
    let head = &clean[..arrow];
    let mut label = String::new();
    let body = if let Some(p) = head.find('：') {
        label = head[..p].trim().to_string();
        clean[p + '：'.len_utf8()..].to_string()
    } else if let Some(p) = head.find(": ") {
        label = head[..p].trim().to_string();
        clean[p + 2..].to_string()
    } else {
        clean.clone()
    };
    let cmds = body
        .replace('→', "->")
        .split("->")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    (label, cmds)
}

fn lab10_tasks(src: &str, marker: &str) -> Vec<String> {
    let a = src.find(marker).unwrap_or_else(|| panic!("缺少 {marker}"));
    let rest = &src[a..];
    let b = rest.find("tasks: [").expect("缺少 tasks: [");
    let body = &rest[b + "tasks: [".len()..];
    let e = body
        .find("\n    ],")
        .or_else(|| body.find("\n  ],"))
        .expect("缺少 tasks 结束");
    body[..e]
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let s = l.find('\'')?;
            let e = l[s + 1..].find('\'')? + s + 1;
            Some(l[s + 1..e].to_string())
        })
        .collect()
}

/// 只匹配真正的 CLI 报错；`100% packet loss` 这类正常统计文本不得误判。
fn is_cli_error(out: &str) -> bool {
    const MARKERS: &[&str] = &[
        "Error:",
        "% Invalid",
        "% Unrecognized",
        "% Ambiguous",
        "% Incomplete",
        "% Too many",
        "% Wrong parameter",
        "Wrong parameter found",
        "Syntax Error:",
    ];
    MARKERS.iter().any(|m| out.contains(m))
}

fn sequences(tasks: &[String]) -> Vec<Vec<String>> {
    tasks
        .iter()
        .map(|t| task_label_and_cli(t).1)
        .collect::<Vec<_>>()
}

/// lab10 课程原文：中英文同一序列，逐条在真实引擎上执行 0 失败，
/// 并断言课程承诺的正向/负向数据面结果。
#[test]
fn stage6a_lab10_course_original_sequence_runs_top_to_bottom() {
    let cn = lab10_tasks(LABS_CN, "export const lab10: LabDef = {");
    let en = lab10_tasks(LABS_EN, "'firewall-zone': {");
    assert!(!cn.is_empty() && !en.is_empty(), "lab10 任务不得为空");
    assert_eq!(
        sequences(&cn),
        sequences(&en),
        "lab10 中英文必须是同一严格可执行序列"
    );
    assert!(
        cn.len() >= 10,
        "lab10 应覆盖配置/查询/禁用/清表: {}",
        cn.len()
    );

    let mut t = topo(Dialect::Vrp);
    let mut fw_sess = Session::default();
    let mut pc_sess = Session::default();
    let mut srv_sess = Session::default();
    let mut failures = 0usize;
    // 课程原文承诺的数据面结果：PC1→SRV 先通（5/5）、规则禁用并清表后不通；
    // SRV→PC1 始终不通。
    let mut pc_to_srv: Vec<String> = vec![];
    let mut srv_to_pc: Vec<String> = vec![];
    for (i, task) in cn.iter().enumerate() {
        let (label, cmds) = task_label_and_cli(task);
        assert!(!cmds.is_empty(), "第 {i} 条任务无可执行命令: {task}");
        for line in &cmds {
            let out = match label.as_str() {
                "FW1" => exec_and_run(&mut t.eng, &mut fw_sess, t.fw, line),
                "PC1" => exec_and_run(&mut t.eng, &mut pc_sess, t.pc, line),
                "SRV" => exec_and_run(&mut t.eng, &mut srv_sess, t.srv, line),
                other => panic!("未知任务标签 {other}"),
            };
            if is_cli_error(&out) {
                failures += 1;
                println!("FAIL step {i} `{line}` => {out}");
            }
            if line == "ping 203.0.113.10" {
                pc_to_srv.push(out.clone());
            }
            if line == "ping 192.168.1.10" {
                srv_to_pc.push(out.clone());
            }
        }
    }
    assert_eq!(failures, 0, "课程原文逐条执行必须 0 失败");
    assert_eq!(pc_to_srv.len(), 2, "PC1→SRV 应有两次 ping: {pc_to_srv:?}");
    assert!(
        pc_to_srv[0].contains("5 received"),
        "放行阶段应 5/5: {}",
        pc_to_srv[0]
    );
    assert!(
        pc_to_srv[1].contains("0 received"),
        "规则禁用并清表后应 0/5: {}",
        pc_to_srv[1]
    );
    assert_eq!(srv_to_pc.len(), 1, "SRV→PC1 应有一次 ping: {srv_to_pc:?}");
    for out in &srv_to_pc {
        assert!(out.contains("0 received"), "反向全程拒绝: {out}");
    }

    // 末态：allow-out 已被禁用、会话表已被清空。
    let mut q = Session::default();
    let pol = exec_and_run(&mut t.eng, &mut q, t.fw, "display security-policy");
    assert!(pol.contains("disable"), "{pol}");
    assert_eq!(icmp_sessions(&t), 0, "reset 后会话表必须为空");
    assert!(tcp_flows(&t).is_empty(), "reset 后 TCP 流表必须为空");
}
