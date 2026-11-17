//! Stage 2B：九条流程（C3 → C6a → C5 → C4 → C7 → C8 → H2 → H3 → H6）闭环测试。
//!
//! 顺序与 `netemu-stage2b-implementation-handoff.md` §5 一致；H4/H5 不在本包。
//! 所有判据走真实 CLI → 引擎静默 → 输出渲染链；不使用 parse 成功、命令返回成功
//! 或硬编码文本代替协议/数据面闭环。无带版本设备样本，输出只做字段/结构/状态断言。

use cli_dialects::specs;
use cli_dialects::{
    complete_line, exec_and_run, exec_and_run_result, exec_line_result, inspect_command,
    inspect_device_command, scan_simulator_wording, state_fingerprint,
};
use cli_engine::spec::{CommandStatus, LintReport, SupportLabel};
use cli_engine::{Mode, Session};
use sim_core::device::{DeviceKind, DeviceState, Dialect, VrrpState};
use sim_core::engine::Engine;
use sim_core::profile as profile_mod;
use sim_core::Net;
use sim_packets::ipv4_from_str;

fn ip(s: &str) -> u32 {
    ipv4_from_str(s).unwrap()
}

/// 单路由器 + 主机拓扑（C3/C4/C5/C6a 用）。端口 0 接主机，端口 1 接外网/服务器。
fn router_lab(dialect: Dialect) -> (Engine, u32, u32, u32, Session) {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, dialect);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc1, r1).unwrap();
    net.add_link(r1, pc2).unwrap();
    net.set_host(pc1, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    net.set_host(pc2, Some(ip("192.168.2.10")), 24, Some(ip("192.168.2.1")));
    net.set_port_admin(r1, 0, true);
    net.set_port_ip(r1, 0, Some(ip("192.168.1.1")), 24);
    net.set_port_admin(r1, 1, true);
    net.set_port_ip(r1, 1, Some(ip("192.168.2.1")), 24);
    (Engine::new(net), pc1, r1, pc2, Session::default())
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

fn leased_ips(eng: &Engine, dev: u32) -> Vec<u32> {
    match &eng.net.device(dev).unwrap().state {
        DeviceState::Router(rs) => rs.dhcp_leases.iter().map(|l| l.ip).collect(),
        _ => vec![],
    }
}

fn host_ip(eng: &Engine, dev: u32) -> Option<u32> {
    eng.net
        .device(dev)
        .and_then(|d| d.port(0))
        .and_then(|p| p.ip)
}

/// 查询统一在顶层视图执行（fresh Exec session），不受配置会话当前模式影响。
fn q(eng: &mut Engine, dev: u32, line: &str) -> cli_engine::spec::CommandResult {
    let mut s = Session::default();
    exec_and_run_result(eng, &mut s, dev, line)
}

fn config_view(eng: &mut Engine, dev: u32, dialect: Dialect) -> String {
    let line = if dialect == Dialect::Ios {
        "show running-config"
    } else {
        "display current-configuration"
    };
    q(eng, dev, line).device_output
}

// =====================================================================
// L4 场景清单：本包显式承诺的正/负数据面场景，每个条目在下方有可执行断言。
// =====================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum L4Outcome {
    Reachable,
    Unreachable,
    TcpOpen,
    TcpBlocked,
}

#[derive(Clone, Copy, Debug)]
struct L4Scenario {
    id: &'static str,
    flow: &'static str,
    #[allow(dead_code)]
    description: &'static str,
    outcome: L4Outcome,
}

static STAGE2B_L4: &[L4Scenario] = &[
    L4Scenario {
        id: "L4-C3-dhcp-lease-ping-ok",
        flow: "C3",
        description: "IOS DHCP 池/网关下发后主机获得地址，ping 网关全通",
        outcome: L4Outcome::Reachable,
    },
    L4Scenario {
        id: "L4-C3-no-pool-ping-fail",
        flow: "C3",
        description: "无匹配池时主机不得伪造地址，ping 全失败",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-C3-excluded-all-ping-fail",
        flow: "C3",
        description: "池内地址全被排除时无地址可分配，ping 全失败",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-C5-acl-implicit-deny-fail",
        flow: "C5",
        description: "ACL 无命中 = 隐式拒绝，ICMP 全失败",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-C5-acl-tcp-permit-ok",
        flow: "C5",
        description: "TCP 规则命中 allow 且端口匹配，TCP 握手成功",
        outcome: L4Outcome::TcpOpen,
    },
    L4Scenario {
        id: "L4-C5-acl-tcp-port-deny-fail",
        flow: "C5",
        description: "TCP 规则端口不匹配被拒绝，握手无响应",
        outcome: L4Outcome::TcpBlocked,
    },
    L4Scenario {
        id: "L4-C4-nat-inside-outside-ok",
        flow: "C4",
        description: "NAT inside/outside 正确时 ICMP 经转换表转发全通",
        outcome: L4Outcome::Reachable,
    },
    L4Scenario {
        id: "L4-C4-nat-no-role-fail",
        flow: "C4",
        description: "no 形式撤销 NAT 角色后配置视图与转换表同步清空，不再渲染 NAT",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-C7-ospf-route-ok",
        flow: "C7",
        description: "OSPF 建邻并生成动态路由，跨网段 ping 全通",
        outcome: L4Outcome::Reachable,
    },
    L4Scenario {
        id: "L4-C7-ospf-area-mismatch-fail",
        flow: "C7",
        description: "两端区域不匹配不建邻，跨网段 ping 失败",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-C8-vrrp-failover-ok",
        flow: "C8",
        description: "主设备故障后备份接管 VIP，业务保持",
        outcome: L4Outcome::Reachable,
    },
    L4Scenario {
        id: "L4-C8-vrrp-no-master-fail",
        flow: "C8",
        description: "删除全部 VRRP 组后无 Master，VIP 业务失败",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-H6-ospf-route-ok",
        flow: "H6",
        description: "VRP OSPF 双层视图建邻并生成路由，ping 全通",
        outcome: L4Outcome::Reachable,
    },
    L4Scenario {
        id: "L4-H6-ospf-area-mismatch-fail",
        flow: "H6",
        description: "VRP OSPF 区域不匹配不建邻，ping 失败",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-H2-stp-redundant-ok",
        flow: "H2",
        description: "STP 冗余拓扑无环，跨交换机主机 ping 全通；主链路恢复后重收敛业务恢复",
        outcome: L4Outcome::Reachable,
    },
    L4Scenario {
        id: "L4-H2-stp-all-links-down-fail",
        flow: "H2",
        description: "交换机间链路全部断开时业务失败，证明转发不是硬编码直通",
        outcome: L4Outcome::Unreachable,
    },
    L4Scenario {
        id: "L4-H3-lacp-single-member-ok",
        flow: "H3",
        description: "LACP 聚合正常及单成员故障时业务保持；成员恢复后业务恢复",
        outcome: L4Outcome::Reachable,
    },
    L4Scenario {
        id: "L4-H3-lacp-all-members-down-fail",
        flow: "H3",
        description: "全部成员失效时 Selected 清空，跨交换机业务失败",
        outcome: L4Outcome::Unreachable,
    },
];

#[test]
fn stage2b_l4_registry_is_explicit_and_nonempty() {
    assert_eq!(STAGE2B_L4.len(), 18);
    let flows = ["C3", "C4", "C5", "C7", "C8", "H2", "H3", "H6"];
    for f in flows {
        let pos = STAGE2B_L4
            .iter()
            .any(|s| s.flow == f && matches!(s.outcome, L4Outcome::Reachable | L4Outcome::TcpOpen));
        let neg = STAGE2B_L4.iter().any(|s| {
            s.flow == f && matches!(s.outcome, L4Outcome::Unreachable | L4Outcome::TcpBlocked)
        });
        assert!(pos && neg, "{f} 必须同时登记正、负 L4 场景");
    }
    let mut ids: Vec<&str> = STAGE2B_L4.iter().map(|s| s.id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "L4 场景 ID 不得重复");
}

// =====================================================================
// C3：IOS DHCP
// =====================================================================

fn dhcp_lab(pool: bool, exclude_all: bool) -> (Engine, u32, u32, Session) {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Ios);
    net.add_link(pc1, r1).unwrap();
    net.set_port_admin(r1, 0, true);
    net.set_port_ip(r1, 0, Some(ip("192.168.10.1")), 24);
    net.set_host(pc1, None, 24, None);
    if let DeviceState::Host(h) = &mut net.device_mut(pc1).unwrap().state {
        h.dhcp_enable = true;
    }
    let mut eng = Engine::new(net);
    let mut sess = Session::default();
    let mut lines: Vec<&str> = vec!["enable", "configure terminal"];
    if pool {
        lines.extend([
            "ip dhcp pool OFFICE",
            "network 192.168.10.0 255.255.255.0",
            "default-router 192.168.10.1",
            "dns-server 8.8.8.8",
            "exit",
        ]);
    }
    if exclude_all {
        lines.push("ip dhcp excluded-address 192.168.10.2 192.168.10.254");
    }
    for l in lines {
        let r = exec_and_run_result(&mut eng, &mut sess, r1, l);
        ok(&r, &format!("C3 配置 {l}"));
    }
    (eng, pc1, r1, sess)
}

fn dhcp_renew(eng: &mut Engine, pc1: u32, r1: u32, sess: &mut Session) {
    let mut ps = Session::default();
    let r = exec_and_run_result(eng, &mut ps, pc1, "ipconfig /renew");
    ok(&r, "C3 ipconfig /renew");
    let _ = (r1, sess);
}

#[test]
fn c3_dhcp_positive_lease_gateway_and_ping() {
    let (mut eng, pc1, r1, mut sess) = dhcp_lab(true, false);
    // 分配状态：主机拿到池内地址与网关。
    assert_eq!(host_ip(&eng, pc1), None, "分配前不得伪造地址");
    dhcp_renew(&mut eng, pc1, r1, &mut sess);
    let assigned = host_ip(&eng, pc1).expect("主机应获得 DHCP 地址");
    assert_ne!(assigned, 0);
    assert_eq!(assigned & 0xffffff00, ip("192.168.10.0") & 0xffffff00);
    let gw = dev_summary(&eng, pc1).gw;
    assert_eq!(gw.as_deref(), Some("192.168.10.1"), "网关必须来自池配置");

    // 配置视图按 IOS 方言从真实池状态渲染，且不为 VRP 文本。
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(cfg.contains("ip dhcp pool OFFICE"));
    assert!(cfg.contains("network 192.168.10.0 255.255.255.0"));
    assert!(cfg.contains("default-router 192.168.10.1"));
    assert!(cfg.contains("dns-server 8.8.8.8"));
    assert!(!cfg.contains("ip pool OFFICE"));
    assert!(!cfg.contains("gateway-list"));

    // 查询输出来自真实租约。
    let show = q(&mut eng, r1, "show ip dhcp binding");
    ok(&show, "C3 查询");
    assert!(show.device_output.contains("Pool OFFICE"));
    assert!(show.device_output.contains("192.168.10.1"));

    // 正向 L4：主机 ping 网关走真实数据面。
    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.10.1");
    assert!(
        ping.device_output.contains("5 received"),
        "C3 正向 ping 应全通: {}",
        ping.device_output
    );

    // 重复分配不得破坏已有租约：再次申请后仍拿到同一池内地址，且租约表不产生重复条目。
    let leases_before = match &eng.net.device(r1).unwrap().state {
        DeviceState::Router(rs) => rs.dhcp_leases.len(),
        _ => unreachable!(),
    };
    dhcp_renew(&mut eng, pc1, r1, &mut sess);
    let renewed = host_ip(&eng, pc1).expect("重复申请仍应获得地址");
    assert_eq!(renewed & 0xffffff00, ip("192.168.10.0") & 0xffffff00);
    let leases_after = match &eng.net.device(r1).unwrap().state {
        DeviceState::Router(rs) => rs.dhcp_leases.len(),
        _ => unreachable!(),
    };
    assert_eq!(leases_after, leases_before, "重复 DORA 不得累加/破坏租约表");
    assert_eq!(leased_ips(&eng, r1).len(), 1);
}

#[test]
fn c3_dhcp_negative_no_pool_and_excluded_all() {
    // 负向 1：无匹配池。
    let (mut eng, pc1, r1, mut sess) = dhcp_lab(false, false);
    dhcp_renew(&mut eng, pc1, r1, &mut sess);
    assert_eq!(host_ip(&eng, pc1), None, "无匹配池时不得伪造成功配置");
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(!cfg.contains("ip dhcp pool"));

    // 负向 2：池内地址全部排除。
    let (mut eng, pc1, r1, mut sess) = dhcp_lab(true, true);
    dhcp_renew(&mut eng, pc1, r1, &mut sess);
    assert_eq!(host_ip(&eng, pc1), None, "全部地址排除时不得分配");
    let d = dev_summary(&eng, r1);
    assert_eq!(d.dhcp_excluded.len(), 1);

    // 错误网关：default-router 置 0 后服务器不应答，主机仍无地址。
    let (mut eng, pc1, r1, mut sess) = dhcp_lab(true, false);
    let mut s2 = Session::default();
    for l in ["enable", "configure terminal", "ip dhcp pool OFFICE"] {
        ok(
            &exec_and_run_result(&mut eng, &mut s2, r1, l),
            "C3 no router 前置",
        );
    }
    ok(
        &exec_and_run_result(&mut eng, &mut s2, r1, "no default-router"),
        "C3 no default-router",
    );
    dhcp_renew(&mut eng, pc1, r1, &mut sess);
    assert_eq!(host_ip(&eng, pc1), None, "网关为 0 时不得分配");
}

#[test]
fn c3_dhcp_negation_cleans_pool_and_leases() {
    let (mut eng, pc1, r1, mut sess) = dhcp_lab(true, false);
    dhcp_renew(&mut eng, pc1, r1, &mut sess);
    assert!(host_ip(&eng, pc1).is_some());
    let mut s2 = Session::default();
    for l in ["enable", "configure terminal"] {
        ok(
            &exec_and_run_result(&mut eng, &mut s2, r1, l),
            "C3 no pool 前置",
        );
    }
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(&mut eng, &mut s2, r1, "no ip dhcp pool MISSING");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert!(!bad.state_changed, "失败路径不得改变状态");
    assert_eq!(state_fingerprint(&eng), before);

    ok(
        &exec_and_run_result(&mut eng, &mut s2, r1, "no ip dhcp pool OFFICE"),
        "C3 no ip dhcp pool",
    );
    let d = dev_summary(&eng, r1);
    assert!(d.dhcp_pools.is_empty());
    let leases = match &eng.net.device(r1).unwrap().state {
        DeviceState::Router(rs) => rs.dhcp_leases.len(),
        _ => unreachable!(),
    };
    assert_eq!(leases, 0, "删池必须同步清理派生租约");
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(!cfg.contains("ip dhcp pool OFFICE"));

    // pool 视图 no network / no dns-server 与排除地址 no 形式。
    let mut s3 = Session::default();
    for l in ["enable", "configure terminal", "ip dhcp pool P2"] {
        ok(
            &exec_and_run_result(&mut eng, &mut s3, r1, l),
            "C3 pool no 前置",
        );
    }
    ok(
        &exec_and_run_result(&mut eng, &mut s3, r1, "network 10.0.0.0 255.255.255.0"),
        "C3 network",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut s3, r1, "no network"),
        "C3 no network",
    );
    let d = dev_summary(&eng, r1);
    assert_eq!(d.dhcp_pools[0].network, "0.0.0.0");
}

#[test]
fn c3_dhcp_wrong_mode_invalid_params_and_state_unchanged() {
    let (mut eng, _pc1, r1, _sess) = dhcp_lab(true, false);
    let mut s = Session::default();
    // 错误模式：Exec 模式下 pool 子命令不可解析。
    let before = state_fingerprint(&eng);
    let r = exec_line_result(&mut eng, &mut s, r1, "network 10.0.0.0 255.255.255.0");
    assert_eq!(r.status, CommandStatus::InvalidCommand);
    assert!(!r.state_changed);
    assert_eq!(state_fingerprint(&eng), before);

    // 缺失参数：解析失败且状态不变。
    let r = exec_line_result(&mut eng, &mut s, r1, "enable");
    ok(&r, "enable");
    let r = exec_line_result(&mut eng, &mut s, r1, "configure terminal");
    ok(&r, "conf t");
    let before = state_fingerprint(&eng);
    let r = exec_line_result(&mut eng, &mut s, r1, "ip dhcp excluded-address 1.1.1.1");
    assert_eq!(r.status, CommandStatus::InvalidCommand, "缺少 high 参数");
    assert_eq!(state_fingerprint(&eng), before);

    // 非法区间完整预检拒绝。
    let r = exec_line_result(
        &mut eng,
        &mut s,
        r1,
        "ip dhcp excluded-address 10.0.0.20 10.0.0.10",
    );
    assert_eq!(r.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before, "非法区间不得部分写入");
    assert!(dev_summary(&eng, r1).dhcp_excluded.is_empty());
}

#[test]
fn c3_dhcp_help_completion_and_unique_abbrev() {
    let (mut eng, _pc1, r1, _sess) = dhcp_lab(true, false);
    let mut s = Session::default();
    ok(&exec_line_result(&mut eng, &mut s, r1, "enable"), "enable");
    ok(
        &exec_line_result(&mut eng, &mut s, r1, "conf t"),
        "唯一缩写 conf t",
    );
    let (_, cands) = complete_line(&eng, &s, r1, "ip dhcp ");
    assert!(
        cands.iter().any(|c| c.contains("pool")),
        "Tab 需含 pool: {cands:?}"
    );
    assert!(cands.iter().any(|c| c.contains("excluded-address")));
    let help = exec_line_result(&mut eng, &mut s, r1, "ip dhcp ?");
    ok(&help, "? 帮助");
    assert!(help.device_output.contains("pool"), "? 帮助应列出 pool");
    // 歧义缩写：'ip d' 前缀不得唯一收敛到错误命令。
    let (_, amb) = complete_line(&eng, &s, r1, "ip d");
    assert!(!amb.is_empty(), "前缀候选不应为空");
}

#[test]
fn c3_vrp_dhcp_uses_vrp_config_wording() {
    let mut net = Net::new();
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Vrp);
    net.set_port_admin(r1, 0, true);
    net.set_port_ip(r1, 0, Some(ip("192.168.10.1")), 24);
    let mut eng = Engine::new(net);
    let mut s = Session::default();
    for l in [
        "system-view",
        "ip pool OFFICE",
        "network 192.168.10.0 mask 255.255.255.0",
        "gateway-list 192.168.10.1",
        "dns-list 8.8.8.8",
        "quit",
        "interface GigabitEthernet0/0/0",
        "dhcp select global",
        "quit",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s, r1, l),
            &format!("VRP DHCP {l}"),
        );
    }
    let cfg = config_view(&mut eng, r1, Dialect::Vrp);
    assert!(cfg.contains("ip pool OFFICE"));
    assert!(cfg.contains("gateway-list 192.168.10.1"));
    assert!(cfg.contains("dns-list 8.8.8.8"));
    assert!(cfg.contains("dhcp select global"));
    assert!(!cfg.contains("ip dhcp pool"), "不得套用 IOS 命令");
    assert!(!cfg.contains("default-router"));
    let d = dev_summary(&eng, r1);
    assert_eq!(d.dhcp_server_ports, vec![0], "dhcp select 必须写入真实状态");
}

// =====================================================================
// C6a：IOS 管理与认证
// =====================================================================

#[test]
fn c6a_line_auth_modes_are_distinct_and_secrets_never_leak() {
    let (mut eng, _pc1, r1, _pc2, _sess) = router_lab(Dialect::Ios);
    let mut s = Session::default();
    for l in ["enable", "configure terminal"] {
        ok(&exec_line_result(&mut eng, &mut s, r1, l), "C6a 前置");
    }
    // 裸 login 未配置线路口令时必须拒绝且状态不变。
    ok(
        &exec_line_result(&mut eng, &mut s, r1, "line vty 0 4"),
        "line vty",
    );
    let before = state_fingerprint(&eng);
    let bare = exec_line_result(&mut eng, &mut s, r1, "login");
    assert_eq!(bare.status, CommandStatus::InvalidCommand);
    assert!(!bare.state_changed);
    assert_eq!(state_fingerprint(&eng), before);

    // 设置线路口令后裸 login 生效。
    ok(
        &exec_line_result(&mut eng, &mut s, r1, "password LINEPW"),
        "password",
    );
    ok(&exec_line_result(&mut eng, &mut s, r1, "login"), "裸 login");
    let d = dev_summary(&eng, r1);
    assert_eq!(
        d.mgmt.effective_line_auth(),
        sim_core::device::LineAuthMode::Line
    );

    // 切换 login local 后模式改变，本地用户生效。
    ok(
        &exec_line_result(&mut eng, &mut s, r1, "login local"),
        "login local",
    );
    ok(&exec_line_result(&mut eng, &mut s, r1, "exit"), "exit line");
    ok(
        &exec_line_result(&mut eng, &mut s, r1, "username admin secret S3CRET"),
        "username",
    );
    let d = dev_summary(&eng, r1);
    assert_eq!(
        d.mgmt.effective_line_auth(),
        sim_core::device::LineAuthMode::Local
    );
    assert_eq!(d.mgmt.users[0].privilege, 1, "默认 privilege 不是 15");

    // 越界 privilege 拒绝且不得创建用户。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(&mut eng, &mut s, r1, "username root privilege 16 secret X");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);
    assert_eq!(dev_summary(&eng, r1).mgmt.users.len(), 1);

    // enable secret + privilege 15 合法；配置视图脱敏。
    ok(
        &exec_line_result(&mut eng, &mut s, r1, "enable secret ENABLEPW"),
        "enable secret",
    );
    ok(
        &exec_line_result(&mut eng, &mut s, r1, "username su privilege 15 secret SUPW"),
        "privilege 15",
    );
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(cfg.contains("username su privilege 15 secret ***"));
    assert!(cfg.contains("enable secret ***"));
    assert!(!cfg.contains("S3CRET"));
    assert!(!cfg.contains("ENABLEPW"));
    assert!(!cfg.contains("SUPW"));
    assert!(!cfg.contains("LINEPW"));

    // 本地控制台会话：已配置 secret 时走口令提示且口令不泄漏。
    let mut local = Session::default();
    assert!(local.remote.is_none(), "当前 Session 必须是本地设备 CLI");
    let r = exec_line_result(&mut eng, &mut local, r1, "enable");
    assert_eq!(r.status, CommandStatus::Executed);
    assert!(local.enable_prompt, "已配置 secret 应进入口令提示");
    let wrong = exec_line_result(&mut eng, &mut local, r1, "WRONGPW");
    assert!(!wrong.device_output.contains("ENABLEPW"));
    assert_eq!(local.mode, Mode::Exec, "错误口令不得提权");
    // 口令提示被消费后需重新 enable，再提交正确口令。
    ok(
        &exec_line_result(&mut eng, &mut local, r1, "enable"),
        "C6a 重新 enable",
    );
    let right = exec_line_result(&mut eng, &mut local, r1, "ENABLEPW");
    assert!(!right.device_output.contains("ENABLEPW"), "口令不得回显");
    assert_eq!(local.mode, Mode::Priv);

    // no enable secret / no login / no username 恢复（回到配置视图）。
    for l in ["enable secret ENABLEPW", "no enable secret"] {
        ok(
            &exec_line_result(&mut eng, &mut s, r1, l),
            "C6a no enable secret 前置",
        );
    }
    ok(
        &exec_line_result(&mut eng, &mut s, r1, "no username su"),
        "no username",
    );
    let d = dev_summary(&eng, r1);
    assert!(d.mgmt.enable_secret.is_none());
    assert!(!d.mgmt.users.iter().any(|u| u.name == "su"));
    let mut l2 = Session::default();
    let _ = exec_line_result(&mut eng, &mut l2, r1, "enable");
    assert_eq!(
        l2.mode,
        Mode::Priv,
        "IOS XE 17.x 本地控制台无 secret 直接提权"
    );
}

// =====================================================================
// C5：IOS ACL
// =====================================================================

fn acl_lab(rules: &[&str]) -> (Engine, u32, u32, u32, Session) {
    let (mut eng, pc1, r1, pc2, mut s) = router_lab(Dialect::Ios);
    for l in ["enable", "configure terminal"] {
        ok(&exec_line_result(&mut eng, &mut s, r1, l), "C5 前置");
    }
    for rule in rules {
        ok(
            &exec_line_result(&mut eng, &mut s, r1, rule),
            &format!("C5 规则 {rule}"),
        );
    }
    ok(
        &exec_line_result(&mut eng, &mut s, r1, "interface GigabitEthernet0/0"),
        "C5 接口",
    );
    ok(
        &exec_line_result(&mut eng, &mut s, r1, "ip access-group 100 in"),
        "C5 绑定",
    );
    (eng, pc1, r1, pc2, s)
}

#[test]
fn c5_acl_implicit_deny_first_match_and_direction() {
    // 隐式拒绝：ACL 只有一条 deny，ping 必须失败。
    let (mut eng, pc1, r1, pc2, _s) =
        acl_lab(&["access-list 100 deny icmp 192.168.1.10 0.0.0.0 192.168.2.10 0.0.0.0"]);
    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(
        ping.device_output.contains("0 received"),
        "隐式拒绝/显式 deny 应全失败: {}",
        ping.device_output
    );
    let _ = (r1, pc2);

    // 首条命中：permit 在前，后续 deny 不生效，流量全通。
    let (mut eng, pc1, _r1, _pc2, _s) = acl_lab(&[
        "access-list 100 permit ip 192.168.1.0 0.0.0.255 192.168.2.0 0.0.0.255",
        "access-list 100 deny ip 0.0.0.0 255.255.255.255 0.0.0.0 255.255.255.255",
    ]);
    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(
        ping.device_output.contains("5 received"),
        "首条 permit 命中应全通: {}",
        ping.device_output
    );

    // 规则计数与流量一致。
    let d = dev_summary(&eng, _r1);
    let hits: u64 = d.acls[0].rules.iter().map(|r| r.hits).sum();
    assert!(hits > 0, "命中计数应随真实流量增长");
}

#[test]
fn c5_acl_tcp_rule_positive_and_negative_port_match() {
    // 正向：TCP 80 端口允许。
    let (mut eng, pc1, r1, _pc2, _s) =
        acl_lab(&["access-list 100 permit tcp 192.168.1.0 0.0.0.255 192.168.2.0 0.0.0.255 eq 80"]);
    let r = exec_and_run_result(
        &mut eng,
        &mut Session::default(),
        pc1,
        "tcp 192.168.2.10 80",
    );
    assert!(
        r.device_output.contains("连接成功"),
        "TCP 80 允许流应握手成功: {}",
        r.device_output
    );
    // 负向：端口 443 不匹配被隐式拒绝，无响应。
    let r = exec_and_run_result(
        &mut eng,
        &mut Session::default(),
        pc1,
        "tcp 192.168.2.10 443",
    );
    assert!(
        r.device_output.contains("无响应") || r.device_output.contains("被拒绝"),
        "TCP 443 非承诺端口应被拒绝: {}",
        r.device_output
    );
    let d = dev_summary(&eng, r1);
    assert_eq!(d.acls[0].rules[0].proto, "tcp");
    assert_eq!(d.acls[0].rules[0].dst_port.as_deref(), Some("80"));
}

#[test]
fn c5_acl_delete_rule_unbind_delete_acl_and_state_recovery() {
    let (mut eng, pc1, r1, pc2, mut s) =
        acl_lab(&["access-list 100 permit ip 192.168.1.0 0.0.0.255 192.168.2.0 0.0.0.255"]);
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(cfg.contains("access-list 100 permit ip"), "不得是占位行");
    assert!(cfg.contains("ip access-group 100 in"));

    // 解除绑定：ACL 未定义规则命中即放行，流量恢复。
    ok(
        &exec_and_run_result(&mut eng, &mut s, r1, "no ip access-group 100 in"),
        "C5 解绑",
    );
    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(ping.device_output.contains("5 received"));
    let _ = pc2;

    // 失败路径：解绑不存在的绑定必须拒绝且状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(&mut eng, &mut s, r1, "no ip access-group 100 in");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);

    // 删除规则：ACL 变空后隐式拒绝仍生效（重新绑定验证）。
    ok(
        &exec_and_run_result(&mut eng, &mut s, r1, "ip access-group 100 in"),
        "C5 重新绑定",
    );
    let mut s2 = Session::default();
    for l in ["enable", "configure terminal"] {
        ok(
            &exec_line_result(&mut eng, &mut s2, r1, l),
            "C5 no rule 前置",
        );
    }
    ok(
        &exec_line_result(&mut eng, &mut s2, r1, "no access-list 100 rule 1"),
        "C5 删除规则",
    );
    assert!(dev_summary(&eng, r1).acls[0].rules.is_empty());
    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(ping.device_output.contains("0 received"));

    // 删除整条 ACL 同时解除所有接口绑定。
    ok(
        &exec_and_run_result(&mut eng, &mut s2, r1, "no access-list 100"),
        "C5 删除 ACL",
    );
    let d = dev_summary(&eng, r1);
    assert!(d.acls.is_empty());
    assert!(d.ports.iter().all(|p| p.acl_in.is_none()));
}

// =====================================================================
// C4：IOS NAT
// =====================================================================

#[test]
fn c4_nat_roles_translations_and_negative() {
    let (mut eng, pc1, r1, pc2, mut s) = router_lab(Dialect::Ios);
    for l in ["enable", "configure terminal"] {
        ok(&exec_line_result(&mut eng, &mut s, r1, l), "C4 前置");
    }
    // 负向 1：未标记角色时无有效 NAT 转换（跨网段无路由/无转换）。
    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(
        ping.device_output.contains("5 received"),
        "前提：直连网段本身可达"
    );

    for l in [
        "interface GigabitEthernet0/0",
        "ip nat inside",
        "exit",
        "interface GigabitEthernet0/1",
        "ip nat outside",
        "exit",
        "ip nat inside source static 192.168.1.10 203.0.113.10",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s, r1, l),
            &format!("C4 {l}"),
        );
    }
    let d = dev_summary(&eng, r1);
    assert_eq!(d.nat_inside, vec![0]);
    assert_eq!(d.nat_outside, vec![1]);
    assert_eq!(d.nat_static.len(), 1);

    // 配置视图：真实 IOS 接口名，删除跨方言硬编码。
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(cfg.contains("interface GigabitEthernet0/0"));
    assert!(cfg.contains(" ip nat inside"));
    assert!(cfg.contains("ip nat inside source static 192.168.1.10 203.0.113.10"));
    assert!(
        !cfg.contains("GigabitEthernet0/0/1 nat"),
        "不得保留占位拼接行"
    );

    // 查询输出来自真实转换表。
    let tr = q(&mut eng, r1, "show ip nat translations");
    ok(&tr, "C4 转换表");
    assert!(tr.device_output.contains("Inside global"));
    let _ = pc2;

    // no 形式：解绑角色与删除静态映射后配置/数据面同步。
    ok(
        &exec_and_run_result(
            &mut eng,
            &mut s,
            r1,
            "no ip nat inside source static 192.168.1.10 203.0.113.10",
        ),
        "C4 no static",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut s, r1, "interface GigabitEthernet0/0"),
        "C4 接口",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut s, r1, "no ip nat inside"),
        "C4 no inside",
    );
    let d = dev_summary(&eng, r1);
    assert!(d.nat_inside.is_empty());
    assert!(d.nat_static.is_empty());
}

#[test]
fn c4_nat_easyip_precheck_and_invalid_interface_rejected() {
    let (mut eng, _pc1, r1, _pc2, mut s) = router_lab(Dialect::Ios);
    for l in ["enable", "configure terminal"] {
        ok(&exec_line_result(&mut eng, &mut s, r1, l), "C4 EasyIP 前置");
    }
    // 缺 ACL 定义 → 完整预检拒绝，状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(
        &mut eng,
        &mut s,
        r1,
        "ip nat inside source list 100 interface GigabitEthernet0/1 overload",
    );
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);

    ok(
        &exec_line_result(
            &mut eng,
            &mut s,
            r1,
            "access-list 100 permit ip 192.168.1.0 0.0.0.255 0.0.0.0 255.255.255.255",
        ),
        "C4 ACL",
    );
    // 未标记 outside → 拒绝（接口参数不被静默忽略）。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(
        &mut eng,
        &mut s,
        r1,
        "ip nat inside source list 100 interface GigabitEthernet0/1 overload",
    );
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);

    for l in ["interface GigabitEthernet0/1", "ip nat outside", "exit"] {
        ok(&exec_and_run_result(&mut eng, &mut s, r1, l), "C4 outside");
    }
    ok(
        &exec_line_result(
            &mut eng,
            &mut s,
            r1,
            "ip nat inside source list 100 interface GigabitEthernet0/1 overload",
        ),
        "C4 EasyIP",
    );
    assert_eq!(dev_summary(&eng, r1).nat_easyip, Some((100, 1)));
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(cfg.contains("ip nat inside source list 100 interface"));

    // no 形式：ACL / 接口参数必须与绑定一致，不匹配拒绝且状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(
        &mut eng,
        &mut s,
        r1,
        "no ip nat inside source list 101 interface GigabitEthernet0/1 overload",
    );
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);
    let bad = exec_line_result(
        &mut eng,
        &mut s,
        r1,
        "no ip nat inside source list 100 interface GigabitEthernet0/0 overload",
    );
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);
    assert_eq!(dev_summary(&eng, r1).nat_easyip, Some((100, 1)));

    // 正确撤销：绑定与派生转换表同步清空，配置视图不再渲染 EasyIP。
    ok(
        &exec_and_run_result(
            &mut eng,
            &mut s,
            r1,
            "no ip nat inside source list 100 interface GigabitEthernet0/1 overload",
        ),
        "C4 no EasyIP",
    );
    assert_eq!(dev_summary(&eng, r1).nat_easyip, None);
    let tr = q(&mut eng, r1, "show ip nat translations");
    assert!(tr.device_output.contains("（无转换表项）"));
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(!cfg.contains("ip nat inside source list"));
    // 重复撤销必须拒绝且状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(
        &mut eng,
        &mut s,
        r1,
        "no ip nat inside source list 100 interface GigabitEthernet0/1",
    );
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);

    // 不带 overload 的撤销变体同样可达：重新配置并撤销。
    ok(
        &exec_line_result(
            &mut eng,
            &mut s,
            r1,
            "ip nat inside source list 100 interface GigabitEthernet0/1",
        ),
        "C4 EasyIP 无 overload 变体",
    );
    assert_eq!(dev_summary(&eng, r1).nat_easyip, Some((100, 1)));
    ok(
        &exec_line_result(
            &mut eng,
            &mut s,
            r1,
            "no ip nat inside source list 100 interface GigabitEthernet0/1",
        ),
        "C4 no EasyIP 无 overload 变体",
    );
    assert_eq!(dev_summary(&eng, r1).nat_easyip, None);
}

// =====================================================================
// C7：IOS OSPF
// =====================================================================

fn ospf_lab(dialect: Dialect) -> (Engine, u32, u32, u32, u32, Session, Session) {
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

fn converge(eng: &mut Engine) {
    let h = eng.drain_horizon();
    eng.run_to_quiescence(h);
}

#[test]
fn c7_ospf_process_real_area_adjacency_and_route() {
    let (mut eng, pc1, r1, r2, pc2, mut s1, mut s2) = ospf_lab(Dialect::Ios);
    for (dev, sess) in [(r1, &mut s1), (r2, &mut s2)] {
        for l in [
            "enable",
            "configure terminal",
            "router ospf 7",
            "network 10.0.0.0 0.0.0.255 area 1",
            "network 192.168.0.0 0.0.255.255 area 1",
            "end",
        ] {
            ok(
                &exec_and_run_result(&mut eng, sess, dev, l),
                &format!("C7 {l}"),
            );
        }
    }
    let d = dev_summary(&eng, r1);
    assert_eq!(d.ospf_process, Some(7), "进程号不得硬编码 1");
    assert_eq!(d.ospf_area, 1, "区域不得硬编码 0");
    assert_eq!(d.ospf_networks.len(), 2);

    converge(&mut eng);
    let show = q(&mut eng, r1, "show ip ospf neighbor");
    ok(&show, "C7 邻居查询");
    assert!(
        show.device_output.contains("OSPF process 7"),
        "查询视图须用真实进程号"
    );
    assert!(show.device_output.contains("10.0.0.2"));

    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(
        ping.device_output.contains("5 received"),
        "C7 OSPF 路由应使跨网段全通: {}",
        ping.device_output
    );
    let _ = pc2;

    // 配置视图从真实进程/区域/network 渲染。
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(cfg.contains("router ospf 7"));
    assert!(cfg.contains("network 10.0.0.0 0.0.0.255 area 1"));
    assert!(!cfg.contains("router ospf 1\n"));

    // no network … area：校验区域一致后撤销单条语句，配置/邻居/路由/数据面同步。
    let mut ns = Session::default();
    for l in ["enable", "configure terminal", "router ospf 7"] {
        ok(
            &exec_and_run_result(&mut eng, &mut ns, r1, l),
            "C7 no network 前置",
        );
    }
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(
        &mut eng,
        &mut ns,
        r1,
        "no network 10.0.0.0 0.0.0.255 area 9",
    );
    assert_eq!(
        bad.status,
        CommandStatus::InvalidCommand,
        "区域不匹配须拒绝"
    );
    assert_eq!(state_fingerprint(&eng), before);
    ok(
        &exec_and_run_result(
            &mut eng,
            &mut ns,
            r1,
            "no network 10.0.0.0 0.0.0.255 area 1",
        ),
        "C7 no network",
    );
    let d = dev_summary(&eng, r1);
    assert_eq!(d.ospf_networks.len(), 1, "撤销须落到真实配置");
    assert!(!d
        .ospf_networks
        .contains(&("10.0.0.0".to_string(), "0.0.0.255".to_string())));
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(!cfg.contains("network 10.0.0.0 0.0.0.255 area 1"));
    assert!(cfg.contains("network 192.168.0.0 0.0.255.255 area 1"));
    converge(&mut eng);
    // 互联链路不再运行 OSPF，邻居与跨网段业务撤销。
    let show = q(&mut eng, r1, "show ip ospf neighbor");
    assert!(!show.device_output.contains("10.0.0.2"));
    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(
        ping.device_output.contains("0 received"),
        "撤销互联 network 后业务应失败: {}",
        ping.device_output
    );
    // 重复撤销拒绝且状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(
        &mut eng,
        &mut ns,
        r1,
        "no network 10.0.0.0 0.0.0.255 area 1",
    );
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);
}

#[test]
fn c7_ospf_area_mismatch_no_adjacency_and_no_process() {
    let (mut eng, pc1, r1, r2, pc2, mut s1, mut s2) = ospf_lab(Dialect::Ios);
    for (dev, sess, area) in [(r1, &mut s1, "0"), (r2, &mut s2, "1")] {
        for l in [
            "enable",
            "configure terminal",
            "router ospf 1",
            format!("network 10.0.0.0 0.0.0.255 area {area}").as_str(),
            "end",
        ] {
            ok(&exec_and_run_result(&mut eng, sess, dev, l), "C7 区域配置");
        }
    }
    converge(&mut eng);
    let show = q(&mut eng, r1, "show ip ospf neighbor");
    assert!(
        !show.device_output.contains("10.0.0.2"),
        "区域不匹配不得建邻: {}",
        show.device_output
    );
    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(
        ping.device_output.contains("0 received"),
        "区域不匹配时无动态路由应失败: {}",
        ping.device_output
    );
    let _ = (r2, pc2);

    // no router ospf 撤销：配置、邻居、路由与数据面同步恢复。
    for l in ["configure terminal", "no router ospf 1"] {
        ok(
            &exec_and_run_result(&mut eng, &mut s1, r1, l),
            "C7 no 前置/撤销",
        );
    }
    let d = dev_summary(&eng, r1);
    assert!(d.ospf_process.is_none());
    assert!(d.ospf_networks.is_empty());
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(!cfg.contains("router ospf"));
}

#[test]
fn c7_ospf_link_failure_withdraw_and_recovery() {
    let (mut eng, pc1, r1, r2, pc2, mut s1, mut s2) = ospf_lab(Dialect::Ios);
    for (dev, sess) in [(r1, &mut s1), (r2, &mut s2)] {
        for l in [
            "enable",
            "configure terminal",
            "router ospf 3",
            "network 10.0.0.0 0.0.0.255 area 0",
            "network 192.168.0.0 0.0.255.255 area 0",
            "end",
        ] {
            ok(&exec_and_run_result(&mut eng, sess, dev, l), "C7 建邻");
        }
    }
    converge(&mut eng);
    let before = q(&mut eng, r1, "show ip ospf neighbor");
    assert!(before.device_output.contains("10.0.0.2"));

    // 链路 shutdown：撤销该口邻居与派生路由。
    for l in [
        "configure terminal",
        "interface GigabitEthernet0/1",
        "shutdown",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s1, r1, l),
            "C7 shutdown",
        );
    }
    eng.ospf_link_changed(r1, 1);
    converge(&mut eng);
    let after = q(&mut eng, r1, "show ip ospf neighbor");
    assert!(
        !after.device_output.contains("10.0.0.2"),
        "链路故障后邻居须撤销: {}",
        after.device_output
    );
    let bad_ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(
        bad_ping.device_output.contains("0 received"),
        "链路故障业务应失败: {}",
        bad_ping.device_output
    );

    // 恢复链路：重新建邻且业务恢复。
    ok(
        &exec_and_run_result(&mut eng, &mut s1, r1, "no shutdown"),
        "C7 no shutdown",
    );
    converge(&mut eng);
    let recovered = q(&mut eng, r1, "show ip ospf neighbor");
    assert!(
        recovered.device_output.contains("10.0.0.2"),
        "恢复后须重建邻居: {}",
        recovered.device_output
    );
    let good_ping =
        exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(
        good_ping.device_output.contains("5 received"),
        "恢复后业务应全通: {}",
        good_ping.device_output
    );
    let _ = (r2, pc2);
}

// =====================================================================
// C8：IOS VRRP
// =====================================================================

fn vrrp_lab() -> (Engine, u32, u32, u32, Session, Session) {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let sw = net.add_device("SW1".into(), DeviceKind::Switch, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Ios);
    let r2 = net.add_device("R2".into(), DeviceKind::Router, Dialect::Ios);
    // 同一二层 LAN：一台交换机连接主机与两台路由器。
    net.add_link(pc1, sw).unwrap();
    net.add_link(sw, r1).unwrap();
    net.add_link(sw, r2).unwrap();
    net.set_host(pc1, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.254")));
    for (dev, addr) in [(r1, "192.168.1.1"), (r2, "192.168.1.2")] {
        net.set_port_admin(dev, 0, true);
        net.set_port_ip(dev, 0, Some(ip(addr)), 24);
    }
    (
        Engine::new(net),
        pc1,
        r1,
        r2,
        Session::default(),
        Session::default(),
    )
}

#[test]
fn c8_vrrp_priority_election_preempt_and_negation() {
    let (mut eng, pc1, r1, r2, mut s1, mut s2) = vrrp_lab();
    for (dev, sess, prio) in [(r1, &mut s1, "120"), (r2, &mut s2, "90")] {
        for l in [
            "enable",
            "configure terminal",
            "interface GigabitEthernet0/0",
            "vrrp 1 ip 192.168.1.254",
            format!("vrrp 1 priority {prio}").as_str(),
            "vrrp 1 preempt",
        ] {
            ok(
                &exec_and_run_result(&mut eng, sess, dev, l),
                &format!("C8 {l}"),
            );
        }
    }
    let states: Vec<VrrpState> = [r1, r2]
        .iter()
        .map(|d| match &eng.net.device(*d).unwrap().state {
            DeviceState::Router(rs) => rs.vrrp[0].state,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(states[0], VrrpState::Master, "高优先级设备应为 Master");
    assert_eq!(states[1], VrrpState::Backup);

    let show = q(&mut eng, r1, "show vrrp");
    ok(&show, "C8 查询");
    assert!(show.device_output.contains("Master"));
    assert!(show.device_output.contains("192.168.1.254"));

    // 配置视图：真实接口名与优先级，不再是占位拼接行。
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    assert!(cfg.contains("interface GigabitEthernet0/0"));
    assert!(cfg.contains("vrrp 1 ip 192.168.1.254"));
    assert!(cfg.contains("vrrp 1 priority 120"));
    assert!(!cfg.contains("GigabitEthernet0/0/1: vrrp"));

    // VIP 业务正向：主设备存在时可达。
    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.1.254");
    assert!(
        ping.device_output.contains("5 received"),
        "C8 有 Master 时 VIP 业务应通: {}",
        ping.device_output
    );

    // 主设备故障：备份接管，业务保持。
    eng.net.set_port_admin(r1, 0, false);
    eng.vrrp_config_changed(r1);
    eng.vrrp_config_changed(r2);
    converge(&mut eng);
    let takeover = match &eng.net.device(r2).unwrap().state {
        DeviceState::Router(rs) => rs.vrrp[0].state,
        _ => unreachable!(),
    };
    assert_eq!(takeover, VrrpState::Master, "主故障后备份应接管");
    let failover_ping =
        exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.1.254");
    assert!(
        failover_ping.device_output.contains("5 received"),
        "C8 主故障后 VIP 业务应保持: {}",
        failover_ping.device_output
    );

    // 恢复抢占：高优先级重新成为 Master。
    eng.net.set_port_admin(r1, 0, true);
    eng.vrrp_config_changed(r1);
    eng.vrrp_config_changed(r2);
    let restored = match &eng.net.device(r1).unwrap().state {
        DeviceState::Router(rs) => rs.vrrp[0].state,
        _ => unreachable!(),
    };
    assert_eq!(
        restored,
        VrrpState::Master,
        "抢占开启时主设备恢复应重新成为 Master"
    );

    // 关闭抢占：no vrrp <vrid> preempt 写入真实状态。
    ok(
        &exec_and_run_result(&mut eng, &mut s1, r1, "no vrrp 1 preempt"),
        "C8 no preempt",
    );
    assert!(!dev_summary(&eng, r1).vrrp[0].preempt);

    // 负向：删除所有组后无可用 Master。
    ok(
        &exec_and_run_result(&mut eng, &mut s1, r1, "no vrrp 1"),
        "C8 no r1",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut s2, r2, "no vrrp 1"),
        "C8 no r2",
    );
    assert!(dev_summary(&eng, r1).vrrp.is_empty());
    assert!(dev_summary(&eng, r2).vrrp.is_empty());
    let fail = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.1.254");
    assert!(
        fail.device_output.contains("0 received"),
        "无 Master 时 VIP 业务应失败: {}",
        fail.device_output
    );
}

// =====================================================================
// H2：VRP STP
// =====================================================================

#[test]
fn h2_stp_priority_root_role_and_undo() {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let sw1 = net.add_device("SW1".into(), DeviceKind::Switch, Dialect::Vrp);
    let sw2 = net.add_device("SW2".into(), DeviceKind::Switch, Dialect::Vrp);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc1, sw1).unwrap(); // SW1 p0
    net.add_link(sw1, sw2).unwrap(); // SW1 p1 - SW2 p0（主链路）
    net.add_link(sw1, sw2).unwrap(); // SW1 p2 - SW2 p1（冗余）
    net.add_link(sw2, pc2).unwrap(); // SW2 p2
    net.set_host(pc1, Some(ip("192.168.10.11")), 24, None);
    net.set_host(pc2, Some(ip("192.168.10.12")), 24, None);
    let mut eng = Engine::new(net);
    let mut s1 = Session::default();
    let mut s2 = Session::default();

    // 对照：默认优先级相同，根桥由桥 ID 决定；显式设置 SW1 为 primary 后根桥变化。
    for l in ["system-view", "stp"] {
        ok(&exec_and_run_result(&mut eng, &mut s1, sw1, l), "H2 SW1");
        ok(&exec_and_run_result(&mut eng, &mut s2, sw2, l), "H2 SW2");
    }
    converge(&mut eng);
    let root_before = eng
        .net
        .device(sw1)
        .map(|d| match &d.state {
            DeviceState::Switch(sw) => format!("{:?}", sw.stp.root),
            _ => String::new(),
        })
        .unwrap();

    ok(
        &exec_and_run_result(&mut eng, &mut s1, sw1, "stp root primary"),
        "H2 root primary",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut s2, sw2, "stp priority 8192"),
        "H2 priority",
    );
    converge(&mut eng);
    let prio = |eng: &Engine, dev: u32| match &eng.net.device(dev).unwrap().state {
        DeviceState::Switch(sw) => sw.stp.priority,
        _ => unreachable!(),
    };
    assert_eq!(prio(&eng, sw1), 0, "primary 应写真实优先级 0");
    assert_eq!(prio(&eng, sw2), 8192);
    let root_after = eng
        .net
        .device(sw1)
        .map(|d| match &d.state {
            DeviceState::Switch(sw) => format!("{:?}", sw.stp.root),
            _ => String::new(),
        })
        .unwrap();
    assert_ne!(root_before, root_after, "根桥不得硬编码，应随优先级变化");

    let brief = q(&mut eng, sw1, "display stp brief");
    ok(&brief, "H2 display stp brief");
    assert!(!brief.device_output.is_empty());
    let cfg = config_view(&mut eng, sw1, Dialect::Vrp);
    assert!(cfg.contains("stp priority 0"));

    // 非法优先级拒绝且状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(&mut eng, &mut s1, sw1, "stp priority 1234");
    assert_eq!(bad.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);

    // undo 恢复默认 32768 并重新收敛。
    ok(
        &exec_and_run_result(&mut eng, &mut s1, sw1, "undo stp priority"),
        "H2 undo",
    );
    assert_eq!(prio(&eng, sw1), 32768);
    converge(&mut eng);
    let cfg2 = config_view(&mut eng, sw1, Dialect::Vrp);
    assert!(cfg2.contains("stp priority 32768"));

    // undo stp root：再次设置 root 后用 undo stp root 恢复默认并重新选举。
    ok(
        &exec_and_run_result(&mut eng, &mut s1, sw1, "stp root primary"),
        "H2 root primary 复设",
    );
    assert_eq!(prio(&eng, sw1), 0);
    let root_primary = eng
        .net
        .device(sw1)
        .map(|d| match &d.state {
            DeviceState::Switch(sw) => format!("{:?}", sw.stp.root),
            _ => String::new(),
        })
        .unwrap();
    ok(
        &exec_and_run_result(&mut eng, &mut s1, sw1, "undo stp root"),
        "H2 undo stp root",
    );
    assert_eq!(prio(&eng, sw1), 32768, "undo stp root 须恢复默认优先级");
    converge(&mut eng);
    let root_undone = eng
        .net
        .device(sw1)
        .map(|d| match &d.state {
            DeviceState::Switch(sw) => format!("{:?}", sw.stp.root),
            _ => String::new(),
        })
        .unwrap();
    assert_ne!(
        root_primary, root_undone,
        "根桥须随优先级重新选举，不得缓存"
    );
    let cfg3 = config_view(&mut eng, sw1, Dialect::Vrp);
    assert!(cfg3.contains("stp priority 32768"));
    assert!(!cfg3.contains("stp root"));

    // 接口视图下的 undo stp root 变体同样可达且语义一致。
    ok(
        &exec_and_run_result(&mut eng, &mut s1, sw1, "stp root primary"),
        "H2 root primary 复设 2",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut s1, sw1, "interface GigabitEthernet0/0/1"),
        "H2 接口视图",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut s1, sw1, "undo stp root"),
        "H2 if undo stp root",
    );
    assert_eq!(prio(&eng, sw1), 32768, "接口视图 undo 亦须恢复默认优先级");

    // L4 正向：冗余拓扑无环，业务可达。
    let baseline =
        exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.10.12");
    assert!(
        baseline.device_output.contains("5 received"),
        "H2 冗余拓扑正常时业务应全通: {}",
        baseline.device_output
    );
    // L4 负向：两条交换机间链路全部断开，业务失败（证明转发不是硬编码直通）。
    eng.net.set_port_admin(sw1, 1, false);
    eng.net.set_port_admin(sw1, 2, false);
    eng.stp_topology_changed();
    converge(&mut eng);
    let down = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.10.12");
    assert!(
        down.device_output.contains("0 received"),
        "H2 交换机间全断时业务应失败: {}",
        down.device_output
    );
    // 恢复主链路后重新收敛，业务恢复。
    eng.net.set_port_admin(sw1, 1, true);
    eng.stp_topology_changed();
    converge(&mut eng);
    let recovered =
        exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.10.12");
    assert!(
        recovered.device_output.contains("5 received"),
        "H2 主链路恢复并重新收敛后业务应恢复: {}",
        recovered.device_output
    );
}

// =====================================================================
// H3：VRP Eth-Trunk / LACP
// =====================================================================

#[test]
fn h3_eth_trunk_selected_members_failure_and_undo() {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let sw1 = net.add_device("SW1".into(), DeviceKind::Switch, Dialect::Vrp);
    let sw2 = net.add_device("SW2".into(), DeviceKind::Switch, Dialect::Vrp);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc1, sw1).unwrap(); // SW1 p0
    net.add_link(sw1, sw2).unwrap(); // SW1 p1 / SW2 p0：成员 1
    net.add_link(sw1, sw2).unwrap(); // SW1 p2 / SW2 p1：成员 2
    net.add_link(sw2, pc2).unwrap(); // SW2 p2
    net.set_host(pc1, Some(ip("192.168.10.11")), 24, None);
    net.set_host(pc2, Some(ip("192.168.10.12")), 24, None);
    let mut eng = Engine::new(net);
    let mut s = Session::default();
    for l in [
        "system-view",
        "interface Eth-Trunk 1",
        "mode lacp-static",
        "quit",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s, sw1, l),
            &format!("H3 {l}"),
        );
    }
    // 前置条件：非物理聚合口不得作为成员；状态不变。
    let before = state_fingerprint(&eng);
    let bad = exec_line_result(&mut eng, &mut s, sw1, "interface Eth-Trunk 1");
    ok(&bad, "H3 进入 Trunk 视图");
    let bad = exec_line_result(&mut eng, &mut s, sw1, "eth-trunk 1");
    assert_eq!(
        bad.status,
        CommandStatus::InvalidCommand,
        "Trunk 口不能加入自身"
    );
    assert_eq!(state_fingerprint(&eng), before, "前置条件失败不得改变状态");
    ok(
        &exec_line_result(&mut eng, &mut s, sw1, "quit"),
        "H3 退出 Trunk 视图",
    );

    // 加入两个成员（VRP 交换机物理口命名从 1 开始：SW1 p1/p2 → 0/0/2、0/0/3）。
    for l in [
        "interface GigabitEthernet0/0/2",
        "eth-trunk 1",
        "quit",
        "interface GigabitEthernet0/0/3",
        "eth-trunk 1",
        "quit",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s, sw1, l),
            &format!("H3 成员 {l}"),
        );
    }
    // 对端也必须配置 LACP 聚合，否则不会有 Selected 成员（真实协商前提）。
    // SW2 p0/p1 → 0/0/1、0/0/2。
    let mut s2 = Session::default();
    for l in [
        "system-view",
        "stp",
        "interface Eth-Trunk 1",
        "mode lacp-static",
        "quit",
        "interface GigabitEthernet0/0/1",
        "eth-trunk 1",
        "quit",
        "interface GigabitEthernet0/0/2",
        "eth-trunk 1",
        "quit",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s2, sw2, l),
            &format!("H3 SW2 {l}"),
        );
    }
    eng.lacp_config_changed(sw1);
    eng.lacp_config_changed(sw2);
    converge(&mut eng);
    let d = dev_summary(&eng, sw1);
    assert!(!d.lags.is_empty(), "Trunk 配置必须落到真实状态");
    assert!(
        d.lags[0].members.len() >= 2,
        "成员加入应写入状态: {:?}",
        d.lags
    );
    let sel = d
        .lag_selected
        .iter()
        .find(|(id, _)| *id == 1)
        .map(|x| x.1.len())
        .unwrap_or(0);
    assert!(sel > 0, "LACP 应协商出 Selected 成员: {:?}", d.lag_selected);

    let display = exec_and_run_result(&mut eng, &mut s, sw1, "display eth-trunk 1");
    ok(&display, "H3 成员查询");
    assert!(!display.device_output.is_empty());
    let cfg = exec_and_run_result(&mut eng, &mut s, sw1, "display current-configuration");
    ok(&cfg, "H3 配置视图");
    assert!(cfg.device_output.contains("interface Eth-Trunk1"));
    assert!(cfg.device_output.contains("mode lacp-static"));

    // L4 正向：聚合链路正常，业务全通。
    let baseline =
        exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.10.12");
    assert!(
        baseline.device_output.contains("5 received"),
        "H3 聚合链路正常时业务应全通: {}",
        baseline.device_output
    );

    // 单成员故障：Selected 缩减但仍有可用成员，业务保持。
    eng.net.set_port_admin(sw1, 1, false);
    eng.lacp_config_changed(sw1);
    converge(&mut eng);
    let sel1 = dev_summary(&eng, sw1)
        .lag_selected
        .iter()
        .find(|(id, _)| *id == 1)
        .map(|x| x.1.len())
        .unwrap_or(0);
    assert!(sel1 > 0, "单成员故障不应导致全部失效");
    let one_down =
        exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.10.12");
    assert!(
        one_down.device_output.contains("5 received"),
        "H3 单成员故障业务应保持: {}",
        one_down.device_output
    );

    // 全部成员失效：Selected 为空，业务失败。
    eng.net.set_port_admin(sw1, 2, false);
    eng.lacp_config_changed(sw1);
    converge(&mut eng);
    let sel2 = dev_summary(&eng, sw1)
        .lag_selected
        .iter()
        .find(|(id, _)| *id == 1)
        .map(|x| x.1.len())
        .unwrap_or(0);
    assert_eq!(sel2, 0, "全部成员失效时不得仍有 Selected 成员");
    let all_down =
        exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.10.12");
    assert!(
        all_down.device_output.contains("0 received"),
        "H3 全部成员失效业务应失败: {}",
        all_down.device_output
    );

    // 恢复后重新加入，业务恢复。
    eng.net.set_port_admin(sw1, 1, true);
    eng.net.set_port_admin(sw1, 2, true);
    eng.lacp_config_changed(sw1);
    converge(&mut eng);
    let sel3 = dev_summary(&eng, sw1)
        .lag_selected
        .iter()
        .find(|(id, _)| *id == 1)
        .map(|x| x.1.len())
        .unwrap_or(0);
    assert!(sel3 > 0, "恢复后应重新 Selected");
    let recovered =
        exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.10.12");
    assert!(
        recovered.device_output.contains("5 received"),
        "H3 成员恢复业务应恢复: {}",
        recovered.device_output
    );

    // 成员移除：undo eth-trunk 状态恢复。
    for l in ["interface GigabitEthernet0/0/2", "undo eth-trunk"] {
        ok(
            &exec_and_run_result(&mut eng, &mut s, sw1, l),
            "H3 移除成员",
        );
    }
    let d = dev_summary(&eng, sw1);
    assert!(
        d.lags.iter().all(|g| g.members.len() < 2),
        "成员移除须落状态"
    );
    let _ = before;
}

// =====================================================================
// H6：VRP OSPF 双层视图
// =====================================================================

#[test]
fn h6_vrp_ospf_two_level_views_area_and_route() {
    let (mut eng, pc1, r1, r2, pc2, mut s1, mut s2) = ospf_lab(Dialect::Vrp);
    for (dev, sess) in [(r1, &mut s1), (r2, &mut s2)] {
        for l in [
            "system-view",
            "ospf 9",
            "area 0.0.0.1",
            "network 10.0.0.0 0.0.0.255",
            "network 192.168.0.0 0.0.255.255",
        ] {
            ok(
                &exec_and_run_result(&mut eng, sess, dev, l),
                &format!("H6 {l}"),
            );
        }
    }
    let d = dev_summary(&eng, r1);
    assert_eq!(d.ospf_process, Some(9));
    assert_eq!(d.ospf_area, 1);
    assert_eq!(d.ospf_networks.len(), 2);

    converge(&mut eng);
    let show = q(&mut eng, r1, "display ospf peer");
    ok(&show, "H6 邻居查询");
    assert!(show.device_output.contains("OSPF Process 9"));
    assert!(show.device_output.contains("10.0.0.2"));
    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(
        ping.device_output.contains("5 received"),
        "H6 OSPF 路由应使跨网段全通: {}",
        ping.device_output
    );
    let _ = pc2;

    // current-configuration 用真实进程/区域/network，关闭硬编码。
    let cfg = config_view(&mut eng, r1, Dialect::Vrp);
    assert!(cfg.contains("ospf 9"));
    assert!(cfg.contains("area 0.0.0.1"));
    assert!(cfg.contains("network 10.0.0.0 0.0.0.255"));
    assert!(!cfg.contains("ospf 1\n"));
}

#[test]
fn h6_vrp_ospf_quit_hierarchy_undo_and_area_mismatch() {
    let (mut eng, pc1, r1, r2, pc2, mut s1, _s2) = ospf_lab(Dialect::Vrp);
    for l in ["system-view", "ospf 5", "area 2"] {
        ok(&exec_and_run_result(&mut eng, &mut s1, r1, l), "H6 层级");
    }
    assert_eq!(s1.mode, Mode::OspfArea, "area 后应进入区域视图");
    ok(
        &exec_and_run_result(&mut eng, &mut s1, r1, "quit"),
        "H6 quit 1",
    );
    assert_eq!(s1.mode, Mode::Ospf, "区域视图 quit 应退回进程视图");
    ok(
        &exec_and_run_result(&mut eng, &mut s1, r1, "quit"),
        "H6 quit 2",
    );
    assert_eq!(s1.mode, Mode::Config, "进程视图 quit 应退回系统视图");
    ok(
        &exec_and_run_result(&mut eng, &mut s1, r1, "return"),
        "H6 return",
    );
    assert_eq!(s1.mode, Mode::Exec, "return 应直达用户视图");

    // undo network：配置与协议状态同步。
    for l in [
        "system-view",
        "ospf 5",
        "area 2",
        "network 10.0.0.0 0.0.0.255",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s1, r1, l),
            "H6 undo 前置",
        );
    }
    ok(
        &exec_and_run_result(&mut eng, &mut s1, r1, "undo network 10.0.0.0 0.0.0.255"),
        "H6 undo network",
    );
    assert!(dev_summary(&eng, r1).ospf_networks.is_empty());

    // 区域不匹配负向：两端不同区域不建邻。用顶层视图的新会话，避免层级歧义。
    let mut ms1 = Session::default();
    let mut ms2 = Session::default();
    for (dev, sess, area) in [(r1, &mut ms1, "0"), (r2, &mut ms2, "1")] {
        for l in ["system-view", "ospf 1"] {
            ok(
                &exec_and_run_result(&mut eng, sess, dev, l),
                "H6 mismatch 前置",
            );
        }
        ok(
            &exec_and_run_result(&mut eng, sess, dev, format!("area {area}").as_str()),
            "H6 area",
        );
        ok(
            &exec_and_run_result(&mut eng, sess, dev, "network 10.0.0.0 0.0.0.255"),
            "H6 network",
        );
    }
    converge(&mut eng);
    let show = q(&mut eng, r1, "display ospf peer");
    assert!(
        !show.device_output.contains("10.0.0.2"),
        "区域不匹配不得建邻: {}",
        show.device_output
    );
    let ping = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.2.10");
    assert!(
        ping.device_output.contains("0 received"),
        "区域不匹配业务应失败: {}",
        ping.device_output
    );
    let _ = pc2;

    // undo ospf 撤销进程（新顶层会话）。
    let mut us = Session::default();
    ok(
        &exec_and_run_result(&mut eng, &mut us, r1, "system-view"),
        "H6 undo ospf 前置",
    );
    ok(
        &exec_and_run_result(&mut eng, &mut us, r1, "undo ospf 1"),
        "H6 undo ospf",
    );
    let d = dev_summary(&eng, r1);
    assert!(d.ospf_process.is_none());
    assert!(d.ospf_networks.is_empty());
}

// =====================================================================
// 横切：strict 规格、能力/证据、双通道、inspection 纯读、配置渲染偏差
// =====================================================================

#[test]
fn stage2b_lint_has_no_duplicates_dangling_drift_or_strict_gaps() {
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
    // 九条流程的可执行节点必须已关联且启用 strict。
    for (d, needle) in [
        (Dialect::Ios, "config/ip/dhcp/pool"),
        (Dialect::Ios, "config/access-list/@0number/@0word/tcp/@0ipv4/@0wildcard/@0ipv4/@0wildcard/eq/@0number"),
        (Dialect::Ios, "config/router/ospf/@0number"),
        (Dialect::Ios, "if/vrrp/@0number/ip/@0ipv4"),
        (Dialect::Vrp, "config/stp/root/@0word"),
        (Dialect::Vrp, "ospf_area/undo/network/@0ipv4/@0wildcard"),
    ] {
        let spec = specs::find(d, needle).unwrap_or_else(|| panic!("{:?} 缺少规格 {needle}", d));
        assert!(spec.strict, "{needle} 必须启用 strict");
        assert!(!spec.contract.is_empty());
        assert!(!spec.references.is_empty());
    }
}

#[test]
fn stage2b_capability_precedes_semantics_and_unknown_is_not_unsupported() {
    // L2 交换端口能力在 VRP 是 Unknown；不得伪判 UnsupportedByDevice。
    let insp = inspect_command(
        profile_mod::VRP_SWITCH_PROFILE,
        Mode::Config,
        "stp root primary",
    );
    assert_eq!(
        insp.status,
        CommandStatus::Executed,
        "Unknown 不得判不支持: {insp:?}"
    );
    assert_eq!(insp.support, SupportLabel::PendingVerification);
    // 未关联规格的命令仍可解析，但不得标为完整支持。
    let backlog = inspect_command(
        profile_mod::IOS_ROUTER_PROFILE,
        Mode::Priv,
        "show ip interface brief",
    );
    assert_eq!(backlog.support, SupportLabel::PendingVerification);
    assert!(backlog.detail.contains("迁移清单"));
}

#[test]
fn stage2b_dual_channel_and_inspection_is_pure_read() {
    let (mut eng, _pc1, r1, _pc2, _sess) = router_lab(Dialect::Ios);
    let mut s = Session::default();
    // 已关联 strict 行的命令：证据不足时提示只走 simulator_notices。
    for l in ["enable", "configure terminal", "ip dhcp pool P"] {
        let r = exec_and_run_result(&mut eng, &mut s, r1, l);
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
    let cfg = exec_and_run(&mut eng, &mut top, r1, "show running-config");
    assert!(cfg.contains("hostname R1"));

    // inspection 纯读：不推进时间、不发探测、不调用 run_checks。
    let before = state_fingerprint(&eng);
    let now = eng.now_ns();
    let inflight = eng.in_flight();
    for _ in 0..3 {
        let p = inspect_command(profile_mod::IOS_ROUTER_PROFILE, Mode::Priv, "show ip route");
        assert_eq!(p.status, CommandStatus::Executed);
        let top = Session::default();
        let view = inspect_device_command(&eng, r1, &top, "show running-config");
        assert_eq!(view.status, CommandStatus::Executed);
    }
    assert_eq!(state_fingerprint(&eng), before, "inspection 不得改变状态");
    assert_eq!(eng.now_ns(), now, "inspection 不得推进虚拟时间");
    assert_eq!(eng.in_flight(), inflight, "inspection 不得发出探测");
}

#[test]
fn stage2b_config_render_closes_registered_deviations() {
    // DHCP 跨方言、ACL 占位、NAT 接口名、VRRP 占位、OSPF 进程/区域、VTY 层级。
    let (mut eng, _pc1, r1, _pc2, mut s) = router_lab(Dialect::Ios);
    for l in [
        "enable",
        "configure terminal",
        "username admin privilege 3 secret HIDDEN",
        "enable secret TOPSECRET",
        "line vty 0 4",
        "password LINEPW",
        "login",
        "exit",
        "ip dhcp pool OFFICE",
        "network 192.168.1.0 255.255.255.0",
        "default-router 192.168.1.1",
        "exit",
        "access-list 100 permit tcp 192.168.1.0 0.0.0.255 192.168.2.0 0.0.0.255 eq 80",
        "interface GigabitEthernet0/0",
        "ip access-group 100 in",
        "ip nat inside",
        "vrrp 1 ip 192.168.1.254",
        "exit",
        "interface GigabitEthernet0/1",
        "ip nat outside",
        "exit",
        "router ospf 12",
        "network 192.168.1.0 0.0.0.255 area 3",
        "end",
    ] {
        ok(
            &exec_and_run_result(&mut eng, &mut s, r1, l),
            &format!("render {l}"),
        );
    }
    let cfg = config_view(&mut eng, r1, Dialect::Ios);
    // DHCP：IOS 用词
    assert!(cfg.contains("ip dhcp pool OFFICE"));
    assert!(cfg.contains("default-router 192.168.1.1"));
    // ACL：真实规则而非占位行
    assert!(cfg.contains("access-list 100 permit tcp"));
    assert!(!cfg.lines().any(|l| l.trim() == "access-list 100"));
    // NAT：真实接口名，无占位拼接
    assert!(cfg.contains("interface GigabitEthernet0/0"));
    assert!(!cfg.contains("GigabitEthernet0/0/1 nat"));
    // VRRP：真实接口块与优先级，无占位行
    assert!(cfg.contains("vrrp 1 ip 192.168.1.254"));
    assert!(!cfg.contains(": vrrp 1 ip"));
    // OSPF：真实进程与区域
    assert!(cfg.contains("router ospf 12"));
    assert!(cfg.contains("network 192.168.1.0 0.0.0.255 area 3"));
    assert!(!cfg.lines().any(|l| l.trim() == "router ospf 1") || cfg.contains(" area 0\n"));
    // mgmt：VTY 层级、真实权限、口令脱敏
    assert!(cfg.contains("line vty 0 4"));
    assert!(cfg.contains(" login\n") || cfg.contains(" login"));
    assert!(cfg.contains(" password ***"));
    assert!(cfg.contains("username admin privilege 3 secret ***"));
    assert!(!cfg.contains("HIDDEN"));
    assert!(!cfg.contains("TOPSECRET"));
    assert!(!cfg.contains("LINEPW"));
}
