//! 阶段 1 末最小端到端原型：C1（IOS）+ H0（VRP）。
//!
//! 验收项按交接文档 §6 组织：独立方言、状态与数据面、执行状态与输出、
//! 配置视图、lint/纯读检查。所有断言走真实执行链，不直接调用解析函数冒充执行。

use cli_dialects::specs;
use cli_dialects::{
    complete_line, exec_and_run_result, exec_line_result, inspect_command, inspect_device_command,
    roots_for, scan_simulator_wording, state_fingerprint,
};
use cli_engine::spec::SpecKey;
use cli_engine::spec::{
    lint, CommandResult, CommandStatus, LintReport, SemanticSupport, SupportLabel,
};
use cli_engine::{Mode, Session};
use sim_core::device::{DeviceKind, Dialect};
use sim_core::engine::Engine;
use sim_core::profile as profile_mod;
use sim_core::summary::DevSummary;
use sim_core::Net;
use sim_packets::ipv4_from_str;

fn ip(s: &str) -> u32 {
    ipv4_from_str(s).unwrap()
}

struct Lab {
    eng: Engine,
    pc1: u32,
    r1: u32,
    r2: u32,
    pc2: u32,
    s1: Session,
    s2: Session,
}

fn lab(dialect: Dialect) -> Lab {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, dialect);
    let r2 = net.add_device("R2".into(), DeviceKind::Router, dialect);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc1, r1).unwrap();
    net.add_link(r1, r2).unwrap();
    net.add_link(r2, pc2).unwrap();
    net.set_host(pc1, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    net.set_host(pc2, Some(ip("192.168.2.10")), 24, Some(ip("192.168.2.1")));
    Lab {
        eng: Engine::new(net),
        pc1,
        r1,
        r2,
        pc2,
        s1: Session::default(),
        s2: Session::default(),
    }
}

fn dev(eng: &Engine, id: u32) -> DevSummary {
    eng.net
        .summary()
        .devices
        .into_iter()
        .find(|d| d.id == id)
        .unwrap()
}

/// 从配置视图中截取单个接口块，避免把其他端口的默认 shutdown 误判为目标接口状态。
fn interface_block(cfg: &str, name: &str) -> String {
    let mut block = String::new();
    let mut active = false;
    for line in cfg.lines() {
        if let Some(rest) = line.strip_prefix("interface ") {
            active = rest.trim() == name;
        }
        if active {
            block.push_str(line);
            block.push('\n');
        }
    }
    block
}

fn run(l: &mut Lab, which: u8, lines: &[&str]) -> Vec<CommandResult> {
    let id = if which == 1 { l.r1 } else { l.r2 };
    let mut out = Vec::new();
    for line in lines {
        let r = if which == 1 {
            exec_and_run_result(&mut l.eng, &mut l.s1, id, line)
        } else {
            exec_and_run_result(&mut l.eng, &mut l.s2, id, line)
        };
        out.push(r);
    }
    out
}

fn run1(l: &mut Lab, lines: &[&str]) -> Vec<CommandResult> {
    run(l, 1, lines)
}

fn run2(l: &mut Lab, lines: &[&str]) -> Vec<CommandResult> {
    run(l, 2, lines)
}

fn ios_configure(l: &mut Lab) {
    let rs = run1(
        l,
        &[
            "enable",
            "configure terminal",
            "interface g0/0",
            "ip address 192.168.1.1 255.255.255.0",
            "no shutdown",
            "exit",
            "interface g0/1",
            "ip address 10.0.0.1 255.255.255.0",
            "no shutdown",
            "exit",
            "ip route 192.168.2.0 255.255.255.0 10.0.0.2",
            "end",
        ],
    );
    for (i, r) in rs.iter().enumerate() {
        assert_eq!(r.status, CommandStatus::Executed, "IOS R1 第 {i} 条: {r:?}");
    }
    let rs = run2(
        l,
        &[
            "enable",
            "configure terminal",
            "interface g0/0",
            "ip address 10.0.0.2 255.255.255.0",
            "no shutdown",
            "exit",
            "interface g0/1",
            "ip address 192.168.2.1 255.255.255.0",
            "no shutdown",
            "exit",
            "ip route 192.168.1.0 255.255.255.0 10.0.0.1",
            "end",
        ],
    );
    for (i, r) in rs.iter().enumerate() {
        assert_eq!(r.status, CommandStatus::Executed, "IOS R2 第 {i} 条: {r:?}");
    }
}

fn vrp_configure(l: &mut Lab) {
    let rs = run1(
        l,
        &[
            "system-view",
            "interface GigabitEthernet0/0/0",
            "ip address 192.168.1.1 24",
            "undo shutdown",
            "quit",
            "interface g0/0/1",
            "ip address 10.0.0.1 24",
            "undo shutdown",
            "quit",
            "ip route-static 192.168.2.0 24 10.0.0.2",
            "return",
        ],
    );
    for (i, r) in rs.iter().enumerate() {
        assert_eq!(r.status, CommandStatus::Executed, "VRP R1 第 {i} 条: {r:?}");
    }
    let rs = run2(
        l,
        &[
            "system-view",
            "interface GigabitEthernet0/0/0",
            "ip address 10.0.0.2 24",
            "undo shutdown",
            "quit",
            "interface g0/0/1",
            "ip address 192.168.2.1 24",
            "undo shutdown",
            "quit",
            "ip route-static 192.168.1.0 24 10.0.0.1",
            "return",
        ],
    );
    for (i, r) in rs.iter().enumerate() {
        assert_eq!(r.status, CommandStatus::Executed, "VRP R2 第 {i} 条: {r:?}");
    }
}

// ---------- §6.1 独立方言 ----------

#[test]
fn ios_c1_and_vrp_h0_work_independently() {
    let mut l = lab(Dialect::Ios);
    ios_configure(&mut l);
    let route = exec_and_run_result(&mut l.eng, &mut l.s1, l.r1, "show ip route");
    assert_eq!(route.status, CommandStatus::Executed);
    assert!(route
        .device_output
        .contains("S    192.168.2.0/24 [1/0] via 10.0.0.2"));

    let mut v = lab(Dialect::Vrp);
    vrp_configure(&mut v);
    let route = exec_and_run_result(&mut v.eng, &mut v.s1, v.r1, "display ip routing-table");
    assert_eq!(route.status, CommandStatus::Executed);
    assert!(route.device_output.contains("192.168.2.0/24"));
    assert!(route.device_output.contains("Static"));
}

#[test]
fn cross_dialect_key_commands_are_rejected() {
    let mut l = lab(Dialect::Ios);
    let before = state_fingerprint(&l.eng);
    let bad = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "display ip routing-table");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert!(!bad.state_changed);
    assert_eq!(state_fingerprint(&l.eng), before);
    let bad = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "undo shutdown");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);

    let mut v = lab(Dialect::Vrp);
    let bad = exec_line_result(&mut v.eng, &mut v.s1, v.r1, "show ip route");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    let bad = exec_line_result(&mut v.eng, &mut v.s1, v.r1, "no shutdown");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert!(!bad.state_changed);
}

#[test]
fn help_completion_and_error_position_still_work() {
    let mut l = lab(Dialect::Ios);
    run1(&mut l, &["enable"]);
    let help = exec_and_run_result(&mut l.eng, &mut l.s1, l.r1, "show ip ?");
    assert!(help.device_output.contains("route"));
    let (completed, options) = complete_line(&l.eng, &l.s1, l.r1, "show ip r");
    assert_eq!(completed, "show ip route ");
    assert_eq!(options, vec!["route".to_string()]);
    let bad = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "show ip routX");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert!(bad
        .device_output
        .contains("Invalid input detected at '^' marker."));
}

#[test]
fn c1_h0_matrix_covers_bad_params_modes_and_abbreviations() {
    // §8.1：错误参数、缺失参数、多余参数、正确/错误模式、唯一缩写与歧义缩写。
    let mut l = lab(Dialect::Ios);
    run1(&mut l, &["enable", "configure terminal", "interface g0/0"]);
    let cases = [
        ("ip address 999.999.999.999 255.255.255.0", "错误参数"),
        ("ip address", "缺失参数"),
    ];
    for (line, why) in cases {
        let r = exec_line_result(&mut l.eng, &mut l.s1, l.r1, line);
        assert_eq!(r.status, CommandStatus::InvalidCommand, "{why}: {line}");
        assert!(!r.state_changed);
    }
    run1(&mut l, &["end"]);
    let too_many = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "show ip route extra");
    assert_eq!(too_many.status, CommandStatus::InvalidCommand, "多余参数");
    let ok = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "sh ip ro");
    assert_eq!(ok.status, CommandStatus::Executed, "唯一缩写");
    assert!(ok.device_output.contains("Codes:"));
    let ambiguous = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "show i");
    assert_eq!(ambiguous.status, CommandStatus::InvalidCommand, "歧义缩写");
    // `show ip route` 只在 Exec/Priv 可用，Config 模式下必须是 InvalidCommand。
    run1(&mut l, &["configure terminal"]);
    let wrong_mode = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "show ip route");
    assert_eq!(wrong_mode.status, CommandStatus::InvalidCommand, "错误模式");

    let mut v = lab(Dialect::Vrp);
    run1(&mut v, &["system-view"]);
    let missing = exec_line_result(&mut v.eng, &mut v.s1, v.r1, "ip route-static");
    assert_eq!(
        missing.status,
        CommandStatus::InvalidCommand,
        "VRP 缺失参数"
    );
    run1(&mut v, &["return"]);
    let ok = exec_line_result(&mut v.eng, &mut v.s1, v.r1, "dis ip rout");
    assert_eq!(ok.status, CommandStatus::Executed, "VRP 唯一缩写");
    let ambiguous = exec_line_result(&mut v.eng, &mut v.s1, v.r1, "display i");
    assert_eq!(
        ambiguous.status,
        CommandStatus::InvalidCommand,
        "VRP 歧义缩写"
    );
    // 配置命令只能在 Config 视图使用。
    let wrong_mode = exec_line_result(
        &mut v.eng,
        &mut v.s1,
        v.r1,
        "ip route-static 1.2.3.0 24 10.0.0.2",
    );
    assert_eq!(
        wrong_mode.status,
        CommandStatus::InvalidCommand,
        "VRP 错误模式"
    );
}

// ---------- §6.2 状态与数据面 ----------

#[test]
fn ios_router_defaults_down_vrp_router_defaults_up() {
    let l = lab(Dialect::Ios);
    assert!(
        !dev(&l.eng, l.r1).ports[0].admin_up,
        "IOS 路由器口默认 down"
    );
    let v = lab(Dialect::Vrp);
    assert!(dev(&v.eng, v.r1).ports[0].admin_up, "VRP 路由器口默认 up");
    assert!(!profile_mod::platform_policy(Dialect::Ios).router_interface_admin_up);
    assert!(profile_mod::platform_policy(Dialect::Vrp).router_interface_admin_up);
    assert_ne!(
        profile_mod::profile_for(Dialect::Ios, DeviceKind::Router).id,
        profile_mod::profile_for(Dialect::Vrp, DeviceKind::Router).id
    );
}

#[test]
fn shutdown_and_undo_change_state_and_config_view() {
    let mut l = lab(Dialect::Ios);
    run1(
        &mut l,
        &["enable", "configure terminal", "interface g0/0", "exit"],
    );
    assert!(!dev(&l.eng, l.r1).ports[0].admin_up);
    let cfg = exec_and_run_result(&mut l.eng, &mut l.s1, l.r1, "end");
    assert_eq!(cfg.status, CommandStatus::Executed);
    let cfg = exec_and_run_result(&mut l.eng, &mut l.s1, l.r1, "show running-config");
    assert_eq!(cfg.status, CommandStatus::Executed);
    assert!(cfg.device_output.contains(" shutdown"));

    run1(&mut l, &["configure terminal", "interface g0/0"]);
    let up = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "no shutdown");
    assert_eq!(up.status, CommandStatus::Executed);
    assert!(up.state_changed);
    assert!(dev(&l.eng, l.r1).ports[0].admin_up);

    let down = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "shutdown");
    assert_eq!(down.status, CommandStatus::Executed);
    assert!(down.state_changed);
    assert!(!dev(&l.eng, l.r1).ports[0].admin_up);

    let mut v = lab(Dialect::Vrp);
    run1(&mut v, &["system-view", "interface GigabitEthernet0/0/0"]);
    assert!(dev(&v.eng, v.r1).ports[0].admin_up);
    let up = exec_line_result(&mut v.eng, &mut v.s1, v.r1, "undo shutdown");
    assert_eq!(up.status, CommandStatus::Executed);
    assert!(dev(&v.eng, v.r1).ports[0].admin_up);
}

#[test]
fn static_route_write_delete_and_show_are_consistent() {
    let mut l = lab(Dialect::Ios);
    run1(&mut l, &["enable", "configure terminal"]);
    let add = exec_line_result(
        &mut l.eng,
        &mut l.s1,
        l.r1,
        "ip route 192.168.2.0 255.255.255.0 10.0.0.2",
    );
    assert_eq!(add.status, CommandStatus::Executed);
    assert!(add.state_changed);
    assert_eq!(dev(&l.eng, l.r1).static_routes.len(), 1);
    assert_eq!(dev(&l.eng, l.r1).static_routes[0].len, 24);

    let del = exec_line_result(
        &mut l.eng,
        &mut l.s1,
        l.r1,
        "no ip route 192.168.2.0 255.255.255.0 10.0.0.2",
    );
    assert_eq!(del.status, CommandStatus::Executed);
    assert!(del.state_changed);
    assert!(dev(&l.eng, l.r1).static_routes.is_empty());

    let mut v = lab(Dialect::Vrp);
    run1(&mut v, &["system-view"]);
    let add = exec_line_result(
        &mut v.eng,
        &mut v.s1,
        v.r1,
        "ip route-static 192.168.2.0 24 10.0.0.2",
    );
    assert_eq!(add.status, CommandStatus::Executed);
    assert_eq!(dev(&v.eng, v.r1).static_routes.len(), 1);
    let del = exec_line_result(
        &mut v.eng,
        &mut v.s1,
        v.r1,
        "undo ip route-static 192.168.2.0 24 10.0.0.2",
    );
    assert_eq!(del.status, CommandStatus::Executed);
    assert!(dev(&v.eng, v.r1).static_routes.is_empty());
}

#[test]
fn handler_precheck_failure_does_not_leave_half_config() {
    let mut l = lab(Dialect::Ios);
    run1(&mut l, &["enable", "configure terminal"]);
    let before = state_fingerprint(&l.eng);
    let bad = exec_line_result(
        &mut l.eng,
        &mut l.s1,
        l.r1,
        "ip route 192.168.2.0 255.255.255.0 0.0.0.0",
    );
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert!(!bad.state_changed);
    assert_eq!(state_fingerprint(&l.eng), before);
    assert!(dev(&l.eng, l.r1).static_routes.is_empty());
}

#[test]
fn positive_and_negative_data_plane_scenarios() {
    let mut l = lab(Dialect::Ios);
    assert_eq!(dev(&l.eng, l.pc2).name, "PC2");
    ios_configure(&mut l);
    let ok = exec_and_run_result(&mut l.eng, &mut l.s1, l.r1, "ping 192.168.2.10");
    assert_eq!(ok.status, CommandStatus::Executed);
    assert!(ok
        .device_output
        .contains("Success rate is 100 percent (5/5)"));
    assert!(ok.device_output.contains("!!!!!"));

    // 负向 1：撤销回程静态路由后 ping 全超时。
    run1(&mut l, &["configure terminal"]);
    let del = exec_line_result(
        &mut l.eng,
        &mut l.s1,
        l.r1,
        "no ip route 192.168.2.0 255.255.255.0 10.0.0.2",
    );
    assert_eq!(del.status, CommandStatus::Executed);
    run1(&mut l, &["end"]);
    let fail = exec_and_run_result(&mut l.eng, &mut l.s1, l.r1, "ping 192.168.2.10");
    assert!(fail
        .device_output
        .contains("Success rate is 0 percent (0/5)"));
    // 主机侧 ping 经过路由器转发，可验证报文确因查无路由被丢弃。
    let mut spc = Session::default();
    let fail = exec_and_run_result(&mut l.eng, &mut spc, l.pc1, "ping 192.168.2.10");
    assert!(
        fail.device_output.contains("0 received"),
        "{}",
        fail.device_output
    );
    assert!(l.eng.dropped > 0, "stats={:?}", l.eng.stats());

    // 负向 2：接口 shutdown 后即使有路由也不可达。
    let mut l2 = lab(Dialect::Ios);
    ios_configure(&mut l2);
    run1(&mut l2, &["configure terminal", "interface g0/1"]);
    let shut = exec_line_result(&mut l2.eng, &mut l2.s1, l2.r1, "shutdown");
    assert_eq!(shut.status, CommandStatus::Executed);
    assert!(!dev(&l2.eng, l2.r1).ports[1].admin_up);
    run1(&mut l2, &["end"]);
    let fail = exec_and_run_result(&mut l2.eng, &mut l2.s1, l2.r1, "ping 192.168.2.10");
    assert!(fail
        .device_output
        .contains("Success rate is 0 percent (0/5)"));
    let mut spc = Session::default();
    let fail = exec_and_run_result(&mut l2.eng, &mut spc, l2.pc1, "ping 192.168.2.10");
    assert!(
        fail.device_output.contains("0 received"),
        "{}",
        fail.device_output
    );

    // VRP 正向。
    let mut v = lab(Dialect::Vrp);
    vrp_configure(&mut v);
    let ok = exec_and_run_result(&mut v.eng, &mut v.s1, v.r1, "ping 192.168.2.10");
    assert!(ok.device_output.contains("5 packet(s) received"));
    assert!(!ok.device_output.contains("100.00% packet loss"));
}

// ---------- §6.3 四态、证据与状态不变 ----------

#[test]
fn unsupported_by_device_uses_positive_evidence_and_preserves_state() {
    let mut l = lab(Dialect::Ios);
    run1(&mut l, &["enable", "configure terminal", "interface g0/0"]);
    let before = state_fingerprint(&l.eng);
    let r = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "switchport mode access");
    assert_eq!(r.status, CommandStatus::UnsupportedByDevice);
    assert!(!r.state_changed);
    assert_eq!(state_fingerprint(&l.eng), before);
    assert!(r.device_output.contains("Invalid input detected"));
    assert!(r.simulator_notices.is_empty());
    assert!(scan_simulator_wording(&r.device_output).is_empty());

    // 有积极证据：IOS Router 不含二层交换端口能力；IOS Switch 则支持。
    let p_router = profile_mod::profile_for(Dialect::Ios, DeviceKind::Router);
    let e = p_router
        .capability_evidence(profile_mod::Capability::L2SwitchPort)
        .unwrap();
    assert_eq!(e.verdict, profile_mod::CapabilityVerdict::Unsupported);
    assert!(e.source.contains("cisco.com"));
    assert_eq!(
        profile_mod::profile_for(Dialect::Ios, DeviceKind::Switch)
            .capability(profile_mod::Capability::L2SwitchPort),
        profile_mod::CapabilityVerdict::Supported
    );
    let ins = inspect_command(
        profile_mod::IOS_ROUTER_PROFILE,
        Mode::If,
        "switchport mode access",
    );
    assert_eq!(ins.status, CommandStatus::UnsupportedByDevice);
    assert_eq!(ins.support, SupportLabel::UnsupportedByDevice);
    assert!(!ins.passed);
}

#[test]
fn not_simulated_command_leaves_state_unchanged() {
    let mut v = lab(Dialect::Vrp);
    let before = state_fingerprint(&v.eng);
    let summary_before = format!("{:?}", dev(&v.eng, v.r1));
    let r = exec_line_result(&mut v.eng, &mut v.s1, v.r1, "save");
    assert_eq!(r.status, CommandStatus::NotSimulated);
    assert!(!r.state_changed);
    assert_eq!(state_fingerprint(&v.eng), before);
    assert_eq!(format!("{:?}", dev(&v.eng, v.r1)), summary_before);
    assert_eq!(v.eng.now_ns(), 0);
    assert!(r.device_output.is_empty(), "不得伪造厂商输出");
    assert!(!r.simulator_notices.is_empty());
    assert!(r
        .simulator_notices
        .iter()
        .all(|n| n.starts_with("[netemu]")));

    let ins = inspect_command(profile_mod::VRP_ROUTER_PROFILE, Mode::Exec, "save");
    assert_eq!(ins.status, CommandStatus::NotSimulated);
    assert_eq!(ins.support, SupportLabel::RecognizedOnly);
    assert!(!ins.passed);
}

#[test]
fn evidence_insufficient_but_semantics_implemented_still_executes() {
    let mut l = lab(Dialect::Ios);
    run1(&mut l, &["enable"]);
    let r = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "show ip route");
    assert_eq!(r.status, CommandStatus::Executed);
    assert!(r.device_output.contains("Codes:"));
    assert!(r.simulator_notices.iter().any(|n| n.contains("待验证")));
    let ins = inspect_command(profile_mod::IOS_ROUTER_PROFILE, Mode::Priv, "show ip route");
    assert_eq!(ins.status, CommandStatus::Executed);
    assert_eq!(ins.support, SupportLabel::PendingVerification);
    assert!(!ins.passed, "证据不足不得标为完整支持");
    let ins = inspect_command(
        profile_mod::VRP_ROUTER_PROFILE,
        Mode::Exec,
        "display ip routing-table",
    );
    assert_eq!(ins.status, CommandStatus::Executed);
    assert_eq!(ins.support, SupportLabel::PendingVerification);

    // 证据不足 + 能力未知仍不得判定为 UnsupportedByDevice。
    use cli_engine::spec::{judge, CapabilityVerdict, EvidenceLevel, JudgeInput};
    let unknown = judge(JudgeInput {
        spec_id: "unknown-evidence",
        parse_ok: true,
        mode_ok: true,
        capability: CapabilityVerdict::Unknown,
        semantics: SemanticSupport::Implemented,
        evidence: EvidenceLevel::Unverified,
    });
    assert_eq!(unknown.status, CommandStatus::Executed);
    assert_eq!(unknown.support, SupportLabel::PendingVerification);
}

#[test]
fn all_non_executed_paths_preserve_state() {
    let mut l = lab(Dialect::Ios);
    run1(&mut l, &["enable", "configure terminal", "interface g0/0"]);
    for line in ["switchport mode access", "this-is-not-a-command", ""] {
        let before = state_fingerprint(&l.eng);
        let r = exec_line_result(&mut l.eng, &mut l.s1, l.r1, line);
        assert!(!r.status.allows_state_change() || r.status == CommandStatus::Executed);
        if r.status != CommandStatus::Executed {
            assert!(!r.state_changed);
            assert_eq!(state_fingerprint(&l.eng), before, "命令 {line} 改变了状态");
        }
    }
    let mut v = lab(Dialect::Vrp);
    let before = state_fingerprint(&v.eng);
    let r = exec_line_result(&mut v.eng, &mut v.s1, v.r1, "save");
    assert_eq!(r.status, CommandStatus::NotSimulated);
    assert!(!r.state_changed);
    assert_eq!(state_fingerprint(&v.eng), before);
}

#[test]
fn command_result_has_no_simulation_event_stream() {
    // 穷举解构：新增任何字段（例如事件流）都会导致编译失败。
    let mut l = lab(Dialect::Ios);
    let CommandResult {
        status,
        device_output,
        simulator_notices,
        state_changed,
    } = exec_line_result(&mut l.eng, &mut l.s1, l.r1, "enable");
    assert_eq!(status, CommandStatus::Executed);
    assert!(device_output.is_empty());
    assert!(simulator_notices.is_empty());
    assert!(!state_changed);
}

// ---------- §6.4 输出通道与配置视图 ----------

#[test]
fn prototype_outputs_have_no_simulator_wording() {
    let mut l = lab(Dialect::Ios);
    let ios_lines = [
        "enable",
        "configure terminal",
        "hostname R1",
        "interface g0/0",
        "ip address 192.168.1.1 255.255.255.0",
        "no shutdown",
        "exit",
        "ip route 192.168.2.0 255.255.255.0 10.0.0.2",
        "end",
        "show ip route",
        "show running-config",
        "ping 192.168.2.10",
    ];
    for line in ios_lines {
        let r = exec_and_run_result(&mut l.eng, &mut l.s1, l.r1, line);
        assert!(
            scan_simulator_wording(&r.device_output).is_empty(),
            "IOS 设备输出被模拟器措辞污染: {line} => {}",
            r.device_output
        );
        for n in &r.simulator_notices {
            assert!(n.starts_with("[netemu]"), "模拟器提示必须有明确前缀: {n}");
        }
    }
    let mut v = lab(Dialect::Vrp);
    let vrp_lines = [
        "system-view",
        "interface GigabitEthernet0/0/0",
        "ip address 192.168.1.1 24",
        "undo shutdown",
        "quit",
        "ip route-static 192.168.2.0 24 10.0.0.2",
        "return",
        "display ip routing-table",
        "display current-configuration",
        "ping 192.168.2.10",
        "save",
    ];
    for line in vrp_lines {
        let r = exec_and_run_result(&mut v.eng, &mut v.s1, v.r1, line);
        assert!(
            scan_simulator_wording(&r.device_output).is_empty(),
            "VRP 设备输出被模拟器措辞污染: {line} => {}",
            r.device_output
        );
    }
    // 旧的 `save` 伪成功文本与 OSPF 的 simulator 措辞必须彻底消失。
    let save = exec_line_result(&mut v.eng, &mut v.s1, v.r1, "save");
    assert!(!save.device_output.contains("no need to save"));
    assert!(!save.device_output.contains("simulator"));
}

#[test]
fn handler_limitations_go_to_notices_not_device_output() {
    // Stage 2B / C7：非零区域不再是一个"模拟器限制"，而是真实配置状态。
    // 本测试保留原有"限制只进 notices" 的双通道断言（改用仍未模拟的 save），
    // 并额外断言非零区域写入 Executed 且状态确实改变，属于收紧而非放宽。
    let mut l = lab(Dialect::Ios);
    run1(&mut l, &["enable", "configure terminal", "router ospf 7"]);
    let before = state_fingerprint(&l.eng);
    let r = exec_line_result(
        &mut l.eng,
        &mut l.s1,
        l.r1,
        "network 10.0.0.0 0.0.0.255 area 1",
    );
    assert_eq!(r.status, CommandStatus::Executed);
    assert!(r.state_changed);
    assert_ne!(state_fingerprint(&l.eng), before);
    assert_eq!(dev(&l.eng, l.r1).ospf_area, 1);
    assert_eq!(dev(&l.eng, l.r1).ospf_process, Some(7));
    assert!(scan_simulator_wording(&r.device_output).is_empty());

    let mut v = lab(Dialect::Vrp);
    run1(&mut v, &["system-view"]);
    let save = exec_line_result(&mut v.eng, &mut v.s1, v.r1, "save");
    assert_eq!(save.status, CommandStatus::NotSimulated);
    assert!(!save.state_changed);
    assert!(save.device_output.is_empty());
    assert!(scan_simulator_wording(&save.device_output).is_empty());
    assert!(save
        .simulator_notices
        .iter()
        .any(|n| n.contains("[netemu]") && n.contains("尚未模拟")));
}

#[test]
fn config_view_reflects_real_state_for_both_dialects() {
    let mut l = lab(Dialect::Ios);
    ios_configure(&mut l);
    let cfg = exec_and_run_result(&mut l.eng, &mut l.s1, l.r1, "show running-config");
    assert_eq!(cfg.status, CommandStatus::Executed);
    assert!(cfg.device_output.contains("interface GigabitEthernet0/0"));
    assert!(cfg
        .device_output
        .contains("ip address 192.168.1.1 255.255.255.0"));
    assert!(cfg.device_output.contains("ip route 192.168.2.0"));
    assert!(cfg.device_output.contains("10.0.0.2"));
    // 只针对已 no shutdown 的接口块断言；其余未使用端口保持 IOS 默认 down。
    let block = interface_block(&cfg.device_output, "GigabitEthernet0/0");
    assert!(
        !block.contains(" shutdown"),
        "g0/0 不应残留 shutdown: {block}"
    );

    run1(&mut l, &["configure terminal"]);
    run1(
        &mut l,
        &["no ip route 192.168.2.0 255.255.255.0 10.0.0.2", "end"],
    );
    let cfg = exec_and_run_result(&mut l.eng, &mut l.s1, l.r1, "show running-config");
    assert!(!cfg.device_output.contains("ip route 192.168.2.0"));

    let mut v = lab(Dialect::Vrp);
    vrp_configure(&mut v);
    let cfg = exec_and_run_result(&mut v.eng, &mut v.s1, v.r1, "display current-configuration");
    assert_eq!(cfg.status, CommandStatus::Executed);
    assert!(cfg.device_output.contains("interface GigabitEthernet0/0/0"));
    assert!(cfg.device_output.contains("ip address 192.168.1.1 24"));
    assert!(cfg.device_output.contains("ip route-static 192.168.2.0"));
    assert!(cfg.device_output.contains("10.0.0.2"));
}

// ---------- §6.5 lint、纯读检查 ----------

#[derive(Clone, Copy)]
struct TestRow {
    id: &'static str,
    shape: &'static str,
    strict: bool,
}
impl SpecKey for TestRow {
    fn id(&self) -> &str {
        self.id
    }
    fn shape(&self) -> &str {
        self.shape
    }
    fn strict(&self) -> bool {
        self.strict
    }
}

#[test]
fn lint_reports_full_migration_backlog_and_strict_coverage() {
    for (d, name) in [(Dialect::Ios, "IOS"), (Dialect::Vrp, "VRP")] {
        let rep: LintReport = specs::lint_report(d);
        assert!(rep.strict_ok(), "{name} 严格门禁失败: {rep:?}");
        assert!(rep.duplicates.is_empty(), "{name}: {rep:?}");
        assert!(rep.dangling.is_empty(), "{name}: {rep:?}");
        assert!(rep.drift.is_empty(), "{name}: {rep:?}");
        assert!(rep.strict_missing.is_empty(), "{name}: {rep:?}");
        assert_eq!(rep.strict_covered, rep.strict_total, "{name}: {rep:?}");
        assert!(rep.strict_total > 0);
        // 未关联的可执行节点全部显式列出，不设隐藏豁免。
        assert!(
            !rep.unassociated.is_empty(),
            "{name} 必须列出迁移清单: {rep:?}"
        );
        assert_eq!(
            rep.unassociated.len() + specs::rows(d).len(),
            rep.derived_total,
            "{name} 迁移清单 + 已关联必须等于派生总数"
        );
        let effects = specs::effect_counts(d);
        assert!(effects.iter().any(|(k, n)| *k == "state-change" && *n > 0));
        assert!(effects.iter().any(|(k, n)| *k == "query" && *n > 0));
    }
}

#[test]
fn lint_catches_deliberate_drift_dangling_duplicate_and_strict_gap() {
    let roots = roots_for(Dialect::Ios);
    // 故意制造：定义漂移（真实 ID + 错误结构指纹）、悬空 ID、重复 ID。
    let bad = [
        TestRow {
            id: "if/ip/address",
            shape: "lit:WRONG|p[]",
            strict: true,
        },
        TestRow {
            id: "exec/this-command-does-not-exist",
            shape: "lit:x|p[]",
            strict: true,
        },
        TestRow {
            id: "if/ip/address",
            shape: "lit:address|p[0.ipv4]",
            strict: true,
        },
    ];
    let rep = lint(
        &roots,
        &bad,
        &[
            "if/ip/address",
            "exec/this-command-does-not-exist",
            "priv/show/ip/route",
        ],
    );
    assert!(rep.drift.contains(&"if/ip/address".to_string()), "{rep:?}");
    assert!(
        rep.dangling
            .contains(&"exec/this-command-does-not-exist".to_string()),
        "{rep:?}"
    );
    assert!(
        rep.duplicates.contains(&"if/ip/address".to_string()),
        "{rep:?}"
    );
    assert!(
        rep.strict_missing
            .contains(&"priv/show/ip/route".to_string()),
        "{rep:?}"
    );
    assert!(!rep.strict_ok());

    // 字面量改变会改变派生 ID；模式改变同样会改变 ID。
    use cli_engine::spec::derive_commands;
    use cli_engine::CNode;
    fn noop(_: &mut cli_engine::Ctx) -> Result<String, cli_engine::CliError> {
        Ok(String::new())
    }
    let a = vec![CNode::lit("show", "show").kid(CNode::lit("route", "route").run(noop))];
    let b = vec![CNode::lit("show", "show").kid(CNode::lit("routes", "routes").run(noop))];
    let id_a = derive_commands(&[("exec", &a)])[0].id.clone();
    let id_b = derive_commands(&[("exec", &b)])[0].id.clone();
    let id_c = derive_commands(&[("priv", &a)])[0].id.clone();
    assert_ne!(id_a, id_b, "字面量漂移必须改变 ID");
    assert_ne!(id_a, id_c, "模式漂移必须改变 ID");
}

#[test]
fn pure_read_checks_are_side_effect_free() {
    let mut l = lab(Dialect::Ios);
    // 放入协议瞬态与虚拟时间，检查纯读路径不推进任何东西。
    l.eng.start_ping(l.pc1, ip("192.168.2.10"));
    let before = state_fingerprint(&l.eng);
    let now = l.eng.now_ns();
    let inflight = l.eng.in_flight();
    let dropped = l.eng.dropped;

    for _ in 0..3 {
        let p = inspect_command(profile_mod::IOS_ROUTER_PROFILE, Mode::Priv, "show ip route");
        assert_eq!(p.status, CommandStatus::Executed);
        assert!(p.detail.contains("平台级结果"));
        assert!(!p.detail.contains("profile="));

        let d = inspect_device_command(&l.eng, l.r1, &l.s1, "ping 10.0.0.2");
        assert_eq!(d.status, CommandStatus::Executed);
        assert!(d.detail.contains("profile=ios-xe-17-router"));
        assert!(d.detail.contains("mode=exec"));
        assert!(d.detail.contains("authenticated="));
        assert!(d.detail.contains("remote="));
        assert!(!d.detail.contains("平台级结果"));

        let bad = inspect_device_command(&l.eng, l.r1, &l.s1, "display ip routing-table");
        assert_eq!(bad.status, CommandStatus::InvalidCommand);
        assert_eq!(bad.support, SupportLabel::InvalidCommand);
    }
    assert_eq!(state_fingerprint(&l.eng), before, "纯读检查改变了设备状态");
    assert_eq!(l.eng.now_ns(), now, "纯读检查推进了虚拟时间");
    assert_eq!(l.eng.in_flight(), inflight, "纯读检查改变了事件队列");
    assert_eq!(l.eng.dropped, dropped, "纯读检查产生了探测包丢弃");
}

#[test]
fn device_level_check_requires_real_device_and_no_fake_fallback() {
    let l = lab(Dialect::Ios);
    let fake = inspect_device_command(&l.eng, 9999, &l.s1, "show ip route");
    assert_eq!(fake.status, CommandStatus::InvalidCommand);
    assert!(fake.detail.contains("设备不存在"));
    let unknown = inspect_command("not-a-profile", Mode::Exec, "show ip route");
    assert_eq!(unknown.status, CommandStatus::InvalidCommand);
    assert!(unknown.detail.contains("未知 Profile"));
    // 未关联规格的命令可以解析，但不得标为完整支持。
    let unassociated = inspect_command(
        profile_mod::IOS_ROUTER_PROFILE,
        Mode::Priv,
        "show ip interface brief",
    );
    assert_eq!(unassociated.status, CommandStatus::Executed);
    assert_eq!(unassociated.support, SupportLabel::PendingVerification);
    assert!(unassociated.detail.contains("迁移清单"));
}

#[test]
fn strict_ids_are_derived_from_the_side_table_not_duplicated() {
    for (d, _) in [(Dialect::Ios, "IOS"), (Dialect::Vrp, "VRP")] {
        let strict = specs::strict_ids(d);
        let rows = specs::rows(d);
        assert_eq!(strict.len(), rows.iter().filter(|r| r.strict).count());
        for id in &strict {
            assert!(rows.iter().any(|r| r.id == *id && r.strict));
            let s = specs::find(d, id).expect("严格 ID 必须能派生出 CommandSpec");
            assert_eq!(s.id, *id);
            assert!(s.strict);
            assert!(!s.contract.is_empty());
            assert!(!s.references.is_empty());
        }
    }
}
