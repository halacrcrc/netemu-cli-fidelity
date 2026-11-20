//! Stage 2C：H4（VRP BGP）与 H5（VRP ACL / Route-Policy / BGP 引入策略）闭环测试。
//!
//! 顺序与 `netemu-stage2c-implementation-handoff.md` 一致：H4 → H5 → 阶段 2 总收口。
//! 所有判据走真实 CLI → 引擎静默 → 输出渲染链；ping 经 `exec_and_run_result` 完整链，
//! 路由/会话经 `bgp_routes_of` / `bgp_sessions_of` 真实状态，不使用 parse 成功、命令返回
//! 成功或硬编码文本代替协议/数据面闭环。无带版本 VRP/eNSP 样本，输出只做字段/结构/状态
//! 断言，不写逐字黄金。失败路径用 `exec_line_result` + `state_fingerprint` 证明状态不变。

use cli_dialects::specs;
use cli_dialects::{
    complete_line, exec_and_run, exec_and_run_result, exec_line_result, inspect_command,
    inspect_device_command, prompt, scan_simulator_wording, state_fingerprint,
};
use cli_engine::spec::{CommandStatus, LintReport, SupportLabel};
use cli_engine::{Mode, Session};
use sim_core::device::{DeviceKind, Dialect};
use sim_core::engine::Engine;
use sim_core::profile as profile_mod;
use sim_core::Net;
use sim_packets::ipv4_from_str;

fn ip(s: &str) -> u32 {
    ipv4_from_str(s).unwrap()
}

fn ok(r: &cli_engine::spec::CommandResult, ctx: &str) {
    assert_eq!(
        r.status,
        CommandStatus::Executed,
        "{ctx}: {:?} / {}",
        r.status,
        r.device_output
    );
    assert!(
        scan_simulator_wording(&r.device_output).is_empty(),
        "{ctx}: 设备输出被模拟器措辞污染: {}",
        r.device_output
    );
}

fn dev_summary(eng: &Engine, dev: u32) -> sim_core::summary::DevSummary {
    eng.net
        .summary()
        .devices
        .into_iter()
        .find(|d| d.id == dev)
        .unwrap()
}

/// 查询统一在顶层视图执行（fresh Exec session），不受配置会话当前模式影响。
fn q(eng: &mut Engine, dev: u32, line: &str) -> cli_engine::spec::CommandResult {
    let mut s = Session::default();
    exec_and_run_result(eng, &mut s, dev, line)
}

fn config_view(eng: &mut Engine, dev: u32) -> String {
    q(eng, dev, "display current-configuration").device_output
}

fn converge(eng: &mut Engine) {
    for _ in 0..4 {
        let h = eng.drain_horizon();
        eng.run_to_quiescence(h);
    }
}

fn ping(eng: &mut Engine, host: u32, target: &str) -> String {
    exec_and_run_result(
        eng,
        &mut Session::default(),
        host,
        &format!("ping {target}"),
    )
    .device_output
}

fn assert_up(out: &str, ctx: &str) {
    assert!(out.contains("5 received"), "{ctx} 应全通: {out}");
}

fn assert_down(out: &str, ctx: &str) {
    assert!(out.contains("0 received"), "{ctx} 应全失败: {out}");
}

/// 是否学到某前缀（真实 BGP RIB，而非文本）。
fn has_route(eng: &Engine, dev: u32, net: &str, len: u8) -> bool {
    eng.bgp_routes_of(dev)
        .iter()
        .any(|(n, l, _, _, _)| *n == ip(net) && *l == len)
}

/// PC1—R1—R2—PC2，R1/R2 互联 10.0.0.0/24，LAN 192.168.1.0/24 与 192.168.2.0/24。
fn bgp_lab() -> (Engine, u32, u32, u32, u32, Session, Session) {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Vrp);
    let r2 = net.add_device("R2".into(), DeviceKind::Router, Dialect::Vrp);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc1, r1).unwrap();
    net.add_link(r1, r2).unwrap();
    net.add_link(r2, pc2).unwrap();
    net.set_host(pc1, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    net.set_host(pc2, Some(ip("192.168.2.10")), 24, Some(ip("192.168.2.1")));
    for (dev, p0, p1) in [
        (r1, "192.168.1.1", "10.0.0.1"),
        (r2, "10.0.0.2", "192.168.2.1"),
    ] {
        net.set_port_admin(dev, 0, true);
        net.set_port_ip(dev, 0, Some(ip(p0)), 24);
        net.set_port_admin(dev, 1, true);
        net.set_port_ip(dev, 1, Some(ip(p1)), 24);
    }
    (
        Engine::new(net),
        pc1,
        r1,
        r2,
        pc2,
        Session::default(),
        Session::default(),
    )
}

/// 经真实 CLI 建立 eBGP：R1 AS65001 宣告 192.168.1.0，R2 AS65002 宣告 192.168.2.0。
/// 结束后两个会话停留在各自 BGP 视图，便于后续 undo。
fn bgp_base(eng: &mut Engine, r1: u32, r2: u32, s1: &mut Session, s2: &mut Session) {
    for l in [
        "system-view",
        "bgp 65001",
        "peer 10.0.0.2 as-number 65002",
        "network 192.168.1.0 255.255.255.0",
    ] {
        ok(&exec_and_run_result(eng, s1, r1, l), &format!("H4 R1 {l}"));
    }
    for l in [
        "system-view",
        "bgp 65002",
        "peer 10.0.0.1 as-number 65001",
        "network 192.168.2.0 255.255.255.0",
    ] {
        ok(&exec_and_run_result(eng, s2, r2, l), &format!("H4 R2 {l}"));
    }
}

// =====================================================================
// L4 场景清单：本包显式承诺的正/负数据面场景，每个条目在下方有可执行断言。
// 映射：H4-P1→h4_ebgp_cross_as_adjacency_route_and_ping
//       H4-N1→h4_wrong_as_no_neighbor_unannounced_are_unreachable
//       H4-N2→h4_undo_network_peer_bgp_withdraw_and_state_unchanged
//       H4-R1→h4_link_failure_withdraw_and_recovery
//       H5-P1→h5_policy_permit_prefix_route_and_ping
//       H5-N1→h5_policy_deny_prefix_no_route_ping_fail
//       H5-N2→h5_implicit_deny_unmatched_default
//       H5-R1→h5_undo_node_unbind_recompute_and_recovery
//       H5-A1→h5_local_preference_changes_real_path_selection
// =====================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum L4Outcome {
    Reachable,
    Unreachable,
}

#[derive(Clone, Copy, Debug)]
struct L4Scenario {
    id: &'static str,
    flow: &'static str,
    #[allow(dead_code)]
    description: &'static str,
    outcome: L4Outcome,
}

static STAGE2C_L4: &[L4Scenario] = &[
    L4Scenario {
        id: "L4-H4-P1-ebgp-cross-as-ping-ok",
        flow: "H4",
        description: "VRP 双向 eBGP 建邻并互相宣告 LAN，跨 AS PC1 ping PC2 全通",
        outcome: L4Outcome::Reachable,
    },
    L4Scenario {
        id: "L4-H4-N1-wrong-as-no-adjacency-fail",
        flow: "H4",
        description: "对端 AS 不匹配/无邻居/未宣告前缀时不建邻或无路由，ping 全失败",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-H4-N2-undo-withdraw-fail",
        flow: "H4",
        description: "undo network/peer/bgp 撤销宣告与邻居后路由消失，ping 全失败",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-H4-R1-link-fail-recover-ok",
        flow: "H4",
        description: "互联链路 shutdown 会话老化撤销路由业务失败，恢复后重建邻业务全通",
        outcome: L4Outcome::Reachable,
    },
    L4Scenario {
        id: "L4-H5-P1-policy-permit-route-ping-ok",
        flow: "H5",
        description: "route-policy 放行前缀经 BGP 引入生成路由，被放行业务 ping 全通",
        outcome: L4Outcome::Reachable,
    },
    L4Scenario {
        id: "L4-H5-N1-policy-deny-no-route-fail",
        flow: "H5",
        description: "route-policy deny 命中前缀不引入路由，被拒绝业务 ping 全失败",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-H5-N2-implicit-deny-default-fail",
        flow: "H5",
        description: "无节点命中的前缀按隐式拒绝过滤，未匹配默认业务 ping 全失败",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-H5-R1-undo-node-unbind-recover-ok",
        flow: "H5",
        description: "undo node/解绑/undo acl 触发重算，被放行前缀路由与业务恢复",
        outcome: L4Outcome::Reachable,
    },
    L4Scenario {
        id: "L4-H5-A1-local-pref-path-selection",
        flow: "H5",
        description: "apply local-preference 改变真实优选下一跳（非仅配置文本），改绑后选路翻转",
        outcome: L4Outcome::Reachable,
    },
];

#[test]
fn stage2c_l4_registry_is_explicit_and_nonempty() {
    assert_eq!(STAGE2C_L4.len(), 9);
    for f in ["H4", "H5"] {
        let pos = STAGE2C_L4
            .iter()
            .any(|s| s.flow == f && s.outcome == L4Outcome::Reachable);
        let neg = STAGE2C_L4
            .iter()
            .any(|s| s.flow == f && s.outcome == L4Outcome::Unreachable);
        assert!(pos && neg, "{f} 必须同时登记正、负 L4 场景");
    }
    //  handoff 要求的最小场景 ID 必须全部登记。
    for id in [
        "L4-H4-P1-ebgp-cross-as-ping-ok",
        "L4-H4-N1-wrong-as-no-adjacency-fail",
        "L4-H4-N2-undo-withdraw-fail",
        "L4-H4-R1-link-fail-recover-ok",
        "L4-H5-P1-policy-permit-route-ping-ok",
        "L4-H5-N1-policy-deny-no-route-fail",
        "L4-H5-N2-implicit-deny-default-fail",
        "L4-H5-R1-undo-node-unbind-recover-ok",
        "L4-H5-A1-local-pref-path-selection",
    ] {
        assert!(
            STAGE2C_L4.iter().any(|s| s.id == id),
            "缺少最小 L4 场景 {id}"
        );
    }
    let mut ids: Vec<&str> = STAGE2C_L4.iter().map(|s| s.id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "L4 场景 ID 不得重复");
}

// =====================================================================
// H4：VRP BGP
// =====================================================================

#[test]
fn h4_ebgp_cross_as_adjacency_route_and_ping() {
    let (mut eng, pc1, r1, r2, pc2, mut s1, mut s2) = bgp_lab();
    bgp_base(&mut eng, r1, r2, &mut s1, &mut s2);
    assert_eq!(prompt(&eng, &s1, r1), "[R1-bgp-65001]", "BGP 视图提示符");
    converge(&mut eng);

    // 会话：双向 Established，对端 AS 来自真实配置。
    let sess1 = eng.bgp_sessions_of(r1);
    assert!(
        sess1
            .iter()
            .any(|(n, a, est)| *n == ip("10.0.0.2") && *a == 65002 && *est),
        "R1 应与 R2 建邻: {sess1:?}"
    );
    let peer = q(&mut eng, r1, "display bgp peer");
    ok(&peer, "H4 display bgp peer");
    assert!(
        peer.device_output.contains("65002"),
        "{}",
        peer.device_output
    );
    assert!(
        peer.device_output.contains("Established"),
        "{}",
        peer.device_output
    );

    // 路由：R1 学到 R2 的 LAN，R2 学到 R1 的 LAN（真实 RIB）。
    assert!(
        has_route(&eng, r1, "192.168.2.0", 24),
        "R1 应学到 192.168.2.0/24"
    );
    assert!(
        has_route(&eng, r2, "192.168.1.0", 24),
        "R2 应学到 192.168.1.0/24"
    );
    let table = q(&mut eng, r1, "display bgp routing-table");
    ok(&table, "H4 display bgp routing-table");
    assert!(
        table.device_output.contains("192.168.2.0/24"),
        "{}",
        table.device_output
    );

    // 正向 L4：跨 AS 双向 ping 全通（真实数据面）。
    assert_up(&ping(&mut eng, pc1, "192.168.2.10"), "H4-P1 PC1→PC2");
    assert_up(&ping(&mut eng, pc2, "192.168.1.10"), "H4-P1 PC2→PC1");

    // 配置视图从真实状态渲染：本端 AS/邻居 AS/宣告前缀，无硬编码。
    let cfg = config_view(&mut eng, r1);
    assert!(cfg.contains("bgp 65001"), "{cfg}");
    assert!(cfg.contains("peer 10.0.0.2 as-number 65002"), "{cfg}");
    assert!(cfg.contains("network 192.168.1.0 255.255.255.0"), "{cfg}");
    assert!(
        !cfg.contains("neighbor 10.0.0.2 remote-as"),
        "VRP 不得套用 IOS 用词: {cfg}"
    );
}

#[test]
fn h4_wrong_as_no_neighbor_unannounced_are_unreachable() {
    // 负向 A：对端 AS 不匹配 → R1 拒绝 Open，不建邻、无路由、不可达。
    {
        let (mut eng, pc1, r1, r2, _pc2, mut s1, mut s2) = bgp_lab();
        for l in [
            "system-view",
            "bgp 65001",
            "peer 10.0.0.2 as-number 65099", // 错误：R2 实际 AS 为 65002
            "network 192.168.1.0 255.255.255.0",
        ] {
            ok(
                &exec_and_run_result(&mut eng, &mut s1, r1, l),
                &format!("H4-N1 A {l}"),
            );
        }
        for l in [
            "system-view",
            "bgp 65002",
            "peer 10.0.0.1 as-number 65001",
            "network 192.168.2.0 255.255.255.0",
        ] {
            ok(
                &exec_and_run_result(&mut eng, &mut s2, r2, l),
                &format!("H4-N1 A {l}"),
            );
        }
        converge(&mut eng);
        assert!(
            eng.bgp_sessions_of(r1).iter().all(|(_, _, est)| !*est),
            "AS 不匹配 R1 不得建邻: {:?}",
            eng.bgp_sessions_of(r1)
        );
        assert!(
            !has_route(&eng, r1, "192.168.2.0", 24),
            "AS 不匹配不得学到路由"
        );
        let peer = q(&mut eng, r1, "display bgp peer");
        assert!(
            peer.device_output.contains("Idle"),
            "{}",
            peer.device_output
        );
        assert!(
            !peer.device_output.contains("Established"),
            "{}",
            peer.device_output
        );
        assert_down(&ping(&mut eng, pc1, "192.168.2.10"), "H4-N1 错 AS");
    }
    // 负向 B：对端完全无 BGP → 无邻居响应，不建邻、不可达。
    {
        let (mut eng, pc1, r1, _r2, _pc2, mut s1, _s2) = bgp_lab();
        for l in [
            "system-view",
            "bgp 65001",
            "peer 10.0.0.2 as-number 65002",
            "network 192.168.1.0 255.255.255.0",
        ] {
            ok(
                &exec_and_run_result(&mut eng, &mut s1, r1, l),
                &format!("H4-N1 B {l}"),
            );
        }
        converge(&mut eng);
        assert!(
            eng.bgp_sessions_of(r1).iter().all(|(_, _, est)| !*est),
            "无邻居不得建邻: {:?}",
            eng.bgp_sessions_of(r1)
        );
        assert!(!has_route(&eng, r1, "192.168.2.0", 24));
        assert_down(&ping(&mut eng, pc1, "192.168.2.10"), "H4-N1 无邻居");
    }
    // 负向 C：建邻成功但对端未宣告前缀 → 有会话无路由，仍不可达。
    {
        let (mut eng, pc1, r1, r2, _pc2, mut s1, mut s2) = bgp_lab();
        for l in [
            "system-view",
            "bgp 65001",
            "peer 10.0.0.2 as-number 65002",
            "network 192.168.1.0 255.255.255.0",
        ] {
            ok(
                &exec_and_run_result(&mut eng, &mut s1, r1, l),
                &format!("H4-N1 C {l}"),
            );
        }
        for l in ["system-view", "bgp 65002", "peer 10.0.0.1 as-number 65001"] {
            ok(
                &exec_and_run_result(&mut eng, &mut s2, r2, l),
                &format!("H4-N1 C {l}"),
            );
        }
        converge(&mut eng);
        assert!(
            eng.bgp_sessions_of(r1).iter().any(|(_, _, est)| *est),
            "未宣告前缀仍应建邻: {:?}",
            eng.bgp_sessions_of(r1)
        );
        assert!(
            !has_route(&eng, r1, "192.168.2.0", 24),
            "未宣告前缀不得有路由"
        );
        assert_down(&ping(&mut eng, pc1, "192.168.2.10"), "H4-N1 未宣告前缀");
    }
}

#[test]
fn h4_undo_network_peer_bgp_withdraw_and_state_unchanged() {
    let (mut eng, pc1, r1, r2, _pc2, mut s1, mut s2) = bgp_lab();
    bgp_base(&mut eng, r1, r2, &mut s1, &mut s2);
    converge(&mut eng);
    assert_up(&ping(&mut eng, pc1, "192.168.2.10"), "H4-N2 撤销前基线");

    // A：R2 undo network → R1 经下一轮 Update 全表刷新撤销该前缀。
    ok(
        &exec_and_run_result(
            &mut eng,
            &mut s2,
            r2,
            "undo network 192.168.2.0 255.255.255.0",
        ),
        "H4-N2 undo network",
    );
    converge(&mut eng);
    assert!(
        !has_route(&eng, r1, "192.168.2.0", 24),
        "undo network 后 R1 路由须撤销"
    );
    let table = q(&mut eng, r1, "display bgp routing-table");
    assert!(
        !table.device_output.contains("192.168.2.0/24"),
        "{}",
        table.device_output
    );
    assert_down(
        &ping(&mut eng, pc1, "192.168.2.10"),
        "H4-N2 undo network 后业务",
    );
    // 重复撤销 / 掩码不匹配：拒绝且状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(
        &mut eng,
        &mut s2,
        r2,
        "undo network 192.168.2.0 255.255.255.0",
    );
    assert_eq!(
        bad.status,
        CommandStatus::InvalidCommand,
        "重复 undo network 须拒绝"
    );
    assert_eq!(state_fingerprint(&eng), before);
    let bad = exec_line_result(
        &mut eng,
        &mut s2,
        r2,
        "undo network 192.168.2.0 255.255.0.0",
    );
    assert_eq!(
        bad.status,
        CommandStatus::InvalidCommand,
        "掩码不匹配须拒绝"
    );
    assert_eq!(state_fingerprint(&eng), before);

    // B：重新宣告 → 路由与业务恢复（证明撤销可逆）。
    ok(
        &exec_and_run_result(&mut eng, &mut s2, r2, "network 192.168.2.0 255.255.255.0"),
        "H4-N2 重新宣告",
    );
    converge(&mut eng);
    assert!(
        has_route(&eng, r1, "192.168.2.0", 24),
        "重新宣告后路由须恢复"
    );
    assert_up(&ping(&mut eng, pc1, "192.168.2.10"), "H4-N2 重新宣告后业务");

    // C：R2 undo peer → 邻居删除，R1 会话老化、路由撤销。
    ok(
        &exec_and_run_result(&mut eng, &mut s2, r2, "undo peer 10.0.0.1"),
        "H4-N2 undo peer",
    );
    converge(&mut eng);
    assert!(
        eng.bgp_sessions_of(r1).iter().all(|(_, _, est)| !*est),
        "undo peer 后 R1 会话须撤销: {:?}",
        eng.bgp_sessions_of(r1)
    );
    assert!(
        !has_route(&eng, r1, "192.168.2.0", 24),
        "undo peer 后路由须撤销"
    );
    assert_down(
        &ping(&mut eng, pc1, "192.168.2.10"),
        "H4-N2 undo peer 后业务",
    );
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(&mut eng, &mut s2, r2, "undo peer 10.0.0.1");
    assert_eq!(
        bad.status,
        CommandStatus::InvalidCommand,
        "重复 undo peer 须拒绝"
    );
    assert_eq!(state_fingerprint(&eng), before);

    // D：R1 undo bgp（配置视图）→ 进程删除，会话/路由清空，查询反映真实状态。
    let mut sc = Session::default();
    ok(
        &exec_and_run_result(&mut eng, &mut sc, r1, "system-view"),
        "H4-N2 system-view",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut sc, r1, "undo bgp 65001"),
        "H4-N2 undo bgp",
    );
    converge(&mut eng);
    assert_eq!(
        dev_summary(&eng, r1).bgp_asn,
        None,
        "undo bgp 后本端 AS 须清空"
    );
    assert!(eng.bgp_sessions_of(r1).is_empty(), "undo bgp 后会话须清空");
    assert!(eng.bgp_routes_of(r1).is_empty(), "undo bgp 后路由须清空");
    let peer = q(&mut eng, r1, "display bgp peer");
    assert!(
        peer.device_output.contains("BGP is not configured"),
        "{}",
        peer.device_output
    );
    let cfg = config_view(&mut eng, r1);
    assert!(
        !cfg.contains("bgp 65001"),
        "undo bgp 后配置视图不得残留: {cfg}"
    );
    // 错误 AS / 重复 undo bgp：拒绝且状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(&mut eng, &mut sc, r1, "undo bgp 65099");
    assert_eq!(
        bad.status,
        CommandStatus::InvalidCommand,
        "错误 AS undo bgp 须拒绝"
    );
    assert_eq!(state_fingerprint(&eng), before);
    let bad = exec_line_result(&mut eng, &mut sc, r1, "undo bgp 65001");
    assert_eq!(
        bad.status,
        CommandStatus::InvalidCommand,
        "重复 undo bgp 须拒绝"
    );
    assert_eq!(state_fingerprint(&eng), before);
}

#[test]
fn h4_link_failure_withdraw_and_recovery() {
    let (mut eng, pc1, r1, r2, _pc2, mut s1, mut s2) = bgp_lab();
    bgp_base(&mut eng, r1, r2, &mut s1, &mut s2);
    converge(&mut eng);
    assert_up(&ping(&mut eng, pc1, "192.168.2.10"), "H4-R1 故障前基线");

    // 互联口 shutdown：keepalive 无法送达，hold  timer 老化撤销会话与路由。
    let mut si = Session::default();
    for l in ["system-view", "interface g0/0/1", "shutdown"] {
        ok(
            &exec_and_run_result(&mut eng, &mut si, r1, l),
            &format!("H4-R1 {l}"),
        );
    }
    converge(&mut eng);
    assert!(
        eng.bgp_sessions_of(r1).iter().all(|(_, _, est)| !*est),
        "链路故障后 R1 会话须老化: {:?}",
        eng.bgp_sessions_of(r1)
    );
    assert!(
        !has_route(&eng, r1, "192.168.2.0", 24),
        "链路故障后路由须撤销"
    );
    assert_down(&ping(&mut eng, pc1, "192.168.2.10"), "H4-R1 链路故障业务");

    // 恢复链路：重新建邻、路由恢复、业务全通。
    ok(
        &exec_and_run_result(&mut eng, &mut si, r1, "undo shutdown"),
        "H4-R1 undo shutdown",
    );
    converge(&mut eng);
    assert!(
        eng.bgp_sessions_of(r1)
            .iter()
            .any(|(n, _, est)| *n == ip("10.0.0.2") && *est),
        "恢复后须重建邻: {:?}",
        eng.bgp_sessions_of(r1)
    );
    assert!(has_route(&eng, r1, "192.168.2.0", 24), "恢复后路由须重建");
    assert_up(&ping(&mut eng, pc1, "192.168.2.10"), "H4-R1 恢复后业务");
}

#[test]
fn h4_config_render_uses_real_as_neighbor_and_prefix() {
    // 用非默认 AS 与额外前缀证明渲染取真实状态，而非硬编码 65001/192.168.1.0。
    let (mut eng, _pc1, r1, r2, _pc2, mut s1, mut s2) = bgp_lab();
    for l in [
        "system-view",
        "bgp 65010",
        "peer 10.0.0.2 as-number 65020",
        "network 192.168.1.0 255.255.255.0",
        "network 10.9.0.0 255.255.255.0",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s1, r1, l),
            &format!("H4 render {l}"),
        );
    }
    for l in ["system-view", "bgp 65020", "peer 10.0.0.1 as-number 65010"] {
        ok(
            &exec_and_run_result(&mut eng, &mut s2, r2, l),
            &format!("H4 render R2 {l}"),
        );
    }
    converge(&mut eng);
    let cfg = config_view(&mut eng, r1);
    assert!(cfg.contains("bgp 65010"), "{cfg}");
    assert!(cfg.contains("peer 10.0.0.2 as-number 65020"), "{cfg}");
    assert!(cfg.contains("network 192.168.1.0 255.255.255.0"), "{cfg}");
    assert!(cfg.contains("network 10.9.0.0 255.255.255.0"), "{cfg}");
    assert!(!cfg.contains("bgp 65001"), "不得硬编码默认 AS: {cfg}");
}

#[test]
fn h4_h5_syntax_help_completion_abbreviation_from_cnode() {
    let (mut eng, _pc1, r1, r2, _pc2, mut s1, mut s2) = bgp_lab();
    bgp_base(&mut eng, r1, r2, &mut s1, &mut s2);
    converge(&mut eng);

    // 补全：BGP 视图 peer <ip> 的下一跳字面量来自 CNode。
    let (_, cands) = complete_line(&eng, &s1, r1, "peer 10.0.0.2 ");
    assert!(cands.contains(&"as-number".to_string()), "{cands:?}");
    assert!(cands.contains(&"route-policy".to_string()), "{cands:?}");
    let (_, uc) = complete_line(&eng, &s1, r1, "undo ");
    assert!(uc.contains(&"peer".to_string()), "{uc:?}");
    assert!(uc.contains(&"network".to_string()), "{uc:?}");

    // 帮助：'?' 纯读，列出 CNode 帮助文本，不改状态、不推进时间。
    let before = state_fingerprint(&eng);
    let now = eng.now_ns();
    let h = exec_line_result(&mut eng, &mut s1, r1, "peer 10.0.0.2 ?");
    assert_eq!(h.status, CommandStatus::Executed);
    assert!(h.device_output.contains("as-number"), "{}", h.device_output);
    assert!(
        h.device_output.contains("route-policy"),
        "{}",
        h.device_output
    );
    assert!(!h.state_changed);
    assert_eq!(state_fingerprint(&eng), before);
    assert_eq!(eng.now_ns(), now);

    // route-policy 视图补全：if-match / apply / undo 均来自 CNode。
    let mut sp = Session::default();
    for l in ["system-view", "route-policy P permit node 10"] {
        ok(
            &exec_and_run_result(&mut eng, &mut sp, r1, l),
            &format!("H5 syntax {l}"),
        );
    }
    assert_eq!(prompt(&eng, &sp, r1), "[R1-route-policy]");
    let (_, rc) = complete_line(&eng, &sp, r1, " ");
    assert!(rc.contains(&"if-match".to_string()), "{rc:?}");
    assert!(rc.contains(&"apply".to_string()), "{rc:?}");
    assert!(rc.contains(&"undo".to_string()), "{rc:?}");

    // 唯一缩写与全称等价（display bgp p == display bgp peer）。
    let mut se = Session::default();
    let full = exec_and_run_result(&mut eng, &mut se, r1, "display bgp peer");
    let abbr = exec_and_run_result(&mut eng, &mut se, r1, "display bgp p");
    assert_eq!(full.status, CommandStatus::Executed);
    assert_eq!(abbr.status, CommandStatus::Executed);
    assert_eq!(
        full.device_output, abbr.device_output,
        "唯一缩写应与全称等价"
    );
    assert!(full.device_output.contains("Established"));
}

/// VRP 终端 `?` 上下文帮助：候选一律来自**当前会话模式对应的 CNode**，纯读、不执行原命令。
///
/// 本测试守护前端 `?` 修复的服务端契约（前端按「原始输入 + `?`」发起请求）：
/// `?` 在 `exec_line_result` 里先于解析被截获，因此 `system-view?` 只出帮助、不切视图；
/// 帮助不写状态、不推进虚拟时间、不混入模拟器提示通道。断言只做字段/结构/状态级，
/// 不断言逐字空格与列宽（无带版本 VRP 设备样本）。
#[test]
fn vrp_context_help_user_and_system_views_from_cnode() {
    let mut net = Net::new();
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Vrp);
    let mut eng = Engine::new(net);
    let mut s = Session::default();
    assert_eq!(s.mode, Mode::Exec, "新会话应处于 VRP 用户视图");
    let before = state_fingerprint(&eng);
    let now = eng.now_ns();

    // 1) 用户视图单按 `?`：列出根候选，纯读。
    let h = exec_line_result(&mut eng, &mut s, r1, "?");
    assert_eq!(h.status, CommandStatus::Executed, "{}", h.device_output);
    for w in ["system-view", "display", "ping"] {
        assert!(
            h.device_output.contains(w),
            "用户视图 `?` 缺候选 {w}: {}",
            h.device_output
        );
    }
    assert!(!h.state_changed, "帮助不得改变状态");
    assert_eq!(state_fingerprint(&eng), before, "帮助不得改变配置/拓扑");
    assert_eq!(eng.now_ns(), now, "帮助不得推进虚拟时间");
    assert!(
        scan_simulator_wording(&h.device_output).is_empty(),
        "帮助文本混入模拟器措辞: {}",
        h.device_output
    );
    assert!(
        !h.device_output.contains("[netemu]"),
        "帮助文本不得伪造模拟器提示: {}",
        h.device_output
    );

    // 2) 用户视图 `display ?`：列出 display 子节点，而不是当作普通命令解析。
    let h = exec_line_result(&mut eng, &mut s, r1, "display ?");
    assert_eq!(h.status, CommandStatus::Executed, "{}", h.device_output);
    assert!(
        h.device_output.contains("current-configuration"),
        "`display ?` 应列出既有子命令: {}",
        h.device_output
    );
    assert!(
        !h.device_output.contains("Error"),
        "`display ?` 不得落到命令错误: {}",
        h.device_output
    );
    assert!(!h.state_changed);

    // 3) 用户视图 `dis?`：前缀帮助，不执行、不补全。
    let h = exec_line_result(&mut eng, &mut s, r1, "dis?");
    assert_eq!(h.status, CommandStatus::Executed, "{}", h.device_output);
    assert!(
        h.device_output.contains("display"),
        "`dis?` 应给前缀候选: {}",
        h.device_output
    );
    assert_eq!(s.mode, Mode::Exec, "帮助请求不得切换视图");
    assert_eq!(state_fingerprint(&eng), before);

    // 4) 用户视图 `system-view?`：只显示帮助，绝不进入系统视图。
    let h = exec_line_result(&mut eng, &mut s, r1, "system-view?");
    assert_eq!(h.status, CommandStatus::Executed, "{}", h.device_output);
    assert!(
        h.device_output.contains("system-view"),
        "`system-view?` 应给前缀候选: {}",
        h.device_output
    );
    assert_eq!(s.mode, Mode::Exec, "`system-view?` 不得进入 Mode::Config");
    assert!(!h.state_changed);
    assert_eq!(state_fingerprint(&eng), before, "帮助不得改变配置");
    assert_eq!(eng.now_ns(), now);

    // 5) 正常进入系统视图后单按 `?`：候选来自 config 根 CNode。
    let r = exec_and_run_result(&mut eng, &mut s, r1, "system-view");
    ok(&r, "进入 VRP 系统视图");
    assert_eq!(s.mode, Mode::Config);
    let cfg_before = state_fingerprint(&eng);
    let h = exec_line_result(&mut eng, &mut s, r1, "?");
    assert_eq!(h.status, CommandStatus::Executed, "{}", h.device_output);
    for w in ["sysname", "interface", "undo"] {
        assert!(
            h.device_output.contains(w),
            "系统视图 `?` 缺候选 {w}: {}",
            h.device_output
        );
    }
    assert!(!h.state_changed);
    assert_eq!(state_fingerprint(&eng), cfg_before);
    assert!(
        scan_simulator_wording(&h.device_output).is_empty(),
        "系统视图帮助混入模拟器措辞: {}",
        h.device_output
    );

    // 6) 系统视图 `display ?` / `display c?`：同一棵 CNode，前缀形式同样可用。
    let h = exec_line_result(&mut eng, &mut s, r1, "display ?");
    assert_eq!(h.status, CommandStatus::Executed, "{}", h.device_output);
    assert!(
        h.device_output.contains("current-configuration"),
        "系统视图 `display ?` 应列出子命令: {}",
        h.device_output
    );
    let hf = exec_line_result(&mut eng, &mut s, r1, "display c?");
    assert_eq!(hf.status, CommandStatus::Executed, "{}", hf.device_output);
    assert!(
        hf.device_output.contains("current-configuration"),
        "系统视图 `display c?` 应给前缀候选: {}",
        hf.device_output
    );
    assert_eq!(state_fingerprint(&eng), cfg_before, "帮助不得改变配置");
}

// =====================================================================
// H5：VRP ACL / Route-Policy / BGP 引入策略
// =====================================================================

/// PC1—R1—R2—PC2，R1 另接 SRV(172.16.9.0/24)。R1 宣告 192.168.1.0 与 172.16.9.0。
fn policy_lab() -> (Engine, u32, u32, u32, u32, u32, Session, Session) {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Vrp);
    let r2 = net.add_device("R2".into(), DeviceKind::Router, Dialect::Vrp);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    let srv = net.add_device("SRV".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc1, r1).unwrap();
    net.add_link(r1, r2).unwrap();
    net.add_link(r2, pc2).unwrap();
    net.add_link(r1, srv).unwrap();
    net.set_host(pc1, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    net.set_host(pc2, Some(ip("192.168.2.10")), 24, Some(ip("192.168.2.1")));
    net.set_host(srv, Some(ip("172.16.9.10")), 24, Some(ip("172.16.9.1")));
    for (dev, port, addr) in [
        (r1, 0, "192.168.1.1"),
        (r1, 1, "10.0.0.1"),
        (r1, 2, "172.16.9.1"),
        (r2, 0, "10.0.0.2"),
        (r2, 1, "192.168.2.1"),
    ] {
        net.set_port_admin(dev, port, true);
        net.set_port_ip(dev, port, Some(ip(addr)), 24);
    }
    (
        Engine::new(net),
        pc1,
        r1,
        r2,
        pc2,
        srv,
        Session::default(),
        Session::default(),
    )
}

/// R1/R2 基础 eBGP，R1 宣告 192.168.1.0 与 172.16.9.0，R2 宣告 192.168.2.0。
fn policy_base(eng: &mut Engine, r1: u32, r2: u32, s1: &mut Session, s2: &mut Session) {
    for l in [
        "system-view",
        "bgp 65001",
        "peer 10.0.0.2 as-number 65002",
        "network 192.168.1.0 255.255.255.0",
        "network 172.16.9.0 255.255.255.0",
    ] {
        ok(&exec_and_run_result(eng, s1, r1, l), &format!("H5 R1 {l}"));
    }
    for l in [
        "system-view",
        "bgp 65002",
        "peer 10.0.0.1 as-number 65001",
        "network 192.168.2.0 255.255.255.0",
    ] {
        ok(&exec_and_run_result(eng, s2, r2, l), &format!("H5 R2 {l}"));
    }
}

/// 在 R2 建立 ACL2000（permit 172.16.9.0）与策略，并把策略绑定到 R1 入向。
/// 前置：s2 停留在 BGP 视图（policy_base 之后）；先 quit 回配置视图再建 ACL/策略。
/// 结束后 s2 停留在 BGP 视图。
fn policy_bind(
    eng: &mut Engine,
    r2: u32,
    s2: &mut Session,
    policy: &str,
    node10_permit: bool,
    node20: bool,
) {
    let action = if node10_permit { "permit" } else { "deny" };
    let mut lines: Vec<String> = vec![
        "quit".into(),
        "acl 2000".into(),
        "rule permit source 172.16.9.0 0.0.0.255".into(),
        "quit".into(),
        format!("route-policy {policy} {action} node 10"),
        "if-match acl 2000".into(),
        "quit".into(),
    ];
    if node20 {
        lines.push(format!("route-policy {policy} permit node 20"));
        lines.push("quit".into());
    }
    lines.push("bgp 65002".into());
    lines.push(format!("peer 10.0.0.1 route-policy {policy} import"));
    for l in lines {
        ok(
            &exec_and_run_result(eng, s2, r2, &l),
            &format!("H5 bind {l}"),
        );
    }
}

#[test]
fn h5_policy_permit_prefix_route_and_ping() {
    let (mut eng, _pc1, r1, r2, pc2, _srv, mut s1, mut s2) = policy_lab();
    policy_base(&mut eng, r1, r2, &mut s1, &mut s2);
    // ALLOW：node10 permit 命中 172.16.9.0，node20 permit 兜底其余前缀。
    policy_bind(&mut eng, r2, &mut s2, "ALLOW", true, true);
    converge(&mut eng);

    // 被放行前缀进入真实 RIB，且其余前缀经兜底节点也放行。
    assert!(
        has_route(&eng, r2, "172.16.9.0", 24),
        "放行前缀须引入: {:?}",
        eng.bgp_routes_of(r2)
    );
    assert!(has_route(&eng, r2, "192.168.1.0", 24), "兜底放行前缀须引入");
    let table = q(&mut eng, r2, "display bgp routing-table");
    assert!(
        table.device_output.contains("172.16.9.0/24"),
        "{}",
        table.device_output
    );

    // 正向 L4：被放行业务 ping 全通。
    assert_up(&ping(&mut eng, pc2, "172.16.9.10"), "H5-P1 PC2→SRV");
    assert_up(&ping(&mut eng, pc2, "192.168.1.10"), "H5-P1 PC2→PC1");

    // 策略查询与配置视图取真实状态。
    let shown = q(&mut eng, r2, "display route-policy");
    ok(&shown, "H5 display route-policy");
    assert!(
        shown.device_output.contains("Route-policy: ALLOW"),
        "{}",
        shown.device_output
    );
    assert!(
        shown.device_output.contains("node 10 permit"),
        "{}",
        shown.device_output
    );
    assert!(
        shown.device_output.contains("if-match acl 2000"),
        "{}",
        shown.device_output
    );
    let acl = q(&mut eng, r2, "display acl all");
    ok(&acl, "H5 display acl all");
    assert!(
        acl.device_output.contains("ACL number: 2000"),
        "{}",
        acl.device_output
    );
    assert!(
        acl.device_output
            .contains("rule 5 permit source 172.16.9.0"),
        "{}",
        acl.device_output
    );
    let cfg = config_view(&mut eng, r2);
    assert!(cfg.contains("route-policy ALLOW permit node 10"), "{cfg}");
    assert!(cfg.contains("if-match acl 2000"), "{cfg}");
    assert!(
        cfg.contains("peer 10.0.0.1 route-policy ALLOW import"),
        "{cfg}"
    );
}

#[test]
fn h5_policy_deny_prefix_no_route_ping_fail() {
    let (mut eng, _pc1, r1, r2, pc2, _srv, mut s1, mut s2) = policy_lab();
    policy_base(&mut eng, r1, r2, &mut s1, &mut s2);
    // FILTER：node10 deny 命中 172.16.9.0，node20 permit 兜底其余。
    policy_bind(&mut eng, r2, &mut s2, "FILTER", false, true);
    converge(&mut eng);

    // 被拒绝前缀不进 RIB，其余前缀仍放行（正负路由同时断言）。
    assert!(
        !has_route(&eng, r2, "172.16.9.0", 24),
        "deny 前缀不得引入: {:?}",
        eng.bgp_routes_of(r2)
    );
    assert!(has_route(&eng, r2, "192.168.1.0", 24), "兜底前缀须引入");
    let table = q(&mut eng, r2, "display bgp routing-table");
    assert!(
        !table.device_output.contains("172.16.9.0/24"),
        "{}",
        table.device_output
    );
    assert!(
        table.device_output.contains("192.168.1.0/24"),
        "{}",
        table.device_output
    );

    // 负向 L4：被拒绝业务 ping 全失败，放行业务仍全通。
    assert_down(&ping(&mut eng, pc2, "172.16.9.10"), "H5-N1 PC2→SRV");
    assert_up(&ping(&mut eng, pc2, "192.168.1.10"), "H5-N1 PC2→PC1");
}

#[test]
fn h5_implicit_deny_unmatched_default() {
    let (mut eng, _pc1, r1, r2, pc2, _srv, mut s1, mut s2) = policy_lab();
    policy_base(&mut eng, r1, r2, &mut s1, &mut s2);
    // ONLY：仅 node10 permit 命中 172.16.9.0，无兜底节点 → 其余前缀隐式拒绝。
    policy_bind(&mut eng, r2, &mut s2, "ONLY", true, false);
    converge(&mut eng);

    // 命中前缀放行，未匹配默认前缀隐式拒绝（与 H5-N1 相反的负向）。
    assert!(
        has_route(&eng, r2, "172.16.9.0", 24),
        "命中前缀须引入: {:?}",
        eng.bgp_routes_of(r2)
    );
    assert!(
        !has_route(&eng, r2, "192.168.1.0", 24),
        "未匹配前缀须隐式拒绝"
    );
    assert_up(&ping(&mut eng, pc2, "172.16.9.10"), "H5-N2 PC2→SRV");
    assert_down(&ping(&mut eng, pc2, "192.168.1.10"), "H5-N2 未匹配默认业务");
}

#[test]
fn h5_undo_node_unbind_recompute_and_recovery() {
    let (mut eng, _pc1, r1, r2, pc2, _srv, mut s1, mut s2) = policy_lab();
    policy_base(&mut eng, r1, r2, &mut s1, &mut s2);
    policy_bind(&mut eng, r2, &mut s2, "FILTER", false, true);
    converge(&mut eng);
    assert_down(
        &ping(&mut eng, pc2, "172.16.9.10"),
        "H5-R1 撤销前 SRV 应被拒",
    );

    // A：undo route-policy FILTER node 10（删 deny 节点）→ 仅剩 node20 permit → SRV 放行。
    let mut sc = Session::default();
    ok(
        &exec_and_run_result(&mut eng, &mut sc, r2, "system-view"),
        "H5-R1 system-view",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut sc, r2, "undo route-policy FILTER node 10"),
        "H5-R1 undo node",
    );
    converge(&mut eng);
    assert!(
        has_route(&eng, r2, "172.16.9.0", 24),
        "删 deny 节点后 SRV 须重算引入"
    );
    assert_up(
        &ping(&mut eng, pc2, "172.16.9.10"),
        "H5-R1 undo node 后业务恢复",
    );
    // 非法节点序号 / 不存在节点：拒绝且状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(&mut eng, &mut sc, r2, "undo route-policy FILTER node 99");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);
    let bad = exec_line_result(&mut eng, &mut sc, r2, "undo route-policy FILTER node 0");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);

    // B：重新加回 deny 节点 → SRV 再次被过滤（证明修改触发重算）。
    for l in [
        "route-policy FILTER deny node 10",
        "if-match acl 2000",
        "quit",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut sc, r2, l),
            &format!("H5-R1 重加 {l}"),
        );
    }
    converge(&mut eng);
    assert!(
        !has_route(&eng, r2, "172.16.9.0", 24),
        "重加 deny 节点后 SRV 须再被过滤"
    );
    assert_down(
        &ping(&mut eng, pc2, "172.16.9.10"),
        "H5-R1 重加 deny 后业务",
    );

    // C：解绑入向策略 → 无过滤，SRV 恢复。
    ok(
        &exec_and_run_result(&mut eng, &mut sc, r2, "bgp 65002"),
        "H5-R1 进 BGP",
    );
    ok(
        &exec_and_run_result(
            &mut eng,
            &mut sc,
            r2,
            "undo peer 10.0.0.1 route-policy FILTER import",
        ),
        "H5-R1 解绑",
    );
    converge(&mut eng);
    assert!(has_route(&eng, r2, "172.16.9.0", 24), "解绑后 SRV 须恢复");
    assert_up(&ping(&mut eng, pc2, "172.16.9.10"), "H5-R1 解绑后业务恢复");
    // 重复解绑：拒绝且状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(
        &mut eng,
        &mut sc,
        r2,
        "undo peer 10.0.0.1 route-policy FILTER import",
    );
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);

    // D：重新绑定后 undo acl 2000 → 匹配条件失效，deny 节点不再命中，SRV 经兜底放行。
    ok(
        &exec_and_run_result(
            &mut eng,
            &mut sc,
            r2,
            "peer 10.0.0.1 route-policy FILTER import",
        ),
        "H5-R1 重绑",
    );
    converge(&mut eng);
    assert_down(
        &ping(&mut eng, pc2, "172.16.9.10"),
        "H5-R1 重绑后 SRV 应被拒",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut sc, r2, "quit"),
        "H5-R1 回配置",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut sc, r2, "undo acl 2000"),
        "H5-R1 undo acl",
    );
    converge(&mut eng);
    assert!(
        has_route(&eng, r2, "172.16.9.0", 24),
        "undo acl 后匹配失效应重算放行"
    );
    assert_up(
        &ping(&mut eng, pc2, "172.16.9.10"),
        "H5-R1 undo acl 后业务恢复",
    );
    let acl = q(&mut eng, r2, "display acl all");
    assert!(
        acl.device_output.contains("No ACL exists"),
        "{}",
        acl.device_output
    );
}

/// R1—R2—R3，R1/R3 同宣告 10.9.0.0/24，R2 经两条 eBGP 学习同一前缀。
fn pref_lab() -> (Engine, u32, u32, u32, Session, Session, Session) {
    let mut net = Net::new();
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Vrp);
    let r2 = net.add_device("R2".into(), DeviceKind::Router, Dialect::Vrp);
    let r3 = net.add_device("R3".into(), DeviceKind::Router, Dialect::Vrp);
    net.add_link(r1, r2).unwrap();
    net.add_link(r2, r3).unwrap();
    for (dev, port, addr) in [
        (r1, 0, "10.0.0.1"),
        (r2, 0, "10.0.0.2"),
        (r2, 1, "10.0.1.2"),
        (r3, 0, "10.0.1.1"),
    ] {
        net.set_port_admin(dev, port, true);
        net.set_port_ip(dev, port, Some(ip(addr)), 24);
    }
    (
        Engine::new(net),
        r1,
        r2,
        r3,
        Session::default(),
        Session::default(),
        Session::default(),
    )
}

#[test]
fn h5_local_preference_changes_real_path_selection() {
    let (mut eng, r1, r2, r3, mut s1, mut s2, mut s3) = pref_lab();
    for l in [
        "system-view",
        "bgp 65001",
        "peer 10.0.0.2 as-number 65002",
        "network 10.9.0.0 255.255.255.0",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s1, r1, l),
            &format!("H5-A1 R1 {l}"),
        );
    }
    for l in [
        "system-view",
        "bgp 65003",
        "peer 10.0.1.2 as-number 65002",
        "network 10.9.0.0 255.255.255.0",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s3, r3, l),
            &format!("H5-A1 R3 {l}"),
        );
    }
    for l in [
        "system-view",
        "route-policy PREF permit node 10",
        "apply local-preference 200",
        "quit",
        "bgp 65002",
        "peer 10.0.0.1 as-number 65001",
        "peer 10.0.1.1 as-number 65003",
        "peer 10.0.1.1 route-policy PREF import",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s2, r2, l),
            &format!("H5-A1 R2 {l}"),
        );
    }
    converge(&mut eng);

    // 优选 local-pref=200 的 R3 路径：真实 RIB 下一跳与 LP，而非仅配置文本。
    let routes = eng.bgp_routes_of(r2);
    assert_eq!(routes.len(), 1, "同前缀只保留最优路径: {routes:?}");
    let (net, len, _, nexthop, lp) = &routes[0];
    assert_eq!((*net, *len), (ip("10.9.0.0"), 24));
    assert_eq!(*nexthop, ip("10.0.1.1"), "应优选 R3（lp=200）: {routes:?}");
    assert_eq!(*lp, 200);
    let table = q(&mut eng, r2, "display bgp routing-table");
    assert!(
        table.device_output.contains("10.0.1.1"),
        "{}",
        table.device_output
    );
    assert!(
        table.device_output.contains("200"),
        "{}",
        table.device_output
    );
    let cfg = config_view(&mut eng, r2);
    assert!(cfg.contains("apply local-preference 200"), "{cfg}");

    // 改绑：把 PREF 从 R3 入向移到 R1 入向 → 真实优选下一跳翻转到 R1。
    for l in [
        "undo peer 10.0.1.1 route-policy PREF import",
        "peer 10.0.0.1 route-policy PREF import",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s2, r2, l),
            &format!("H5-A1 改绑 {l}"),
        );
    }
    converge(&mut eng);
    let routes = eng.bgp_routes_of(r2);
    assert_eq!(routes.len(), 1, "{routes:?}");
    let (_, _, _, nexthop, lp) = &routes[0];
    assert_eq!(
        *nexthop,
        ip("10.0.0.1"),
        "改绑后应优选 R1（lp=200）: {routes:?}"
    );
    assert_eq!(*lp, 200);

    // 撤销 apply → 两条路径回到默认 lp=100，不再有人为优选（证明 apply 真实生效）。
    let mut sc = Session::default();
    for l in [
        "system-view",
        "route-policy PREF permit node 10",
        "undo apply local-preference",
        "quit",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut sc, r2, l),
            &format!("H5-A1 undo apply {l}"),
        );
    }
    converge(&mut eng);
    let routes = eng.bgp_routes_of(r2);
    assert_eq!(routes.len(), 1, "{routes:?}");
    assert_eq!(
        routes[0].4, 100,
        "undo apply 后 LP 应回默认 100: {routes:?}"
    );
    let cfg = config_view(&mut eng, r2);
    assert!(
        !cfg.contains("apply local-preference 200"),
        "undo apply 后配置须清除: {cfg}"
    );
}

#[test]
fn h5_failure_paths_are_state_unchanged() {
    let (mut eng, _pc1, r1, r2, _pc2, _srv, mut s1, mut s2) = policy_lab();
    policy_base(&mut eng, r1, r2, &mut s1, &mut s2);
    converge(&mut eng);

    // 非法序号：node 0 / node 70000 → 拒绝且状态不变（配置视图）。
    let mut sc = Session::default();
    exec_line_result(&mut eng, &mut sc, r2, "system-view");
    for line in [
        "route-policy BAD permit node 0",
        "route-policy BAD permit node 70000",
    ] {
        let before = state_fingerprint(&eng);
        let bad = exec_line_result(&mut eng, &mut sc, r2, line);
        assert_eq!(bad.status, CommandStatus::InvalidCommand, "{line}");
        assert_eq!(state_fingerprint(&eng), before, "{line} 不得改状态");
    }
    // 非法 action：拒绝且状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(&mut eng, &mut sc, r2, "route-policy BAD maybe node 10");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);

    // 不存在的 ACL 引用：if-match acl 9999 → 拒绝，节点 match_acl 保持 None。
    let l = "route-policy P permit node 10";
    ok(
        &exec_and_run_result(&mut eng, &mut sc, r2, l),
        &format!("H5 fail {l}"),
    );
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(&mut eng, &mut sc, r2, "if-match acl 9999");
    assert_eq!(
        bad.status,
        CommandStatus::InvalidCommand,
        "不存在 ACL 引用须拒绝"
    );
    assert!(
        bad.device_output.contains("ACL not found"),
        "{}",
        bad.device_output
    );
    assert_eq!(state_fingerprint(&eng), before);
    let sum = dev_summary(&eng, r2);
    let p = sum.route_policies.iter().find(|p| p.name == "P").unwrap();
    assert!(
        p.nodes.iter().all(|n| n.match_acl.is_none()),
        "拒绝后不得写入 match_acl"
    );

    // 错误模式：配置视图下 if-match / peer route-policy 不可解析 → 拒绝且状态不变。
    exec_line_result(&mut eng, &mut sc, r2, "quit"); // 回配置视图
    for line in ["if-match acl 2000", "peer 10.0.0.1 route-policy P import"] {
        let before = state_fingerprint(&eng);
        let bad = exec_line_result(&mut eng, &mut sc, r2, line);
        assert_eq!(bad.status, CommandStatus::InvalidCommand, "错误模式 {line}");
        assert_eq!(state_fingerprint(&eng), before);
    }

    // 不存在的策略引用：peer route-policy NOPE → 拒绝且状态不变（BGP 视图）。
    exec_line_result(&mut eng, &mut sc, r2, "bgp 65002");
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(
        &mut eng,
        &mut sc,
        r2,
        "peer 10.0.0.1 route-policy NOPE import",
    );
    assert_eq!(
        bad.status,
        CommandStatus::InvalidCommand,
        "不存在策略引用须拒绝"
    );
    assert!(
        bad.device_output.contains("Route policy not found"),
        "{}",
        bad.device_output
    );
    assert_eq!(state_fingerprint(&eng), before);
    // 非法方向：拒绝且状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(
        &mut eng,
        &mut sc,
        r2,
        "peer 10.0.0.1 route-policy P sideways",
    );
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);

    // 重复定义不静默覆盖整条策略：重入 node10 后 node20 仍在。
    exec_line_result(&mut eng, &mut sc, r2, "quit");
    for l in [
        "route-policy DUP deny node 10",
        "quit",
        "route-policy DUP permit node 20",
        "quit",
        "route-policy DUP deny node 10", // 重入同节点
        "quit",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut sc, r2, l),
            &format!("H5 dup {l}"),
        );
    }
    let sum = dev_summary(&eng, r2);
    let dup = sum.route_policies.iter().find(|p| p.name == "DUP").unwrap();
    assert!(dup.nodes.iter().any(|n| n.seq == 10), "重入不得丢失 node10");
    assert!(
        dup.nodes.iter().any(|n| n.seq == 20),
        "重入不得静默覆盖 node20"
    );

    // 不存在策略 / 节点的 undo：拒绝且状态不变。
    for line in ["undo route-policy NOPE", "undo route-policy DUP node 99"] {
        let before = state_fingerprint(&eng);
        let bad = exec_line_result(&mut eng, &mut sc, r2, line);
        assert_eq!(bad.status, CommandStatus::InvalidCommand, "{line}");
        assert_eq!(state_fingerprint(&eng), before);
    }
}

#[test]
fn h5_policy_modification_before_after_comparison() {
    // 同一拓扑下先后施加不同策略，对比真实 RIB 与业务，证明修改即时重算。
    let (mut eng, _pc1, r1, r2, pc2, _srv, mut s1, mut s2) = policy_lab();
    policy_base(&mut eng, r1, r2, &mut s1, &mut s2);
    converge(&mut eng);
    // 无策略：两条前缀都引入，SRV 可达。
    assert!(has_route(&eng, r2, "172.16.9.0", 24));
    assert_up(&ping(&mut eng, pc2, "172.16.9.10"), "H5 修改前无策略 SRV");

    // 施加 deny：SRV 前缀消失、业务失败。
    policy_bind(&mut eng, r2, &mut s2, "FILTER", false, true);
    converge(&mut eng);
    assert!(!has_route(&eng, r2, "172.16.9.0", 24));
    assert_down(&ping(&mut eng, pc2, "172.16.9.10"), "H5 修改后 deny SRV");

    // 改为 permit（重入 node10 改动作）：SRV 前缀恢复、业务恢复。
    let mut sc = Session::default();
    for l in [
        "system-view",
        "route-policy FILTER permit node 10",
        "if-match acl 2000",
        "quit",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut sc, r2, l),
            &format!("H5 改 permit {l}"),
        );
    }
    converge(&mut eng);
    assert!(
        has_route(&eng, r2, "172.16.9.0", 24),
        "改 permit 后须重算引入"
    );
    assert_up(&ping(&mut eng, pc2, "172.16.9.10"), "H5 改 permit 后 SRV");
}

// =====================================================================
// 横切：lint / 能力先于语义 / 双通道 / inspection 纯读
// =====================================================================

#[test]
fn stage2c_lint_has_no_duplicates_dangling_drift_or_strict_gaps() {
    for (d, name) in [
        (Dialect::Ios, "IOS"),
        (Dialect::Vrp, "VRP"),
        (Dialect::Comware, "Comware"),
    ] {
        let rep: LintReport = specs::lint_report(d);
        assert!(rep.strict_ok(), "{name} strict 失败: {rep:?}");
        assert!(rep.duplicates.is_empty(), "{name}: {rep:?}");
        assert!(rep.dangling.is_empty(), "{name}: {rep:?}");
        assert!(rep.drift.is_empty(), "{name}: {rep:?}");
        assert!(rep.strict_missing.is_empty(), "{name}: {rep:?}");
        assert_eq!(rep.strict_covered, rep.strict_total, "{name}: {rep:?}");
        assert_eq!(
            rep.unassociated.len() + specs::rows(d).len(),
            rep.derived_total,
            "{name}: 已关联 + 显式未关联 = 派生总数"
        );
    }
    // H4/H5 的可执行节点必须已关联且启用 strict。
    for needle in [
        "config/bgp/@0number",
        "bgp/peer/@0ipv4/as-number/@0number",
        "bgp/network/@0ipv4/@0dotted-mask",
        "config/undo/bgp/@0number",
        "bgp/undo/peer/@0ipv4",
        "bgp/undo/network/@0ipv4/@0dotted-mask",
        "exec/display/bgp/peer",
        "exec/display/bgp/routing-table",
        "config/acl/@0number",
        "config/undo/acl/@0number",
        "exec/display/acl/all",
        "config/route-policy/@0word/@0word/node/@0number",
        "route_policy/if-match/acl/@0number",
        "route_policy/apply/local-preference/@0number",
        "config/undo/route-policy/@0word/node/@0number",
        "route_policy/undo/if-match",
        "route_policy/undo/apply",
        "exec/display/route-policy",
        "bgp/peer/@0ipv4/route-policy/@0word/@0word",
        "bgp/undo/peer/@0ipv4/route-policy/@0word/@0word",
    ] {
        let spec =
            specs::find(Dialect::Vrp, needle).unwrap_or_else(|| panic!("VRP 缺少规格 {needle}"));
        assert!(spec.strict, "{needle} 必须启用 strict");
        assert!(!spec.contract.is_empty(), "{needle} 必须有契约");
        assert!(!spec.references.is_empty(), "{needle} 必须有证据引用");
    }
}

#[test]
fn stage2c_capability_precedes_semantics_and_unknown_is_not_unsupported() {
    // VRP 路由器 BGP/策略命令：能力 Supported（文档基线），证据不足 → Executed + 待验证，
    // 不得伪判 UnsupportedByDevice。
    for (mode, line) in [
        (Mode::Config, "bgp 65001"),
        (Mode::Bgp, "peer 10.0.0.2 as-number 65002"),
        (Mode::Bgp, "network 192.168.1.0 255.255.255.0"),
        (Mode::Config, "route-policy FILTER deny node 10"),
        (Mode::RoutePolicy, "if-match acl 2000"),
        (Mode::RoutePolicy, "apply local-preference 200"),
        (Mode::Exec, "display bgp peer"),
        (Mode::Exec, "display route-policy"),
    ] {
        let insp = inspect_command(profile_mod::VRP_ROUTER_PROFILE, mode, line);
        assert_eq!(
            insp.status,
            CommandStatus::Executed,
            "{mode:?} {line}: 能力 Supported 不得判不支持: {insp:?}"
        );
        assert_eq!(
            insp.support,
            SupportLabel::PendingVerification,
            "{mode:?} {line}: 证据不足应待验证: {insp:?}"
        );
    }
}

#[test]
fn stage2c_dual_channel_and_inspection_is_pure_read() {
    let (mut eng, _pc1, r1, r2, _pc2, mut s1, mut s2) = bgp_lab();
    bgp_base(&mut eng, r1, r2, &mut s1, &mut s2);
    converge(&mut eng);

    // 设备输出不得被模拟器措辞污染；提示只走 simulator_notices（[netemu] 前缀）。
    for l in [
        "display bgp peer",
        "display bgp routing-table",
        "display current-configuration",
    ] {
        let r = q(&mut eng, r1, l);
        assert!(
            scan_simulator_wording(&r.device_output).is_empty(),
            "{l}: {}",
            r.device_output
        );
        for n in &r.simulator_notices {
            assert!(n.starts_with("[netemu]"), "提示必须有明确前缀: {n}");
        }
    }
    // 旧文本 API 不丢设备输出。
    let mut top = Session::default();
    let cfg = exec_and_run(&mut eng, &mut top, r1, "display current-configuration");
    assert!(cfg.contains("bgp 65001"), "{cfg}");

    // inspection 纯读：不推进时间、不发探测、不改状态。
    let before = state_fingerprint(&eng);
    let now = eng.now_ns();
    let inflight = eng.in_flight();
    for _ in 0..3 {
        let p = inspect_command(
            profile_mod::VRP_ROUTER_PROFILE,
            Mode::Exec,
            "display bgp peer",
        );
        assert_eq!(p.status, CommandStatus::Executed);
        let top = Session::default();
        let view = inspect_device_command(&eng, r1, &top, "display current-configuration");
        assert_eq!(view.status, CommandStatus::Executed);
    }
    assert_eq!(state_fingerprint(&eng), before, "inspection 不得改变状态");
    assert_eq!(eng.now_ns(), now, "inspection 不得推进虚拟时间");
    assert_eq!(eng.in_flight(), inflight, "inspection 不得发出探测");
}
