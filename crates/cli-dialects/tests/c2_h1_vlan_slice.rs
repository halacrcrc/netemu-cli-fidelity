//! 阶段 2A 流程 C2（IOS VLAN/Access/Trunk/SVI）与 H1（VRP VLAN/Trunk/SVI）闭环。
//!
//! 覆盖能力门禁（IOS 三层路由器口 L2SwitchPort 被拒）、参数校验、否定形式
//! （no switchport access vlan / undo port default vlan 恢复 VLAN1）、平台默认策略
//! （IOS trunk 全放行 / VRP trunk 仅 VLAN1）、trunk 放行前置条件与累加语义。
//! 数据面正/负向场景见 l4_scenarios.rs；此处断言状态/结构，不写逐字黄金。

use cli_dialects::{exec_line_result, state_fingerprint};
use cli_engine::spec::CommandStatus;
use cli_engine::{Mode, Session};
use sim_core::device::{DeviceKind, Dialect, PortMode};
use sim_core::engine::Engine;
use sim_core::Net;

fn dev(kind: DeviceKind, dialect: Dialect) -> (Engine, u32) {
    let mut net = Net::new();
    let d = net.add_device("D1".into(), kind, dialect);
    (Engine::new(net), d)
}

fn ok(eng: &mut Engine, s: &mut Session, d: u32, lines: &[&str]) {
    for l in lines {
        let r = exec_line_result(eng, s, d, l);
        assert_eq!(
            r.status,
            CommandStatus::Executed,
            "命令 {l:?} 应执行: {}",
            r.device_output
        );
    }
}

// ---------- C2：IOS ----------

#[test]
fn ios_router_l2_switchport_capability_rejected_and_stateless() {
    let (mut eng, d) = dev(DeviceKind::Router, Dialect::Ios);
    let mut s = Session::default();
    ok(&mut eng, &mut s, d, &["enable", "conf t"]);
    let before = state_fingerprint(&eng);
    // 三层路由器口不具备 L2 交换能力：vlan 创建被能力门禁拒绝。
    let r = exec_line_result(&mut eng, &mut s, d, "vlan 10");
    assert_eq!(r.status, CommandStatus::UnsupportedByDevice);
    assert!(!r.state_changed);
    assert_eq!(state_fingerprint(&eng), before);
    // 进入接口后 switchport 同样被拒，且不改状态。
    ok(&mut eng, &mut s, d, &["int g0/0"]);
    let before = state_fingerprint(&eng);
    let r = exec_line_result(&mut eng, &mut s, d, "switchport mode access");
    assert_eq!(r.status, CommandStatus::UnsupportedByDevice);
    assert_eq!(state_fingerprint(&eng), before);
}

#[test]
fn ios_switch_l2_commands_executed() {
    let (mut eng, d) = dev(DeviceKind::Switch, Dialect::Ios);
    let mut s = Session::default();
    ok(
        &mut eng,
        &mut s,
        d,
        &[
            "enable",
            "conf t",
            "vlan 10",
            "int g0/0",
            "switchport mode access",
            "switchport access vlan 10",
        ],
    );
    let p = &eng.net.device(d).unwrap().ports[0];
    assert_eq!(p.pvid, 10);
    assert_eq!(p.mode, PortMode::Access);
}

#[test]
fn ios_access_vlan_invalid_id_rejected_and_stateless() {
    let (mut eng, d) = dev(DeviceKind::Switch, Dialect::Ios);
    let mut s = Session::default();
    ok(&mut eng, &mut s, d, &["enable", "conf t", "int g0/0"]);
    let before = state_fingerprint(&eng);
    let r = exec_line_result(&mut eng, &mut s, d, "switchport access vlan 4095");
    assert_eq!(r.status, CommandStatus::InvalidCommand);
    assert_eq!(state_fingerprint(&eng), before);
    assert_eq!(eng.net.device(d).unwrap().ports[0].pvid, 1);
}

#[test]
fn ios_no_switchport_access_vlan_resets_to_vlan1() {
    let (mut eng, d) = dev(DeviceKind::Switch, Dialect::Ios);
    let mut s = Session::default();
    ok(
        &mut eng,
        &mut s,
        d,
        &[
            "enable",
            "conf t",
            "vlan 10",
            "int g0/0",
            "switchport access vlan 10",
        ],
    );
    assert_eq!(eng.net.device(d).unwrap().ports[0].pvid, 10);
    ok(&mut eng, &mut s, d, &["no switchport access vlan"]);
    assert_eq!(
        eng.net.device(d).unwrap().ports[0].pvid,
        1,
        "否定 access VLAN 应恢复默认 VLAN1"
    );
}

#[test]
fn ios_trunk_initial_allows_all_vlans() {
    let (mut eng, d) = dev(DeviceKind::Switch, Dialect::Ios);
    let mut s = Session::default();
    ok(
        &mut eng,
        &mut s,
        d,
        &["enable", "conf t", "int g0/0", "switchport mode trunk"],
    );
    let p = &eng.net.device(d).unwrap().ports[0];
    assert_eq!(p.mode, PortMode::Trunk);
    assert_eq!(p.trunk_allowed, None, "IOS trunk 默认放行全部 VLAN");
}

// ---------- H1：VRP ----------

#[test]
fn vrp_trunk_initial_vlan1_only() {
    let (mut eng, d) = dev(DeviceKind::Switch, Dialect::Vrp);
    let mut s = Session::default();
    ok(
        &mut eng,
        &mut s,
        d,
        &["system-view", "int g0/0/1", "port link-type trunk"],
    );
    let p = &eng.net.device(d).unwrap().ports[0];
    assert_eq!(p.mode, PortMode::Trunk);
    assert_eq!(
        p.trunk_allowed,
        Some(vec![1]),
        "VRP trunk 默认仅放行 VLAN1（平台策略注入）"
    );
}

#[test]
fn vrp_allow_pass_without_trunk_rejected_and_mode_unchanged() {
    let (mut eng, d) = dev(DeviceKind::Switch, Dialect::Vrp);
    let mut s = Session::default();
    ok(
        &mut eng,
        &mut s,
        d,
        &["system-view", "int g0/0/1", "port link-type access"],
    );
    let before = state_fingerprint(&eng);
    let r = exec_line_result(&mut eng, &mut s, d, "port trunk allow-pass vlan 10");
    assert_eq!(r.status, CommandStatus::InvalidCommand);
    assert!(
        r.device_output.contains("not a trunk port"),
        "应提示非 trunk 端口: {}",
        r.device_output
    );
    assert!(!r.state_changed);
    assert_eq!(state_fingerprint(&eng), before);
    assert_eq!(s.mode, Mode::If, "拒绝后仍停在接口视图");
    assert_eq!(eng.net.device(d).unwrap().ports[0].mode, PortMode::Access);
}

#[test]
fn vrp_allow_pass_additive_keeps_vlan1() {
    let (mut eng, d) = dev(DeviceKind::Switch, Dialect::Vrp);
    let mut s = Session::default();
    ok(
        &mut eng,
        &mut s,
        d,
        &[
            "system-view",
            "int g0/0/1",
            "port link-type trunk",
            "port trunk allow-pass vlan 10",
            "port trunk allow-pass vlan 20",
        ],
    );
    assert_eq!(
        eng.net.device(d).unwrap().ports[0].trunk_allowed,
        Some(vec![1, 10, 20]),
        "VRP allow-pass 应累加并保留 VLAN1"
    );
}

#[test]
fn vrp_undo_port_default_vlan_resets_to_vlan1() {
    let (mut eng, d) = dev(DeviceKind::Switch, Dialect::Vrp);
    let mut s = Session::default();
    ok(
        &mut eng,
        &mut s,
        d,
        &[
            "system-view",
            "vlan batch 10",
            "int g0/0/1",
            "port link-type access",
            "port default vlan 10",
        ],
    );
    assert_eq!(eng.net.device(d).unwrap().ports[0].pvid, 10);
    ok(&mut eng, &mut s, d, &["undo port default vlan"]);
    assert_eq!(
        eng.net.device(d).unwrap().ports[0].pvid,
        1,
        "undo port default vlan 应恢复 VLAN1"
    );
}
