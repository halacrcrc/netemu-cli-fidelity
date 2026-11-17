//! 阶段 4A：Comware V7 独立命令树、首批原生闭环、双向隔离、配置渲染重执行、
//! OSPF 两级退出、课程表分层矩阵与实验二 Comware 化的回归测试。
//!
//! 证据见 `docs/cli-fidelity/stage4a-implementation-record.md` §2（E1–E11）。
//! 无带版本真机样本 → 这里只做语法 / 模式 / 结构化状态 / 数据面断言，
//! 不为厂商逐字输出建立黄金。

use cli_dialects::{
    config_render, exec_and_run_result, exec_line_result, inspect_command, prompt,
    scan_simulator_wording, state_fingerprint, trees,
};
use cli_engine::spec::CommandStatus;
use cli_engine::{parse, Mode, Session};
use sim_core::device::{DevId, DeviceKind, DeviceState, Dialect, PortMode};
use sim_core::engine::Engine;
use sim_core::profile::{
    COMWARE_ROUTER_PROFILE, COMWARE_SWITCH_PROFILE, IOS_ROUTER_PROFILE, VRP_ROUTER_PROFILE,
};
use sim_core::Net;
use sim_packets::ipv4_from_str;

fn ip(s: &str) -> u32 {
    ipv4_from_str(s).unwrap()
}

/// 配置命令走完整 CLI → 引擎静默链（与浏览器 exec_command 一致），
/// 让 STP / ARP 等协议状态随配置收敛；这是 L4 数据面断言的前置条件。
fn ok(eng: &mut Engine, sess: &mut Session, dev: DevId, line: &str) -> String {
    let r = exec_and_run_result(eng, sess, dev, line);
    assert_eq!(
        r.status,
        CommandStatus::Executed,
        "Comware 命令 {line:?} 应执行，实际 {:?} / {}",
        r.status,
        r.device_output
    );
    r.device_output
}

fn rejected(eng: &mut Engine, sess: &mut Session, dev: DevId, line: &str) -> String {
    let before = state_fingerprint(eng);
    let r = exec_line_result(eng, sess, dev, line);
    assert_eq!(
        r.status,
        CommandStatus::InvalidCommand,
        "Comware 命令 {line:?} 应被拒绝，实际 {:?}",
        r.status
    );
    assert!(!r.state_changed, "被拒绝命令不得改变状态: {line}");
    assert_eq!(state_fingerprint(eng), before, "状态摘要不得变化: {line}");
    r.device_output
}

fn accepts(dialect: Dialect, mode: Mode, line: &str) -> bool {
    matches!(parse(trees(dialect).root_for(mode), line), Ok(Some(_)))
}

// ---------------------------------------------------------------- 拆树与双向隔离

#[test]
fn stage4a_trees_are_independent_and_bidirectionally_isolated() {
    // 1) 直接证明 trees(Comware) 不再返回 VRP 树对象。
    assert!(
        !std::ptr::eq(trees(Dialect::Comware), trees(Dialect::Vrp)),
        "Comware 与 VRP 必须是两个独立 Trees"
    );
    let cw_ids: Vec<String> = cli_dialects::specs::derived(Dialect::Comware)
        .into_iter()
        .map(|d| d.id)
        .collect();
    let vrp_ids: Vec<String> = cli_dialects::specs::derived(Dialect::Vrp)
        .into_iter()
        .map(|d| d.id)
        .collect();
    for id in [
        "config/vlan",
        "config/undo/vlan/@0number",
        "config/interface/vlan-interface/@0number",
        "config/interface/bridge-aggregation/@0number",
        "if/port/access/vlan/@0number",
        "if/port/trunk/permit/vlan/@0vlan-list",
        "if/packet-filter/@0number/@0word",
    ] {
        assert!(cw_ids.contains(&id.to_string()), "Comware 树缺少 {id}");
    }
    for id in [
        "config/vlan/batch",
        "config/interface/vlanif/@0number",
        "config/interface/eth-trunk/@0number",
        "if/port/default/vlan/@0number",
        "if/port/trunk/allow-pass/vlan/@0vlan-list",
        "if/traffic-filter/@0word/acl/@0number",
    ] {
        assert!(
            !cw_ids.contains(&id.to_string()),
            "VRP 专属 {id} 混入 Comware 树"
        );
    }
    for id in [
        "config/vlan/batch",
        "config/interface/vlanif/@0number",
        "if/port/default/vlan/@0number",
        "if/port/trunk/allow-pass/vlan/@0vlan-list",
    ] {
        assert!(vrp_ids.contains(&id.to_string()), "VRP 树缺少原生 {id}");
    }
    for id in [
        "config/interface/vlan-interface/@0number",
        "config/interface/bridge-aggregation/@0number",
        "if/port/access/vlan/@0number",
        "if/port/trunk/permit/vlan/@0vlan-list",
    ] {
        assert!(
            !vrp_ids.contains(&id.to_string()),
            "Comware 专属 {id} 混入 VRP 树"
        );
    }

    // 2) 逐方言接受 / 拒绝矩阵（参数、模式和字面量都覆盖）。
    let vrp_only: &[(Mode, &str)] = &[
        (Mode::Config, "vlan batch 10 20"),
        (Mode::Config, "interface vlanif 10"),
        (Mode::Config, "interface eth-trunk 1"),
        (Mode::Config, "user-interface vty 0 4"),
        (Mode::Config, "firewall zone trust"),
        (Mode::If, "port default vlan 10"),
        (Mode::If, "port trunk allow-pass vlan 10 20"),
        (Mode::If, "eth-trunk 1"),
        (Mode::If, "traffic-filter inbound acl 3000"),
        (Mode::Zone, "add interface g0/0/0"),
        (Mode::Exec, "display eth-trunk 1"),
    ];
    let comware_only: &[(Mode, &str)] = &[
        (Mode::Config, "undo vlan 10"),
        (Mode::Config, "interface vlan-interface 10"),
        (Mode::Config, "interface bridge-aggregation 1"),
        (Mode::Config, "line vty 0 4"),
        (Mode::Config, "security-zone name trust"),
        (Mode::If, "port access vlan 10"),
        (Mode::If, "port trunk permit vlan 10 20"),
        (Mode::If, "undo port access vlan"),
        (Mode::If, "undo port trunk permit vlan 10"),
        (Mode::If, "packet-filter 3000 inbound"),
        (Mode::Zone, "import interface g1/0/1"),
        (Mode::Exec, "display link-aggregation summary"),
    ];
    for (mode, line) in vrp_only {
        assert!(accepts(Dialect::Vrp, *mode, line), "VRP 应接受 {line:?}");
        assert!(
            !accepts(Dialect::Comware, *mode, line),
            "Comware 应拒绝 VRP 写法 {line:?}"
        );
    }
    for (mode, line) in comware_only {
        assert!(
            accepts(Dialect::Comware, *mode, line),
            "Comware 应接受 {line:?}"
        );
        assert!(
            !accepts(Dialect::Vrp, *mode, line),
            "VRP 应拒绝 Comware 写法 {line:?}"
        );
    }
    // 3) 有官方证据的通用命令在两棵树都保留。
    for (mode, line) in [
        (Mode::Exec, "system-view"),
        (Mode::Config, "vlan 10"),
        (Mode::Config, "ip route-static 192.168.2.0 24 10.0.0.2"),
        (Mode::Config, "ospf 1"),
        (Mode::If, "port link-type trunk"),
        (Mode::If, "undo shutdown"),
        (Mode::Exec, "display current-configuration"),
        (Mode::Exec, "display vlan"),
    ] {
        assert!(accepts(Dialect::Vrp, mode, line), "VRP 通用命令 {line:?}");
        assert!(
            accepts(Dialect::Comware, mode, line),
            "Comware 通用命令 {line:?}"
        );
    }
}

#[test]
fn stage4a_lint_and_profile_are_comware_v7_baseline() {
    let report = cli_dialects::specs::lint_report(Dialect::Comware);
    assert!(report.strict_ok(), "Comware 规格 lint 失败: {report:?}");
    assert!(report.duplicates.is_empty(), "{report:?}");
    assert!(report.dangling.is_empty(), "{report:?}");
    assert!(report.drift.is_empty(), "{report:?}");
    assert!(
        report.strict_total > 10,
        "首批闭环命令必须进入 strict 门禁: {report:?}"
    );
    let rows = cli_dialects::specs::rows(Dialect::Comware);
    assert!(rows.len() > 20, "Comware 规格行数不足: {}", rows.len());
    assert!(rows
        .iter()
        .any(|r| r.semantics == cli_engine::spec::SemanticSupport::NotSimulated));
    assert!(rows
        .iter()
        .any(|r| r.effect == cli_engine::spec::Effect::Query));
    assert!(
        rows.iter().all(|r| !r.references.is_empty()),
        "每条规格必须有官方证据"
    );

    let profile = sim_core::profile::profile_for(Dialect::Comware, DeviceKind::Switch);
    assert!(profile.os_baseline.contains("Comware V7"));
    assert_eq!(
        profile.evidence,
        sim_core::profile::EvidenceLevel::DocumentationOnly
    );
    let p = profile.policy;
    assert_eq!(p.trunk_initial, sim_core::profile::TrunkInitial::Vlan1Only);
    assert_eq!(p.trunk_initial_list(), vec![1]);
    assert!(p.trunk_allow_additive, "E1: permit 多次执行为累加");
    assert!(p.trunk_allow_keeps_vlan1, "E3: 显式 permit 后仍保留 VLAN1");
    assert!(
        p.trunk_allow_requires_trunk_mode,
        "E1/E2: 先 link-type trunk"
    );
    // VRP 策略独立存在且不是 Comware 的隐式来源。
    let vrp = sim_core::profile::platform_policy(Dialect::Vrp);
    assert_eq!(vrp.trunk_initial_list(), vec![1]);
    assert!(!sim_core::profile::platform_policy(Dialect::Ios).trunk_allow_requires_trunk_mode);
}

// ---------------------------------------------------------------- 首批原生闭环

#[test]
fn stage4a_vlan_access_svi_cross_vlan_routing_and_config_render_reexecution() {
    let (mut eng, pc1, sw, pc2) = office_l3_lab();
    let mut s = Session::default();
    for l in [
        "system-view",
        // E1/E11：`vlan <id>` 进入 VLAN 视图，必须 quit 回系统视图再继续。
        "vlan 10",
        "quit",
        "vlan 20",
        "quit",
        "interface g1/0/1",
        "port link-type access",
        "port access vlan 10",
        "quit",
        "interface g1/0/2",
        "port link-type access",
        "port access vlan 20",
        "quit",
        "interface Vlan-interface 10",
        "ip address 192.168.10.1 24",
        "quit",
        "interface Vlan-interface 20",
        "ip address 192.168.20.1 24",
        "quit",
    ] {
        ok(&mut eng, &mut s, sw, l);
    }
    assert_eq!(prompt(&eng, &s, sw), "[SW1]");
    let dev = eng.net.device(sw).unwrap();
    assert_eq!(dev.ports[0].pvid, 10);
    assert_eq!(dev.ports[0].mode, PortMode::Access);
    assert_eq!(dev.ports[1].pvid, 20);
    let DeviceState::Switch(st) = &dev.state else {
        panic!()
    };
    assert!(st.vlans.contains(&10) && st.vlans.contains(&20));
    assert_eq!(ipv4_str(st.svi[&10].ip), "192.168.10.1");
    assert_eq!(ipv4_str(st.svi[&20].ip), "192.168.20.1");
    assert!(st.svi[&10].admin_up && st.svi[&20].admin_up);

    // 非法 VLAN ID：全部拒绝且状态不变（E1 范围 1-4094）。
    for bad in ["vlan 0", "vlan 4095", "vlan 99999", "port access vlan 0"] {
        let before = state_fingerprint(&eng);
        let r = exec_line_result(&mut eng, &mut s, sw, bad);
        assert_eq!(r.status, CommandStatus::InvalidCommand, "{bad}");
        assert_eq!(state_fingerprint(&eng), before, "{bad} 不得写状态");
    }
    // 错误模式：接口视图命令在系统视图必须被拒绝。
    rejected(&mut eng, &mut s, sw, "port access vlan 10");

    // VLAN 创建 / 删除 / 不可删除 VLAN1 / 不存在的 VLAN。
    ok(&mut eng, &mut s, sw, "vlan 30");
    assert!(
        matches!(&eng.net.device(sw).unwrap().state, DeviceState::Switch(x) if x.vlans.contains(&30))
    );
    ok(&mut eng, &mut s, sw, "quit");
    ok(&mut eng, &mut s, sw, "undo vlan 30");
    assert!(
        matches!(&eng.net.device(sw).unwrap().state, DeviceState::Switch(x) if !x.vlans.contains(&30))
    );
    rejected(&mut eng, &mut s, sw, "undo vlan 1");
    rejected(&mut eng, &mut s, sw, "undo vlan 99");

    // Access PVID / 撤销 / 恢复。
    ok(&mut eng, &mut s, sw, "interface g1/0/1");
    ok(&mut eng, &mut s, sw, "undo port access vlan");
    assert_eq!(eng.net.device(sw).unwrap().ports[0].pvid, 1);
    ok(&mut eng, &mut s, sw, "port access vlan 10");
    assert_eq!(eng.net.device(sw).unwrap().ports[0].pvid, 10);
    ok(&mut eng, &mut s, sw, "quit");

    // SVI 管理状态：shutdown 后跨 VLAN 不通，undo shutdown 后恢复。
    let ping_ok = |eng: &mut Engine| {
        exec_and_run_result(eng, &mut Session::default(), pc1, "ping 192.168.20.12").device_output
    };
    let out = ping_ok(&mut eng);
    assert!(
        out.contains("5 packets transmitted, 5 received"),
        "跨 VLAN 应通: {out}"
    );
    ok(&mut eng, &mut s, sw, "interface Vlan-interface 10");
    ok(&mut eng, &mut s, sw, "shutdown");
    ok(&mut eng, &mut s, sw, "quit");
    let out = ping_ok(&mut eng);
    assert!(
        out.contains("5 packets transmitted, 0 received"),
        "SVI down 应不通: {out}"
    );
    ok(&mut eng, &mut s, sw, "interface Vlan-interface 10");
    ok(&mut eng, &mut s, sw, "undo shutdown");
    ok(&mut eng, &mut s, sw, "quit");
    let out = ping_ok(&mut eng);
    assert!(
        out.contains("5 packets transmitted, 5 received"),
        "恢复后应通: {out}"
    );

    // 静态路由写入 / 查询（供配置渲染重执行验证）。
    ok(
        &mut eng,
        &mut s,
        sw,
        "ip route-static 192.168.30.0 24 192.168.10.9",
    );
    let route_view = ok(&mut eng, &mut s, sw, "display ip routing-table");
    assert!(route_view.contains("192.168.30.0/24"), "{route_view}");
    assert!(route_view.contains("Static"), "{route_view}");

    // display current-configuration 只输出 Comware 原生写法。
    let cfg = ok(&mut eng, &mut s, sw, "display current-configuration");
    for forbidden in [
        "vlan batch",
        "port default vlan",
        "allow-pass",
        "Vlanif",
        "Eth-Trunk",
        "traffic-filter",
    ] {
        assert!(
            !cfg.contains(forbidden),
            "Comware 配置不得输出 {forbidden}: {cfg}"
        );
    }
    for required in [
        "vlan 10",
        "vlan 20",
        "port access vlan 10",
        "port access vlan 20",
        "interface Vlan-interface10",
        "interface Vlan-interface20",
        "ip address 192.168.10.1 24",
        "ip route-static 192.168.30.0 24 192.168.10.9",
    ] {
        assert!(cfg.contains(required), "配置渲染缺少 {required}: {cfg}");
    }
    assert!(
        scan_simulator_wording(&cfg).is_empty(),
        "设备输出不得混入模拟器措辞"
    );

    // 渲染出的首批命令必须能逐行重新执行，并还原同一状态摘要。
    let (mut fresh, _, fresh_sw, _) = office_l3_lab();
    let mut fs = Session {
        mode: Mode::Config,
        ..Default::default()
    };
    for line in cfg.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        ok(&mut fresh, &mut fs, fresh_sw, line);
    }
    assert_eq!(fs.mode, Mode::Config, "重执行后应回到系统视图");
    let original = config_fingerprint(&eng, sw);
    let restored = config_fingerprint(&fresh, fresh_sw);
    assert_eq!(
        restored, original,
        "重执行后配置状态必须一致\nORIGINAL={original}\nRESTORED={restored}"
    );
    // 显式验证 config_render 的三分支（不依赖 CLI 会话）。
    let summary = eng.net.summary();
    let dev_summary = &summary.devices[1];
    let vrp_text = config_render::render(dev_summary, Dialect::Vrp);
    assert!(vrp_text.contains("vlan batch"), "{vrp_text}");
    assert!(vrp_text.contains("port default vlan"), "{vrp_text}");
    assert!(vrp_text.contains("interface Vlanif"), "{vrp_text}");
    // 本拓扑无 Trunk 口，allow-pass 不会出现；关键是 VRP 分支不含 Comware 字面量。
    assert!(!vrp_text.contains("port access vlan"), "{vrp_text}");
    assert!(!vrp_text.contains("permit vlan"), "{vrp_text}");
    let ios_text = config_render::render(dev_summary, Dialect::Ios);
    assert!(ios_text.contains("switchport access vlan"), "{ios_text}");
    assert_eq!(cfg, config_render::render(dev_summary, Dialect::Comware));
    let _ = pc2;
}

fn ipv4_str(v: u32) -> String {
    sim_packets::ipv4_str(v)
}

/// 确定性配置指纹：只看 sim-core 持有的配置态，并对 HashMap 派生列表排序，
/// 用于比较「原生配置渲染 → 逐行重执行」前后的状态是否一致。
fn config_fingerprint(eng: &Engine, dev: DevId) -> String {
    let d = eng.net.device(dev).unwrap();
    let DeviceState::Switch(st) = &d.state else {
        panic!("需要三层交换机状态");
    };
    let mut vlans = st.vlans.clone();
    vlans.sort_unstable();
    let mut svis: Vec<(u16, u32, u8, bool)> = st
        .svi
        .iter()
        .map(|(v, s)| (*v, s.ip, s.prefix, s.admin_up))
        .collect();
    svis.sort_unstable();
    let mut routes: Vec<(u32, u8, u32)> = st
        .static_routes
        .iter()
        .map(|r| (r.dst, r.len, r.nexthop))
        .collect();
    routes.sort_unstable();
    let ports: Vec<String> = d
        .ports
        .iter()
        .map(|p| {
            format!(
                "{}|{}|{:?}|{}|{:?}|{:?}|{}",
                p.idx, p.admin_up, p.mode, p.pvid, p.trunk_allowed, p.ip, p.prefix
            )
        })
        .collect();
    format!("{}|{vlans:?}|{svis:?}|{routes:?}|{ports:?}", d.name)
}

/// 实验二拓扑：PC1 —— SW1(Comware 三层交换机) —— PC2。
fn office_l3_lab() -> (Engine, DevId, DevId, DevId) {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let sw = net.add_device("SW1".into(), DeviceKind::Switch, Dialect::Comware);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc1, sw).unwrap();
    net.add_link(sw, pc2).unwrap();
    net.set_host(pc1, Some(ip("192.168.10.11")), 24, Some(ip("192.168.10.1")));
    net.set_host(pc2, Some(ip("192.168.20.12")), 24, Some(ip("192.168.20.1")));
    (Engine::new(net), pc1, sw, pc2)
}

// ---------------------------------------------------------------- Trunk 数据面

/// 两台 Comware 交换机经 Trunk 互联，主机同属 VLAN 10。
fn trunk_lab() -> (Engine, DevId, DevId, DevId, DevId) {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let sw1 = net.add_device("SW1".into(), DeviceKind::Switch, Dialect::Comware);
    let sw2 = net.add_device("SW2".into(), DeviceKind::Switch, Dialect::Comware);
    let pc2 = net.add_device("PC2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc1, sw1).unwrap(); // sw1 port0
    net.add_link(sw1, sw2).unwrap(); // sw1 port1 / sw2 port0
    net.add_link(sw2, pc2).unwrap(); // sw2 port1
    net.set_host(pc1, Some(ip("192.168.10.11")), 24, Some(ip("192.168.10.1")));
    net.set_host(pc2, Some(ip("192.168.10.12")), 24, Some(ip("192.168.10.1")));
    (Engine::new(net), pc1, sw1, sw2, pc2)
}

fn trunk_ping(eng: &mut Engine, pc1: DevId) -> String {
    exec_and_run_result(eng, &mut Session::default(), pc1, "ping 192.168.10.12").device_output
}

#[test]
fn stage4a_trunk_default_permit_semantics_and_positive_negative_traffic() {
    let (mut eng, pc1, sw1, sw2, _pc2) = trunk_lab();
    let mut s1 = Session::default();
    let mut s2 = Session::default();
    for (sw, sess, access_port) in [(sw1, &mut s1, "g1/0/1"), (sw2, &mut s2, "g1/0/2")] {
        ok(&mut eng, sess, sw, "system-view");
        ok(&mut eng, sess, sw, "vlan 10");
        ok(&mut eng, sess, sw, "quit");
        ok(&mut eng, sess, sw, &format!("interface {access_port}"));
        ok(&mut eng, sess, sw, "port link-type access");
        ok(&mut eng, sess, sw, "port access vlan 10");
        ok(&mut eng, sess, sw, "quit");
    }
    let trunk_port = |sw: DevId| if sw == sw1 { "g1/0/2" } else { "g1/0/1" };
    for (sw, sess) in [(sw1, &mut s1), (sw2, &mut s2)] {
        ok(&mut eng, sess, sw, &format!("interface {}", trunk_port(sw)));
        ok(&mut eng, sess, sw, "port link-type trunk");
    }
    // E1/E2: 未配置 permit 时 Trunk 默认只允许 VLAN1。
    for sw in [sw1, sw2] {
        let p = &eng.net.device(sw).unwrap().ports[if sw == sw1 { 1 } else { 0 }];
        assert_eq!(p.mode, PortMode::Trunk);
        assert_eq!(p.trunk_allowed, Some(vec![1]));
    }
    // 负向流量：VLAN10 未被 permit → 全超时。
    let out = trunk_ping(&mut eng, pc1);
    assert!(
        out.contains("5 packets transmitted, 0 received"),
        "默认列表应阻断 VLAN10: {out}"
    );
    // 未在 Trunk 模式的端口执行 permit 必须拒绝且模式不变。
    ok(&mut eng, &mut s1, sw1, "quit");
    ok(&mut eng, &mut s1, sw1, "interface g1/0/1");
    rejected(&mut eng, &mut s1, sw1, "port trunk permit vlan 10");
    assert_eq!(eng.net.device(sw1).unwrap().ports[0].mode, PortMode::Access);
    ok(&mut eng, &mut s1, sw1, "quit");

    // permit 累加与保留 VLAN1。
    ok(&mut eng, &mut s1, sw1, "interface g1/0/2");
    ok(&mut eng, &mut s1, sw1, "port trunk permit vlan 10");
    assert_eq!(
        eng.net.device(sw1).unwrap().ports[1].trunk_allowed,
        Some(vec![1, 10])
    );
    ok(&mut eng, &mut s1, sw1, "port trunk permit vlan 20");
    assert_eq!(
        eng.net.device(sw1).unwrap().ports[1].trunk_allowed,
        Some(vec![1, 10, 20]),
        "E1: 多次 permit 应累加"
    );
    ok(&mut eng, &mut s1, sw1, "undo port trunk permit vlan 20");
    assert_eq!(
        eng.net.device(sw1).unwrap().ports[1].trunk_allowed,
        Some(vec![1, 10])
    );
    ok(&mut eng, &mut s1, sw1, "quit");
    ok(&mut eng, &mut s2, sw2, "port trunk permit vlan 10");
    ok(&mut eng, &mut s2, sw2, "quit");
    // 正向流量：两端 permit VLAN10 后同 VLAN 全通。
    let out = trunk_ping(&mut eng, pc1);
    assert!(
        out.contains("5 packets transmitted, 5 received"),
        "permit 后应全通: {out}"
    );
    // 撤销一端 permit → 再次阻断；`all` 恢复默认列表。
    ok(&mut eng, &mut s2, sw2, "interface g1/0/1");
    ok(&mut eng, &mut s2, sw2, "undo port trunk permit vlan 10");
    let out = trunk_ping(&mut eng, pc1);
    assert!(
        out.contains("5 packets transmitted, 0 received"),
        "撤销后应阻断: {out}"
    );
    ok(&mut eng, &mut s2, sw2, "port trunk permit vlan 10 20");
    ok(&mut eng, &mut s2, sw2, "undo port trunk permit vlan all");
    assert_eq!(
        eng.net.device(sw2).unwrap().ports[0].trunk_allowed,
        Some(vec![1]),
        "E1/E10: undo ... all 恢复默认仅 VLAN1"
    );
    // 配置视图反映 permit 列表，且仍不含 VRP 的 allow-pass。
    let cfg = ok(&mut eng, &mut s2, sw2, "display current-configuration");
    assert!(cfg.contains("port trunk permit vlan"), "{cfg}");
    assert!(!cfg.contains("allow-pass"), "{cfg}");
    assert!(!cfg.contains("vlan batch"), "{cfg}");
}

// ---------------------------------------------------------------- 静态路由

/// 两台 Comware 路由器 + 两台主机，端口默认管理状态由拓扑显式置 up。
fn router_lab() -> (Engine, DevId, DevId, DevId, DevId) {
    let mut net = Net::new();
    let pc1 = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Comware);
    let r2 = net.add_device("R2".into(), DeviceKind::Router, Dialect::Comware);
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
        for port in [0usize, 1] {
            net.set_port_admin(dev, port, true);
        }
        net.set_port_ip(dev, 0, Some(ip(p0)), 24);
        net.set_port_ip(dev, 1, Some(ip(p1)), 24);
    }
    (Engine::new(net), pc1, r1, r2, pc2)
}

fn route_ping(eng: &mut Engine, pc1: DevId) -> String {
    exec_and_run_result(eng, &mut Session::default(), pc1, "ping 192.168.2.10").device_output
}

#[test]
fn stage4a_static_route_write_query_undo_and_positive_negative_ping() {
    let (mut eng, pc1, r1, r2, _pc2) = router_lab();
    let mut s1 = Session::default();
    let mut s2 = Session::default();
    ok(&mut eng, &mut s1, r1, "system-view");
    ok(&mut eng, &mut s2, r2, "system-view");
    // 负向：无静态路由 → 全超时。
    let out = route_ping(&mut eng, pc1);
    assert!(
        out.contains("5 packets transmitted, 0 received"),
        "无路由应不通: {out}"
    );
    // 写入 + 查询。
    ok(
        &mut eng,
        &mut s1,
        r1,
        "ip route-static 192.168.2.0 24 10.0.0.2",
    );
    ok(
        &mut eng,
        &mut s2,
        r2,
        "ip route-static 192.168.1.0 24 10.0.0.1",
    );
    let table = ok(&mut eng, &mut s1, r1, "display ip routing-table");
    assert!(table.contains("192.168.2.0/24"), "{table}");
    assert!(
        table.contains("Static") && table.contains("10.0.0.2"),
        "{table}"
    );
    assert!(matches!(&eng.net.device(r1).unwrap().state,
        DeviceState::Router(x) if x.static_routes.len() == 1));
    let out = route_ping(&mut eng, pc1);
    assert!(
        out.contains("5 packets transmitted, 5 received"),
        "双向静态路由应全通: {out}"
    );
    // 撤销 + 重新查询 + 负向 Ping。
    ok(
        &mut eng,
        &mut s1,
        r1,
        "undo ip route-static 192.168.2.0 24 10.0.0.2",
    );
    let table = ok(&mut eng, &mut s1, r1, "display ip routing-table");
    assert!(
        !table.contains("192.168.2.0/24"),
        "撤销后路由表不应再有该表项: {table}"
    );
    assert!(matches!(&eng.net.device(r1).unwrap().state,
        DeviceState::Router(x) if x.static_routes.is_empty()));
    let out = route_ping(&mut eng, pc1);
    assert!(
        out.contains("5 packets transmitted, 0 received"),
        "撤销后应不通: {out}"
    );
    // 恢复后再次正向，证明状态变化被数据面感知。
    ok(
        &mut eng,
        &mut s1,
        r1,
        "ip route-static 192.168.2.0 24 10.0.0.2",
    );
    let out = route_ping(&mut eng, pc1);
    assert!(
        out.contains("5 packets transmitted, 5 received"),
        "恢复后应通: {out}"
    );
    // 配置视图反映路由，且 Comware 不输出 IOS 写法。
    let cfg = ok(&mut eng, &mut s1, r1, "display current-configuration");
    assert!(
        cfg.contains("ip route-static 192.168.2.0 24 10.0.0.2"),
        "{cfg}"
    );
    assert!(!cfg.contains("ip route "), "{cfg}");
}

// ---------------------------------------------------------------- OSPF 两级退出

#[test]
fn stage4a_ospf_area_view_two_quits_prompt_config_and_error_mode() {
    let mut net = Net::new();
    let r = net.add_device("R1".into(), DeviceKind::Router, Dialect::Comware);
    let mut eng = Engine::new(net);
    let mut s = Session::default();
    ok(&mut eng, &mut s, r, "system-view");
    ok(&mut eng, &mut s, r, "ospf 7");
    assert_eq!(s.mode, Mode::Ospf);
    assert_eq!(prompt(&eng, &s, r), "[R1-ospf-7]");
    ok(&mut eng, &mut s, r, "area 0.0.0.1");
    assert_eq!(s.mode, Mode::OspfArea);
    assert_eq!(prompt(&eng, &s, r), "[R1-ospf-7-area-0.0.0.1]");
    ok(&mut eng, &mut s, r, "network 10.1.0.0 0.0.0.3");
    assert!(matches!(&eng.net.device(r).unwrap().state,
        DeviceState::Router(x) if x.ospf_process == Some(7)
            && x.ospf_networks.len() == 1));
    // 错误模式命令被拒绝且状态不变。
    rejected(&mut eng, &mut s, r, "vlan 99");
    rejected(&mut eng, &mut s, r, "port access vlan 10");
    assert_eq!(prompt(&eng, &s, r), "[R1-ospf-7-area-0.0.0.1]");
    // 第一次 quit：回到同一 OSPF 进程视图，进程号不丢失。
    ok(&mut eng, &mut s, r, "quit");
    assert_eq!(s.mode, Mode::Ospf);
    assert_eq!(prompt(&eng, &s, r), "[R1-ospf-7]");
    assert_eq!(s.sub.ospf.map(|(pid, _)| pid), Some(7));
    // 第二次 quit：回到系统视图。
    ok(&mut eng, &mut s, r, "quit");
    assert_eq!(s.mode, Mode::Config);
    assert_eq!(prompt(&eng, &s, r), "[R1]");
    // 配置视图与真实状态一致，且两次 quit 保证可重执行。
    let cfg = ok(&mut eng, &mut s, r, "display current-configuration");
    assert!(cfg.contains("ospf 7"), "{cfg}");
    assert!(cfg.contains("area 0.0.0.1"), "{cfg}");
    assert!(cfg.contains("network 10.1.0.0 0.0.0.3"), "{cfg}");
    // OSPF 块自身必须包含两次 quit（区域 → 进程 → 系统），且每条命令在对应模式可解析。
    let ospf_block = &cfg[cfg.find("ospf 7").unwrap()..];
    assert_eq!(
        ospf_block.matches(" quit").count(),
        2,
        "OSPF 块应两次 quit: {cfg}"
    );
    assert!(accepts(Dialect::Comware, Mode::Config, "ospf 7"));
    assert!(accepts(Dialect::Comware, Mode::Ospf, "area 0.0.0.1"));
    assert!(accepts(
        Dialect::Comware,
        Mode::OspfArea,
        "network 10.1.0.0 0.0.0.3"
    ));
}

// ---------------------------------------------------------------- Profile / 四态 / 双通道

#[test]
fn stage4a_profile_inspection_is_layered_pure_and_dual_channel() {
    // 首批闭环：完整支持。
    for (profile, mode, line) in [
        (COMWARE_SWITCH_PROFILE, Mode::Config, "vlan 10"),
        (COMWARE_SWITCH_PROFILE, Mode::If, "port access vlan 10"),
        (
            COMWARE_ROUTER_PROFILE,
            Mode::Config,
            "ip route-static 192.168.2.0 24 10.0.0.2",
        ),
        (
            COMWARE_SWITCH_PROFILE,
            Mode::Exec,
            "display current-configuration",
        ),
    ] {
        let i = inspect_command(profile, mode, line);
        assert_eq!(i.support.label(), "full-support", "{line} / {}", i.detail);
        assert_eq!(i.status.label(), "Executed");
        assert!(i.passed);
        assert!(!i.spec_id.is_empty());
    }
    // 真实但未模拟：仅识别，不得完整支持。
    let pf = inspect_command(
        COMWARE_ROUTER_PROFILE,
        Mode::If,
        "packet-filter 3000 inbound",
    );
    assert_eq!(pf.support.label(), "recognized-only");
    assert_eq!(pf.status.label(), "NotSimulated");
    assert!(!pf.passed);
    // 未迁移 / 证据不足：待验证，不得完整支持。
    let pending = inspect_command(
        COMWARE_ROUTER_PROFILE,
        Mode::Config,
        "security-zone name trust",
    );
    assert_eq!(pending.support.label(), "pending-verification");
    assert_ne!(pending.support.label(), "full-support");
    // 命令错误：VRP 专属写法在 Comware 上被独立树拒绝。
    let bad = inspect_command(COMWARE_ROUTER_PROFILE, Mode::Config, "vlan batch 10 20");
    assert_eq!(bad.support.label(), "invalid-command");
    assert_eq!(bad.status.label(), "InvalidCommand");
    // 会话级纯读：调用前后状态摘要不变。
    let mut net = Net::new();
    let sw = net.add_device("SW1".into(), DeviceKind::Switch, Dialect::Comware);
    let mut eng = Engine::new(net);
    let mut s = Session {
        mode: Mode::Config,
        ..Default::default()
    };
    let before = state_fingerprint(&eng);
    let i = cli_dialects::inspect_device_command(&eng, sw, &s, "port access vlan 10");
    assert_eq!(
        i.support.label(),
        "invalid-command",
        "If 命令在 Config 模式应无效"
    );
    assert_eq!(state_fingerprint(&eng), before);
    // 双通道：Executed 路径设备输出无模拟器措辞；NotSimulated 路径只有提示。
    ok(&mut eng, &mut s, sw, "vlan 10");
    ok(&mut eng, &mut s, sw, "quit");
    let r = exec_line_result(&mut eng, &mut s, sw, "display vlan");
    assert_eq!(r.status, CommandStatus::Executed);
    assert!(scan_simulator_wording(&r.device_output).is_empty());
    ok(&mut eng, &mut s, sw, "interface g1/0/1");
    let r = exec_line_result(&mut eng, &mut s, sw, "packet-filter 3000 inbound");
    assert_eq!(r.status, CommandStatus::NotSimulated);
    assert!(r.device_output.is_empty());
    assert!(!r.simulator_notices.is_empty());
    assert!(!r.state_changed);
    // 迁移期兼容的语法检查与拆树后行为一致（WASM 旧 verify_command 走同一 parse）。
    assert!(accepts(Dialect::Comware, Mode::Config, "vlan 10"));
    assert!(!accepts(Dialect::Comware, Mode::Config, "vlan batch 10 20"));
    // 既有 IOS / VRP 行为不回归。
    assert_eq!(
        inspect_command(IOS_ROUTER_PROFILE, Mode::If, "switchport access vlan 10")
            .support
            .label(),
        "unsupported-by-device"
    );
    let vrp = inspect_command(VRP_ROUTER_PROFILE, Mode::Config, "vlan batch 10 20");
    assert_eq!(vrp.status.label(), "Executed");
    assert_eq!(vrp.support.label(), "pending-verification");
    assert!(!accepts(Dialect::Vrp, Mode::If, "port access vlan 10"));
}

// ---------------------------------------------------------------- 课程表与实验二

const LABS_CN: &str = include_str!("../../../web/src/lib/labs.ts");
const LABS_EN: &str = include_str!("../../../web/src/content/labs-en.ts");
const CMDMAP: &str = include_str!("../../../web/src/content/cmdmap.ts");

fn block<'a>(src: &'a str, start: &str, end: &str) -> &'a str {
    let a = src.find(start).unwrap_or_else(|| panic!("缺少 {start}"));
    let rest = &src[a..];
    let b = rest.find(end).unwrap_or_else(|| panic!("缺少 {end}"));
    &rest[..b]
}

// ---- 课程原文解析（与浏览器验收脚本使用同一规则） ----

/// 去掉中文/英文括号附注，保留命令正文。
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

/// 把一条任务文本解析为严格 CLI 序列：取标签冒号之后，按 `→` / `->` 切分。
/// 阶段 5A：标签冒号只可能在**第一个箭头之前**，且必须后跟空格（全角无此限制）。
/// 否则 tmsh 的 `destination 10.0.0.100:80` 会被误当成「标签: 正文」。
fn task_cli(task: &str) -> Vec<String> {
    let clean = strip_task_notes(task);
    let arrow = clean
        .find("->")
        .or_else(|| clean.find('→'))
        .unwrap_or(clean.len());
    let head = &clean[..arrow];
    let body = if let Some(p) = head.find('：') {
        clean[p + '：'.len_utf8()..].to_string()
    } else if let Some(p) = head.find(": ") {
        clean[p + 2..].to_string()
    } else {
        clean.clone()
    };
    body.replace('→', "->")
        .split("->")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// 抽出 `tasks: [` 数组中的字符串字面量。
fn lab2_task_lines(src: &str, marker: &str) -> Vec<String> {
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

/// 直接从课程原文（中文 `labs.ts` + 英文 `labs-en.ts`）解析实验二前三步的
/// 完整 CLI 序列；中英文必须给出同一条序列，否则课程不可顺序执行。
fn office_l3_course_cli() -> Vec<String> {
    let cn = lab2_task_lines(LABS_CN, "export const lab2: LabDef = {");
    let en = lab2_task_lines(LABS_EN, "'office-l3': {");
    assert_eq!(cn.len(), 5, "中文实验二应有 5 步任务: {cn:?}");
    assert_eq!(en.len(), 5, "英文实验二应有 5 步任务: {en:?}");
    let cn_cli: Vec<String> = cn[..3].iter().flat_map(|t| task_cli(t)).collect();
    let en_cli: Vec<String> = en[..3].iter().flat_map(|t| task_cli(t)).collect();
    assert_eq!(
        cn_cli, en_cli,
        "中英文课程原文必须给出同一条可顺序执行的 CLI 序列"
    );
    cn_cli
}

#[test]
fn stage4a_office_l3_course_original_sequence_runs_top_to_bottom() {
    // 序列直接从课程原文解析，不是另外手写的一份「验收命令」。
    let cli = office_l3_course_cli();
    assert_eq!(cli.first().map(String::as_str), Some("system-view"));
    assert_eq!(cli.last().map(String::as_str), Some("quit"));
    // 三处此前缺失的层级退出：VLAN 视图、物理接口之间、SVI 之间。
    assert!(cli
        .windows(3)
        .any(|w| w[0] == "vlan 10" && w[1] == "quit" && w[2] == "vlan 20"));
    assert!(cli.windows(3).any(|w| {
        w[0] == "port access vlan 10" && w[1] == "quit" && w[2] == "interface g1/0/2"
    }));
    assert!(cli.windows(3).any(|w| {
        w[0] == "ip address 192.168.10.1 24"
            && w[1] == "quit"
            && w[2] == "interface Vlan-interface 20"
    }));

    let (mut eng, pc1, sw, pc2) = office_l3_lab();
    let mut s = Session::default();
    let mut prompts = Vec::new();
    for line in &cli {
        ok(&mut eng, &mut s, sw, line);
        prompts.push(prompt(&eng, &s, sw));
    }
    assert_eq!(s.mode, Mode::Config, "课程序列结束应回到系统视图");
    assert_eq!(prompts.last().map(String::as_str), Some("[SW1]"));
    assert!(
        prompts.iter().any(|p| p == "[SW1-vlan10]"),
        "必须出现官方 VLAN 视图提示符: {prompts:?}"
    );
    assert!(
        prompts.iter().any(|p| p == "[SW1-vlan20]"),
        "必须出现官方 VLAN 视图提示符: {prompts:?}"
    );

    let dev = eng.net.device(sw).unwrap();
    assert_eq!(dev.ports[0].pvid, 10);
    assert_eq!(dev.ports[1].pvid, 20);
    let DeviceState::Switch(st) = &dev.state else {
        panic!()
    };
    assert!(st.vlans.contains(&10) && st.vlans.contains(&20));
    assert_eq!(ipv4_str(st.svi[&10].ip), "192.168.10.1");
    assert_eq!(ipv4_str(st.svi[&20].ip), "192.168.20.1");

    // 任务 5：display 核对 + PC1 跨 VLAN ping。
    let cfg = ok(&mut eng, &mut s, sw, "display current-configuration");
    assert!(cfg.contains("vlan 10") && cfg.contains("port access vlan 10"));
    assert!(!cfg.contains("vlan batch"));
    let out = exec_and_run_result(&mut eng, &mut Session::default(), pc1, "ping 192.168.20.12")
        .device_output;
    assert!(
        out.contains("5 packets transmitted, 5 received"),
        "课程序列执行完应跨 VLAN 连通: {out}"
    );
    let _ = pc2;
}

#[test]
fn stage4a_comware_vlan_view_quit_and_return_hierarchy() {
    let (mut eng, _pc1, sw, _pc2) = office_l3_lab();
    let mut s = Session::default();
    ok(&mut eng, &mut s, sw, "system-view");
    assert_eq!(prompt(&eng, &s, sw), "[SW1]");

    // E1/E11：vlan <id> 创建 VLAN 并进入 VLAN 视图。
    ok(&mut eng, &mut s, sw, "vlan 10");
    assert_eq!(s.mode, Mode::Vlan);
    assert_eq!(s.sub.vlan, Some(10));
    assert_eq!(prompt(&eng, &s, sw), "[SW1-vlan10]");
    // E11：display 为 Any view，VLAN 视图内可用。
    let v = ok(&mut eng, &mut s, sw, "display vlan");
    assert!(
        v.contains("VLAN0010"),
        "VLAN 视图内 display vlan 应可见: {v}"
    );
    // 错误模式：接口视图命令在 VLAN 视图被拒且状态不变。
    rejected(&mut eng, &mut s, sw, "port access vlan 20");
    assert_eq!(s.mode, Mode::Vlan);

    // E6：quit 退回上一级（系统视图），不是用户视图。
    ok(&mut eng, &mut s, sw, "quit");
    assert_eq!(s.mode, Mode::Config);
    assert_eq!(prompt(&eng, &s, sw), "[SW1]");
    ok(&mut eng, &mut s, sw, "vlan 20");
    assert_eq!(prompt(&eng, &s, sw), "[SW1-vlan20]");
    // E6：return 从任意非用户视图直达用户视图。
    ok(&mut eng, &mut s, sw, "return");
    assert_eq!(s.mode, Mode::Exec);
    assert_eq!(prompt(&eng, &s, sw), "<SW1>");
    rejected(&mut eng, &mut s, sw, "vlan 30");
}

#[test]
fn stage4a_office_l3_is_native_comware_in_both_languages() {
    let cn = block(
        LABS_CN,
        "export const lab2: LabDef = {",
        "// ---------- 实验三",
    );
    let en = block(LABS_EN, "'office-l3': {", "'stp-redundancy': {");
    for (name, text) in [("中文", cn), ("英文", en)] {
        for forbidden in [
            "G0/0/1",
            "G0/0/2",
            "vlan batch",
            "Vlanif",
            "port default vlan",
            "allow-pass",
            "Eth-Trunk",
        ] {
            assert!(
                !text.contains(forbidden),
                "{name} 实验二不得出现 {forbidden}"
            );
        }
        for required in [
            "G1/0/1",
            "G1/0/2",
            "vlan 10",
            "vlan 20",
            "port access vlan 10",
            "port access vlan 20",
            "Vlan-interface 10",
            "Vlan-interface 20",
        ] {
            assert!(text.contains(required), "{name} 实验二缺少 {required}");
        }
    }
    // 实验二设备确为 Comware 交换机，检查项仍是 2 纯读 + 1 主动 Ping。
    assert!(cn.contains("sim.add_device('Switch', 'Comware')"));
    assert_eq!(cn.matches("type: 'sviIp'").count(), 2);
    assert!(cn.contains("type: 'ping'"));
    // 任务里出现的每条 CLI 都能在 Comware 树对应模式下解析。
    for (mode, line) in [
        (Mode::Config, "vlan 10"),
        (Mode::If, "port link-type access"),
        (Mode::If, "port access vlan 10"),
        (Mode::Config, "interface Vlan-interface 10"),
        (Mode::If, "ip address 192.168.10.1 24"),
        (Mode::Exec, "display current-configuration"),
        (Mode::Exec, "display vlan"),
    ] {
        assert!(
            accepts(Dialect::Comware, mode, line),
            "实验二命令 {line:?} 应可解析"
        );
    }
}

#[test]
fn stage4a_cmdmap_is_spec_driven_and_security_gate_remains() {
    // UI 不再对 Comware 做全量降级，改为规格驱动。
    let course_view = include_str!("../../../web/src/components/CourseView.svelte");
    assert!(!course_view.contains("vendor === 'comware' && r.support === 'full-support'"));
    assert!(course_view.contains("return r.support;"));
    // 双通道：设备文本与模拟器提示在终端里各渲染一次，不双写、不伪造厂商报错。
    let terminal = include_str!("../../../web/src/components/TerminalPanel.svelte");
    assert_eq!(
        terminal.matches("ed.term.write(r.deviceOutput").count(),
        1,
        "device_output 只应渲染一次"
    );
    assert_eq!(
        terminal
            .matches("for (const n of r.simulatorNotices)")
            .count(),
        1,
        "simulator_notices 只应循环渲染一次"
    );
    // 课程表使用 Comware 原生写法，且平台专属空列语义不变。
    assert!(CMDMAP.contains("comware: 'packet-filter 3000 inbound'"));
    assert!(CMDMAP.contains("comware: 'security-zone name trust'"));
    assert!(CMDMAP.contains("comware: 'import interface g1/0/1'"));
    assert!(CMDMAP.contains("comware: 'vlan 10'"));
    assert!(CMDMAP.contains("comware: 'port trunk permit vlan 10 20'"));
    // 零拒绝：所有非空 Comware 单元格在选定 Profile 下都不得是命令错误/设备不支持。
    let mut rejected_cells = 0usize;
    let mut checked = 0usize;
    for raw in CMDMAP.split('}') {
        let Some(topic) = field(raw, "topic") else {
            continue;
        };
        let Some(cmd) = field(raw, "comware") else {
            continue;
        };
        if cmd.is_empty() {
            continue;
        }
        let mode = field(raw, "modeComware")
            .or_else(|| field(raw, "mode"))
            .unwrap_or("Exec");
        let profile = if matches!(
            topic,
            "vlan" | "stp" | "lag" | "stack" | "mlag" | "ha" | "vxlan" | "isolation"
        ) || field(raw, "role").is_some()
        {
            COMWARE_SWITCH_PROFILE
        } else {
            COMWARE_ROUTER_PROFILE
        };
        let i = inspect_command(profile, mode_of(mode), cmd);
        checked += 1;
        if matches!(
            i.support.label(),
            "invalid-command" | "unsupported-by-device"
        ) {
            rejected_cells += 1;
            println!("REJECTED {topic} | {cmd} | {}", i.support.label());
        }
        if topic == "security" {
            assert_ne!(
                i.support.label(),
                "full-support",
                "security 负向门禁: {cmd}"
            );
        }
    }
    println!("COMWARE_CELLS={checked} REJECTED={rejected_cells}");
    assert!(checked > 100, "课程表非空 Comware 单元格太少: {checked}");
    assert_eq!(rejected_cells, 0, "课程表零拒绝硬断言");
    // 分层：既有完整支持，也有非完整支持。
    assert_eq!(
        inspect_command(COMWARE_SWITCH_PROFILE, Mode::If, "port access vlan 10")
            .support
            .label(),
        "full-support"
    );
    assert_ne!(
        inspect_command(
            COMWARE_ROUTER_PROFILE,
            Mode::Config,
            "security-zone name trust"
        )
        .support
        .label(),
        "full-support"
    );
}

fn field<'a>(block: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}: '");
    let start = block.find(&needle)? + needle.len();
    let rest = &block[start..];
    Some(&rest[..rest.find('\'')?])
}

fn mode_of(s: &str) -> Mode {
    match s {
        "Exec" => Mode::Exec,
        "Priv" => Mode::Priv,
        "Config" => Mode::Config,
        "If" => Mode::If,
        "Pool" => Mode::Pool,
        "Acl" => Mode::Acl,
        "Line" => Mode::Line,
        "Aaa" => Mode::Aaa,
        "Mst" => Mode::Mst,
        "Ospf" => Mode::Ospf,
        "OspfArea" => Mode::OspfArea,
        "Bgp" => Mode::Bgp,
        "Isis" => Mode::Isis,
        "RoutePolicy" => Mode::RoutePolicy,
        "Zone" => Mode::Zone,
        "SecurityPolicy" => Mode::SecurityPolicy,
        "SecurityRule" => Mode::SecurityRule,
        "ClassMap" => Mode::ClassMap,
        "PolicyMap" => Mode::PolicyMap,
        "PolicyMapClass" => Mode::PolicyMapClass,
        "Ipsec" => Mode::Ipsec,
        "Ospfv3" => Mode::Ospfv3,
        other => panic!("未知模式 {other}"),
    }
}
