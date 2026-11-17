//! 阶段 2A 本切片 config_render 修复的字段/结构断言。
//!
//! 不写逐字黄金（无带版本设备样本）：通过真实 CLI 链建立状态，再对渲染输出断言
//! “正确字段存在 + 错误形态不存在”，覆盖 IOS 静态路由点分掩码、IOS SVI 点分掩码、
//! IOS username 真实 privilege，以及 VRP 前缀长度形态保持不变。

use cli_dialects::exec_and_run;
use cli_engine::Session;
use sim_core::device::{DeviceKind, Dialect};
use sim_core::engine::Engine;
use sim_core::Net;

fn run_seq(net: Net, dev: u32, lines: &[&str]) -> String {
    let mut eng = Engine::new(net);
    let mut sess = Session::default();
    let mut last = String::new();
    for l in lines {
        last = exec_and_run(&mut eng, &mut sess, dev, l);
    }
    last
}

fn router(dialect: Dialect) -> (Net, u32) {
    let mut net = Net::new();
    let r = net.add_device("R1".into(), DeviceKind::Router, dialect);
    (net, r)
}

fn switch(dialect: Dialect) -> (Net, u32) {
    let mut net = Net::new();
    let s = net.add_device("SW1".into(), DeviceKind::Switch, dialect);
    (net, s)
}

#[test]
fn ios_static_route_renders_dotted_mask_not_prefix_len() {
    let (net, r) = router(Dialect::Ios);
    let cfg = run_seq(
        net,
        r,
        &[
            "enable",
            "conf t",
            "ip route 192.168.2.0 255.255.255.0 10.0.0.2",
            "end",
            "show running-config",
        ],
    );
    assert!(
        cfg.contains("ip route 192.168.2.0 255.255.255.0 10.0.0.2"),
        "IOS 静态路由应渲染点分掩码: {cfg}"
    );
    assert!(
        !cfg.contains("ip route 192.168.2.0 24 "),
        "IOS 静态路由不得渲染前缀长度: {cfg}"
    );
}

#[test]
fn ios_svi_renders_dotted_mask() {
    let (net, s) = switch(Dialect::Ios);
    let cfg = run_seq(
        net,
        s,
        &[
            "enable",
            "conf t",
            "vlan 10",
            "int vlan 10",
            "ip address 192.168.10.1 255.255.255.0",
            "end",
            "show running-config",
        ],
    );
    assert!(cfg.contains("interface Vlan10"), "应有 SVI 接口块: {cfg}");
    assert!(
        cfg.contains("ip address 192.168.10.1 255.255.255.0"),
        "IOS SVI 应渲染点分掩码: {cfg}"
    );
    assert!(
        !cfg.contains("ip address 192.168.10.1 24"),
        "IOS SVI 不得渲染前缀长度: {cfg}"
    );
}

#[test]
fn ios_username_renders_stored_privilege_and_masks_secret() {
    let (net, r) = router(Dialect::Ios);
    let cfg = run_seq(
        net,
        r,
        &[
            "enable",
            "conf t",
            "username admin privilege 7 secret Cisco@123",
            "end",
            "show running-config",
        ],
    );
    assert!(
        cfg.contains("username admin privilege 7 secret ***"),
        "username 应渲染真实 privilege（7）而非硬编码 15: {cfg}"
    );
    assert!(
        !cfg.contains("privilege 15"),
        "不得硬编码 privilege 15: {cfg}"
    );
    assert!(
        !cfg.contains("Cisco@123"),
        "配置视图不得回显明文口令: {cfg}"
    );
}

#[test]
fn vrp_static_route_and_svi_keep_prefix_len() {
    let (net, s) = switch(Dialect::Vrp);
    let cfg = run_seq(
        net,
        s,
        &[
            "system-view",
            "ip route-static 192.168.2.0 24 10.0.0.2",
            "vlan batch 10",
            "int vlanif 10",
            "ip address 192.168.10.1 24",
            "quit",
            "display current-configuration",
        ],
    );
    assert!(
        cfg.contains("ip route-static 192.168.2.0 24 10.0.0.2"),
        "VRP 静态路由应保持前缀长度: {cfg}"
    );
    assert!(cfg.contains("interface Vlanif10"), "应有 Vlanif 块: {cfg}");
    assert!(
        cfg.contains("ip address 192.168.10.1 24"),
        "VRP SVI 应保持前缀长度: {cfg}"
    );
}
