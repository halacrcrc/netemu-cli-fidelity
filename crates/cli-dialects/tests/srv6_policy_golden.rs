//! SRv6 Policy（SRH 段列表 + End/End.DT6）的黄金测试：
//! 菱形双路径 PE1—P1—PE2 / PE1—P2—PE2，Policy 引导流量走 P2 路径。

use cli_dialects::exec_and_run;
use cli_engine::Session;
use sim_core::device::{DeviceKind, Dialect};
use sim_core::engine::{Engine, PING_HORIZON_NS};
use sim_core::Net;
use sim_packets::{ipv6_from_str, ipv6_str};

use std::collections::HashMap;

struct Lab {
    eng: Engine,
    /// 每设备独立 CLI 会话（与 wasm/浏览器行为一致）
    sessions: HashMap<u32, Session>,
    pc1: u32,
    pe1: u32,
}

fn build() -> Lab {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let pe1 = net.add_device("PE1".into(), DeviceKind::Router, Dialect::Vrp);
    let p1 = net.add_device("P1".into(), DeviceKind::Router, Dialect::Vrp);
    let p2 = net.add_device("P2".into(), DeviceKind::Router, Dialect::Vrp);
    let pe2 = net.add_device("PE2".into(), DeviceKind::Router, Dialect::Vrp);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc1, pe1).unwrap(); // PE1 p0
    net.add_link(pe1, p1).unwrap(); // PE1 p1 / P1 p0
    net.add_link(p1, pe2).unwrap(); // P1 p1 / PE2 p0
    net.add_link(pe1, p2).unwrap(); // PE1 p2 / P2 p0
    net.add_link(p2, pe2).unwrap(); // P2 p1 / PE2 p1
    net.add_link(pe2, pc2).unwrap(); // PE2 p2
    net.set_host_v6(
        pc1,
        Some(ipv6_from_str("2001:db8:1::10").unwrap()),
        64,
        Some(ipv6_from_str("2001:db8:1::1").unwrap()),
    );
    net.set_host_v6(
        pc2,
        Some(ipv6_from_str("2001:db8:2::10").unwrap()),
        64,
        Some(ipv6_from_str("2001:db8:2::1").unwrap()),
    );
    Lab {
        eng: Engine::new(net),
        sessions: HashMap::new(),
        pc1,
        pe1,
    }
}

fn run(lab: &mut Lab, dev: u32, lines: &[&str]) {
    let mut sess = lab.sessions.remove(&dev).unwrap_or_default();
    for l in lines {
        let out = exec_and_run(&mut lab.eng, &mut sess, dev, l);
        assert!(
            !out.contains("% ") && !out.contains("Error:"),
            "命令 {:?} 意外报错: {}",
            l,
            out
        );
    }
    lab.sessions.insert(dev, sess);
}

fn ping6_count(lab: &mut Lab, from: u32, to: &str) -> usize {
    let dropped_before = lab.eng.stats().dropped;
    lab.eng.start_ping6(from, ipv6_from_str(to).unwrap());
    // 与判分桥同等待遇：45s 虚拟时间（NDP 解析 + ping6 会话完成）
    lab.eng.run_to_quiescence(45_000_000_000);
    let sessions = lab.eng.take_unreported_ping6(from);
    for s in &sessions {
        println!(
            "  ping6 session id={} target={} sent={} received={} done={}",
            s.id,
            ipv6_str(&s.target),
            s.sent,
            s.received,
            s.done
        );
    }
    let got = sessions
        .iter()
        .find(|s| s.target == ipv6_from_str(to).unwrap())
        .map(|s| (s.sent as usize, s.received as usize))
        .unwrap_or((0, 0));
    println!(
        "  ping6_count target={} sent/received={:?} dropped_delta={}",
        to,
        got,
        lab.eng.stats().dropped - dropped_before
    );
    got.1
}

/// Host→网关 ping 的最小回归（不涉及 SRv6）。
#[test]
fn host_gw_ping_regression() {
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r = net.add_device("R1".into(), DeviceKind::Router, Dialect::Vrp);
    net.add_link(pc, r).unwrap();
    net.set_host_v6(
        pc,
        Some(ipv6_from_str("2001:db8:1::10").unwrap()),
        64,
        Some(ipv6_from_str("2001:db8:1::1").unwrap()),
    );
    let mut lab = Lab {
        eng: Engine::new(net),
        sessions: std::collections::HashMap::new(),
        pc1: pc,
        pe1: r,
    };
    run(
        &mut lab,
        r,
        &[
            "system-view",
            "interface g0/0/0",
            "ipv6 address 2001:db8:1::1/64",
            "undo shutdown",
            "quit",
        ],
    );
    lab.eng.run_to_quiescence(PING_HORIZON_NS);
    let pc = lab.pc1;
    assert_eq!(
        ping6_count(&mut lab, pc, "2001:db8:1::1"),
        sim_core::device::Ping6Session::TOTAL as usize,
        "Host → 网关 ping6 应通（回归保护）"
    );
}

fn build_config(lab: &mut Lab) {
    let pe1 = lab.pe1;
    let p2 = lab
        .eng
        .net
        .devices
        .iter()
        .find(|d| d.name == "P2")
        .unwrap()
        .id;
    let pe2 = lab
        .eng
        .net
        .devices
        .iter()
        .find(|d| d.name == "PE2")
        .unwrap()
        .id;
    run(
        lab,
        pe1,
        &[
            "quit",
            "system-view",
            "interface g0/0/0",
            "ipv6 address 2001:db8:1::1/64",
            "undo shutdown",
            "quit",
            "interface g0/0/1",
            "ipv6 address 2001:db8:12::1/64",
            "undo shutdown",
            "quit",
            "interface g0/0/2",
            "ipv6 address 2001:db8:14::1/64",
            "undo shutdown",
            "quit",
            "ipv6 route-static 2001:db8:400:: 64 2001:db8:14::2",
            "ipv6 route-static 2001:db8:200:: 64 2001:db8:12::2",
            "srv6 locator 2001:db8:100::/64",
            "srv6 sid 2001:db8:100::1",
            "srv6 policy POL1 color 100 end-point 2001:db8:200::1",
            "segment-list 2001:db8:400::2 2001:db8:200::1",
            "quit",
            "srv6 route 2001:db8:2::/64 policy POL1",
        ],
    );
    let p1 = lab
        .eng
        .net
        .devices
        .iter()
        .find(|d| d.name == "P1")
        .unwrap()
        .id;
    run(
        lab,
        p1,
        &[
            "quit",
            "system-view",
            "interface g0/0/0",
            "ipv6 address 2001:db8:12::2/64",
            "undo shutdown",
            "quit",
            "interface g0/0/1",
            "ipv6 address 2001:db8:23::1/64",
            "undo shutdown",
            "quit",
            "ipv6 route-static 2001:db8:200:: 64 2001:db8:23::2",
            "ipv6 route-static 2001:db8:100:: 64 2001:db8:12::1",
        ],
    );
    run(
        lab,
        p2,
        &[
            "quit",
            "system-view",
            "interface g0/0/0",
            "ipv6 address 2001:db8:14::2/64",
            "undo shutdown",
            "quit",
            "interface g0/0/1",
            "ipv6 address 2001:db8:24::1/64",
            "undo shutdown",
            "quit",
            "ipv6 route-static 2001:db8:200:: 64 2001:db8:24::2",
            "ipv6 route-static 2001:db8:100:: 64 2001:db8:14::1",
            "srv6 locator 2001:db8:400::/64",
            "srv6 sid 2001:db8:400::2",
        ],
    );
    run(
        lab,
        pe2,
        &[
            "quit",
            "system-view",
            "interface g0/0/0",
            "ipv6 address 2001:db8:23::2/64",
            "undo shutdown",
            "quit",
            "interface g0/0/1",
            "ipv6 address 2001:db8:24::2/64",
            "undo shutdown",
            "quit",
            "interface g0/0/2",
            "ipv6 address 2001:db8:2::1/64",
            "undo shutdown",
            "quit",
            "ipv6 route-static 2001:db8:400:: 64 2001:db8:24::1",
            "ipv6 route-static 2001:db8:100:: 64 2001:db8:23::2",
            "srv6 locator 2001:db8:200::/64",
            "srv6 sid 2001:db8:200::1",
            "srv6 route 2001:db8:1::/64 sid 2001:db8:100::1",
        ],
    );
    lab.eng.run_to_quiescence(PING_HORIZON_NS);
}

/// Policy 配置存在：PE1 有 POL1（端点 + 段列表），route 引用 policy。
#[test]
fn srv6_policy_configured() {
    let mut lab = build();
    build_config(&mut lab);
    let pe1 = lab.pe1;
    run(&mut lab, pe1, &["display srv6"]);
    let summary = lab.eng.net.summary();
    let d = summary
        .devices
        .iter()
        .find(|x| x.name == "PE1")
        .expect("PE1 存在");
    assert_eq!(d.srv6_policies.len(), 1, "PE1 应有一条 SRv6 Policy");
    let pol = &d.srv6_policies[0];
    assert_eq!(pol.name, "POL1");
    assert_eq!(pol.endpoint, "2001:db8:200::1");
    assert_eq!(pol.segments.len(), 2, "段列表应有两段");
    assert_eq!(pol.segments[0], "2001:db8:400::2");
    assert_eq!(pol.segments[1], "2001:db8:200::1");
    let r = &d.srv6_routes[0];
    assert_eq!(r.policy.as_deref(), Some("POL1"), "route 应引用 POL1");
}

/// Underlay：PE1 能 ping 通 P2 与 PE2 的 SID 地址。
#[test]
fn underlay_reachable() {
    let mut lab = build();
    build_config(&mut lab);
    let pe1 = lab.pe1;
    let p2 = lab
        .eng
        .net
        .devices
        .iter()
        .find(|d| d.name == "P2")
        .unwrap()
        .id;
    println!("=== PE1 brief ===");
    println!(
        "{}",
        exec_and_run(
            &mut lab.eng,
            lab.sessions.entry(pe1).or_default(),
            pe1,
            "display ipv6 interface brief"
        )
    );
    println!("=== PE1 rt ===");
    println!(
        "{}",
        exec_and_run(
            &mut lab.eng,
            lab.sessions.entry(pe1).or_default(),
            pe1,
            "display ipv6 routing-table"
        )
    );
    println!("=== P2 brief ===");
    println!(
        "{}",
        exec_and_run(
            &mut lab.eng,
            lab.sessions.entry(p2).or_default(),
            p2,
            "display ipv6 interface brief"
        )
    );
    println!("=== P1 brief ===");
    let p1 = lab
        .eng
        .net
        .devices
        .iter()
        .find(|d| d.name == "P1")
        .unwrap()
        .id;
    println!(
        "{}",
        exec_and_run(
            &mut lab.eng,
            lab.sessions.entry(p1).or_default(),
            p1,
            "display ipv6 interface brief"
        )
    );
    let direct = ping6_count(&mut lab, pe1, "2001:db8:12::2");
    assert_eq!(
        direct,
        sim_core::device::Ping6Session::TOTAL as usize,
        "PE1 → P1 直连段应通"
    );
    assert_eq!(
        ping6_count(&mut lab, pe1, "2001:db8:400::2"),
        sim_core::device::Ping6Session::TOTAL as usize,
        "PE1 → P2 SID 应全通"
    );
    assert_eq!(
        ping6_count(&mut lab, pe1, "2001:db8:200::1"),
        sim_core::device::Ping6Session::TOTAL as usize,
        "PE1 → PE2 SID 应全通"
    );
}

/// 端到端：PC1 → PC2 经 SRv6 Policy 路径（PE1→P2→PE2）连通。
#[test]
fn end_to_end_via_policy() {
    let mut lab = build();
    build_config(&mut lab);
    let pc1 = lab.pc1;
    assert_eq!(
        ping6_count(&mut lab, pc1, "2001:db8:2::10"),
        sim_core::device::Ping6Session::TOTAL as usize,
        "PC1 → PC2 经 SRv6 Policy 路径应全通"
    );
}

/// 最小 IPv6 回归：两台路由器直连 + ping6（不涉及 SRv6）。
#[test]
fn plain_ipv6_regression() {
    let mut net = Net::new();
    let a = net.add_device("A".into(), DeviceKind::Router, Dialect::Vrp);
    let b = net.add_device("B".into(), DeviceKind::Router, Dialect::Vrp);
    net.add_link(a, b).unwrap();
    let mut lab = Lab {
        eng: Engine::new(net),
        sessions: std::collections::HashMap::new(),
        pc1: 0,
        pe1: a,
    };
    run(
        &mut lab,
        a,
        &[
            "system-view",
            "interface g0/0/0",
            "ipv6 address 2001:db8:12::1/64",
            "undo shutdown",
            "quit",
        ],
    );
    run(
        &mut lab,
        b,
        &[
            "system-view",
            "interface g0/0/0",
            "ipv6 address 2001:db8:12::2/64",
            "undo shutdown",
            "quit",
        ],
    );
    lab.eng.run_to_quiescence(PING_HORIZON_NS);
    let got = ping6_count(&mut lab, a, "2001:db8:12::2");
    assert_eq!(
        got,
        sim_core::device::Ping6Session::TOTAL as usize,
        "最小 IPv6 直连 ping 应通（回归保护）"
    );
}

/// BE 对照：去掉 Policy（改为 BE 直达远端 SID）后仍连通（回归保护）。
#[test]
fn srv6_be_still_works() {
    let mut lab = build();
    build_config(&mut lab);
    let pe1 = lab.pe1;
    let pc1 = lab.pc1;
    run(
        &mut lab,
        pe1,
        &[
            "quit",
            "system-view",
            "srv6 route 2001:db8:2::/64 sid 2001:db8:200::1",
        ],
    );
    assert_eq!(
        ping6_count(&mut lab, pc1, "2001:db8:2::10"),
        sim_core::device::Ping6Session::TOTAL as usize,
    );
}
