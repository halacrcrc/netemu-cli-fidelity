//! L4 承诺场景清单：显式维护在测试目录中（主技术方案 §2.2）。
//!
//! 未列入 `L4_SCENARIOS` 的协议或数据面行为不在本执行包的承诺范围内。
//! 每个条目都有本文件内可执行的真实数据面断言，ping 命令经 `exec_and_run_result`
//! 走完整 CLI → 引擎静默 → 输出渲染链；不使用 `run_checks`，不推进虚拟时间做判分。

use cli_dialects::exec_and_run_result;
use cli_engine::Session;
use sim_core::device::{DeviceKind, Dialect};
use sim_core::engine::Engine;
use sim_core::Net;
use sim_packets::ipv4_from_str;

fn ip(s: &str) -> u32 {
    ipv4_from_str(s).unwrap()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expected {
    Reachable,
    Unreachable,
}

#[derive(Clone, Copy, Debug)]
struct L4Scenario {
    id: &'static str,
    dialect: Dialect,
    description: &'static str,
    expected: Expected,
    /// 场景构造方式；`ping` 输出是唯一判据，不检查"命令返回成功"。
    variant: Variant,
}

#[derive(Clone, Copy, Debug)]
enum Variant {
    FullRoutes,
    NoRoutes,
    LinkShutdown,
    RouteRemovedByCli,
    VlanSameSubnet,
    VlanCrossIsolated,
    VlanCrossRouted,
}

/// 本切片明确承诺的 L4 场景。新增场景必须同时新增可执行断言。
static L4_SCENARIOS: &[L4Scenario] = &[
    L4Scenario {
        id: "L4-C1-static-route-echo-ok",
        dialect: Dialect::Ios,
        description: "IOS 双路由器 + 双向静态路由，PC1 ping PC2 全通",
        expected: Expected::Reachable,
        variant: Variant::FullRoutes,
    },
    L4Scenario {
        id: "L4-H0-static-route-echo-ok",
        dialect: Dialect::Vrp,
        description: "VRP 双路由器 + 双向静态路由，PC1 ping PC2 全通",
        expected: Expected::Reachable,
        variant: Variant::FullRoutes,
    },
    L4Scenario {
        id: "L4-C1-no-route-echo-fail",
        dialect: Dialect::Ios,
        description: "IOS 未配置静态路由，PC1 ping PC2 全超时且报文被丢弃",
        expected: Expected::Unreachable,
        variant: Variant::NoRoutes,
    },
    L4Scenario {
        id: "L4-H0-no-route-echo-fail",
        dialect: Dialect::Vrp,
        description: "VRP 未配置静态路由，PC1 ping PC2 全超时",
        expected: Expected::Unreachable,
        variant: Variant::NoRoutes,
    },
    L4Scenario {
        id: "L4-C1-link-shutdown-echo-fail",
        dialect: Dialect::Ios,
        description: "IOS 有静态路由但 R1 上联口 shutdown，PC1 ping PC2 全超时",
        expected: Expected::Unreachable,
        variant: Variant::LinkShutdown,
    },
    L4Scenario {
        id: "L4-C1-route-removed-echo-fail",
        dialect: Dialect::Ios,
        description: "IOS 经 CLI 撤销静态路由后，PC1 ping PC2 全超时",
        expected: Expected::Unreachable,
        variant: Variant::RouteRemovedByCli,
    },
    L4Scenario {
        id: "L4-C2-same-vlan-echo-ok",
        dialect: Dialect::Ios,
        description: "IOS 交换机两 access 口同属 VLAN10 同网段，PC1 ping PC2 全通",
        expected: Expected::Reachable,
        variant: Variant::VlanSameSubnet,
    },
    L4Scenario {
        id: "L4-C2-cross-vlan-isolated-echo-fail",
        dialect: Dialect::Ios,
        description: "IOS 交换机两 access 口分属 VLAN10/20 且无 SVI，PC1 ping PC2 全超时",
        expected: Expected::Unreachable,
        variant: Variant::VlanCrossIsolated,
    },
    L4Scenario {
        id: "L4-H1-cross-vlan-routed-echo-ok",
        dialect: Dialect::Vrp,
        description: "VRP 交换机 VLAN10/20 + 双 Vlanif SVI 路由，PC1 ping PC2 跨 VLAN 全通",
        expected: Expected::Reachable,
        variant: Variant::VlanCrossRouted,
    },
    L4Scenario {
        id: "L4-H1-cross-vlan-isolated-echo-fail",
        dialect: Dialect::Vrp,
        description: "VRP 交换机两 access 口分属 VLAN10/20 且无 Vlanif，PC1 ping PC2 全超时",
        expected: Expected::Unreachable,
        variant: Variant::VlanCrossIsolated,
    },
];

fn topology(dialect: Dialect, with_routes: bool, shut_link: bool) -> (Engine, u32, u32) {
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
    for (dev, p0, p1) in [
        (r1, "192.168.1.1", "10.0.0.1"),
        (r2, "10.0.0.2", "192.168.2.1"),
    ] {
        // 显式平台策略：IOS 路由器口默认 down，L4 场景统一先启用，隔离"默认策略"与"数据面"。
        net.set_port_admin(dev, 0, true);
        net.set_port_ip(dev, 0, Some(ip(p0)), 24);
        net.set_port_admin(dev, 1, true);
        net.set_port_ip(dev, 1, Some(ip(p1)), 24);
    }
    if shut_link {
        net.set_port_admin(r1, 1, false);
    }
    if with_routes {
        assert!(net.add_static_route(r1, ip("192.168.2.0"), 24, ip("10.0.0.2")));
        assert!(net.add_static_route(r2, ip("192.168.1.0"), 24, ip("10.0.0.1")));
    }
    (Engine::new(net), pc1, r1)
}

fn ping_from_pc1(eng: &mut Engine, pc1: u32) -> String {
    let mut sess = Session::default();
    exec_and_run_result(eng, &mut sess, pc1, "ping 192.168.2.10").device_output
}

fn execute(scenario: &L4Scenario) -> (String, u64) {
    match scenario.variant {
        Variant::VlanSameSubnet | Variant::VlanCrossIsolated | Variant::VlanCrossRouted => {
            execute_vlan(scenario)
        }
        _ => execute_router(scenario),
    }
}

fn execute_router(scenario: &L4Scenario) -> (String, u64) {
    let with_routes = matches!(
        scenario.variant,
        Variant::FullRoutes | Variant::LinkShutdown | Variant::RouteRemovedByCli
    );
    let shut_link = matches!(scenario.variant, Variant::LinkShutdown);
    let (mut eng, pc1, r1) = topology(scenario.dialect, with_routes, shut_link);
    if matches!(scenario.variant, Variant::RouteRemovedByCli) {
        // 用 IOS CLI 撤销路由，验证数据面随配置状态变化。
        let mut sess = Session::default();
        for line in [
            "enable",
            "configure terminal",
            "no ip route 192.168.2.0 255.255.255.0 10.0.0.2",
        ] {
            let r = exec_and_run_result(&mut eng, &mut sess, r1, line);
            assert_eq!(r.status, cli_engine::spec::CommandStatus::Executed);
        }
    }
    let out = ping_from_pc1(&mut eng, pc1);
    (out, eng.dropped)
}

/// VLAN 数据面场景：交换机经真实 CLI 配置 access/VLAN/SVI，PC1 ping PC2 判据。
/// 同网段同 VLAN 应通；跨 VLAN 无 SVI 应隔离；跨 VLAN 有双 SVI 应经路由全通。
fn execute_vlan(scenario: &L4Scenario) -> (String, u64) {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let sw = net.add_device("SW1".into(), DeviceKind::Switch, scenario.dialect);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc1, sw).unwrap();
    net.add_link(sw, pc2).unwrap();
    net.set_host(pc1, Some(ip("192.168.10.11")), 24, Some(ip("192.168.10.1")));
    net.set_host(pc2, Some(ip("192.168.20.12")), 24, Some(ip("192.168.20.1")));

    let mut eng = Engine::new(net);
    let mut sess = Session::default();
    let lines: Vec<&str> = match (scenario.dialect, scenario.variant) {
        (Dialect::Ios, Variant::VlanSameSubnet) => vec![
            "enable",
            "conf t",
            "vlan 10",
            "int g0/0",
            "switchport mode access",
            "switchport access vlan 10",
            "exit",
            "int g0/1",
            "switchport mode access",
            "switchport access vlan 10",
            "end",
        ],
        (Dialect::Ios, Variant::VlanCrossIsolated) => vec![
            "enable",
            "conf t",
            "vlan 10",
            "vlan 20",
            "int g0/0",
            "switchport mode access",
            "switchport access vlan 10",
            "exit",
            "int g0/1",
            "switchport mode access",
            "switchport access vlan 20",
            "end",
        ],
        (Dialect::Vrp, Variant::VlanCrossRouted) => vec![
            "system-view",
            "vlan batch 10 20",
            "int g0/0/1",
            "port link-type access",
            "port default vlan 10",
            "quit",
            "int g0/0/2",
            "port link-type access",
            "port default vlan 20",
            "quit",
            "int vlanif 10",
            "ip address 192.168.10.1 24",
            "quit",
            "int vlanif 20",
            "ip address 192.168.20.1 24",
            "quit",
        ],
        (Dialect::Vrp, Variant::VlanCrossIsolated) => vec![
            "system-view",
            "vlan batch 10 20",
            "int g0/0/1",
            "port link-type access",
            "port default vlan 10",
            "quit",
            "int g0/0/2",
            "port link-type access",
            "port default vlan 20",
            "quit",
        ],
        _ => unreachable!("非 VLAN 场景不应进入 execute_vlan"),
    };
    // 同网段场景两台 PC 都在 192.168.10.0/24。
    if matches!(scenario.variant, Variant::VlanSameSubnet) {
        eng.net
            .set_host(pc2, Some(ip("192.168.10.12")), 24, Some(ip("192.168.10.1")));
    }
    for line in lines {
        let r = exec_and_run_result(&mut eng, &mut sess, sw, line);
        assert_eq!(
            r.status,
            cli_engine::spec::CommandStatus::Executed,
            "{} 配置命令 {line:?} 应执行: {}",
            scenario.id,
            r.device_output
        );
    }
    let target = if matches!(scenario.variant, Variant::VlanSameSubnet) {
        "192.168.10.12"
    } else {
        "192.168.20.12"
    };
    let out = exec_and_run_result(
        &mut eng,
        &mut Session::default(),
        pc1,
        &format!("ping {target}"),
    )
    .device_output;
    (out, eng.dropped)
}

#[test]
fn every_declared_l4_scenario_has_an_executable_assertion() {
    assert_eq!(L4_SCENARIOS.len(), 10);
    let mut ids = std::collections::HashSet::new();
    for s in L4_SCENARIOS {
        assert!(ids.insert(s.id), "L4 场景 ID 重复: {}", s.id);
        assert!(!s.description.is_empty());
        let (out, dropped) = execute(s);
        match s.expected {
            Expected::Reachable => {
                assert!(
                    out.contains("5 packets transmitted, 5 received"),
                    "{} 应全通: {out}",
                    s.id
                );
                assert!(!out.contains("100% packet loss"), "{}: {out}", s.id);
            }
            Expected::Unreachable => {
                assert!(
                    out.contains("5 packets transmitted, 0 received"),
                    "{} 应全超时: {out}",
                    s.id
                );
                assert!(out.contains("100% packet loss"), "{}: {out}", s.id);
                // 路由器场景：报文到达设备后因缺路由被转发丢弃，dropped 必须 >0。
                // VLAN 隔离场景：无 SVI 时网关 ARP 无法解析，报文在主机侧即不可达，
                // 引擎转发丢弃计数为 0 属正确语义；隔离判据是 0 received + 100% loss。
                if !matches!(s.variant, Variant::VlanCrossIsolated) {
                    assert!(dropped > 0, "{} 应有报文丢弃: {out}", s.id);
                }
            }
        }
    }
}

#[test]
fn ios_and_vrp_l4_baselines_are_both_declared() {
    for (d, label) in [(Dialect::Ios, "C1"), (Dialect::Vrp, "H0")] {
        assert!(
            L4_SCENARIOS.iter().any(|s| s.dialect == d
                && s.expected == Expected::Reachable
                && s.id.contains(label)),
            "{label} 必须同时声明正向场景"
        );
        assert!(
            L4_SCENARIOS.iter().any(|s| s.dialect == d
                && s.expected == Expected::Unreachable
                && s.id.contains(label)),
            "{label} 必须同时声明负向场景"
        );
    }
}

#[test]
fn vlan_l4_scenarios_cover_c2_and_h1_both_directions() {
    for (d, label) in [(Dialect::Ios, "C2"), (Dialect::Vrp, "H1")] {
        assert!(
            L4_SCENARIOS.iter().any(|s| s.dialect == d
                && s.expected == Expected::Reachable
                && s.id.contains(label)),
            "{label} 必须声明 VLAN 正向数据面场景"
        );
        assert!(
            L4_SCENARIOS.iter().any(|s| s.dialect == d
                && s.expected == Expected::Unreachable
                && s.id.contains(label)
                && matches!(s.variant, Variant::VlanCrossIsolated)),
            "{label} 必须声明跨 VLAN 隔离负向场景"
        );
    }
}
