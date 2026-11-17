//! 规范编号 C6a：IOS 管理与登录闭环。
//!
//! 编号更正（Stage 2B 前置）：主方案 §8 中 C3 是 `IOS DHCP Pool / network /
//! default-router`，C6a 才是本地用户、VTY、`login local` 与认证/传输配置。
//! Stage 2A 实施记录把本切片误标为「C3」，属历史记录，保留不回写；本文件保留
//! 全部既有管理测试，仅把当前源码注释改为 canonical C6a。真正 C3 DHCP 由
//! `stage2b_flows.rs` 的 C3 段补做，不得据此认定 DHCP 已完成。
//!
//! 覆盖本地 enable 口令门禁（无 secret 直进特权 / 有 secret 先提示口令，正确进特权、
//! 错误拒绝并留在用户模式且不改设备状态）、privilege 边界（>15 拒绝且无副作用、15 接受）、
//! 以及管理面配置视图（真实 privilege、login local、transport、口令掩码）。
//! 无带版本设备样本，故采用字段/结构/状态断言，不写逐字黄金。

use cli_dialects::{exec_line_result, prompt, state_fingerprint};
use cli_engine::spec::CommandStatus;
use cli_engine::{Mode, Session};
use sim_core::device::{DeviceKind, Dialect};
use sim_core::engine::Engine;
use sim_core::Net;

fn lab() -> (Engine, u32) {
    let mut net = Net::new();
    let r = net.add_device("R1".into(), DeviceKind::Router, Dialect::Ios);
    (Engine::new(net), r)
}

fn run(eng: &mut Engine, s: &mut Session, d: u32, lines: &[&str]) {
    for l in lines {
        let r = exec_line_result(eng, s, d, l);
        assert_eq!(
            r.status,
            CommandStatus::Executed,
            "命令 {l:?} 应执行成功: {}",
            r.device_output
        );
    }
}

#[test]
fn ios_enable_without_secret_enters_priv_directly() {
    let (mut eng, d) = lab();
    let mut s = Session::default();
    assert_eq!(prompt(&eng, &s, d), "R1>");
    let r = exec_line_result(&mut eng, &mut s, d, "enable");
    assert_eq!(r.status, CommandStatus::Executed);
    assert!(!s.enable_prompt, "无 enable secret 时不应等待口令");
    assert_eq!(s.mode, Mode::Priv);
    assert_eq!(prompt(&eng, &s, d), "R1#");
}

#[test]
fn ios_enable_with_secret_prompts_then_gates() {
    let (mut eng, d) = lab();
    let mut s = Session::default();
    run(
        &mut eng,
        &mut s,
        d,
        &[
            "enable",
            "conf t",
            "enable secret Cisco@123",
            "end",
            "disable",
        ],
    );
    // 回到用户模式后再次 enable：必须先提示口令，而不是直接进特权。
    let r = exec_line_result(&mut eng, &mut s, d, "enable");
    assert_eq!(r.status, CommandStatus::Executed);
    assert!(s.enable_prompt, "配置了 enable secret 后应等待口令");
    assert_eq!(s.mode, Mode::Exec, "口令校验前不得进入特权模式");
    assert_eq!(prompt(&eng, &s, d), "Password: ");

    // 错误口令：拒绝、留在用户模式、清除等待标志。
    let before = state_fingerprint(&eng);
    let r = exec_line_result(&mut eng, &mut s, d, "wrong-pass");
    assert!(
        r.device_output.contains("Access denied"),
        "错误口令应拒绝: {}",
        r.device_output
    );
    assert!(!s.enable_prompt);
    assert_eq!(s.mode, Mode::Exec);
    assert_eq!(prompt(&eng, &s, d), "R1>");
    assert_eq!(state_fingerprint(&eng), before, "口令交互不得改设备状态");

    // 正确口令：进入特权模式。
    let r = exec_line_result(&mut eng, &mut s, d, "enable");
    assert_eq!(r.status, CommandStatus::Executed);
    assert_eq!(prompt(&eng, &s, d), "Password: ");
    let r = exec_line_result(&mut eng, &mut s, d, "Cisco@123");
    assert_eq!(r.status, CommandStatus::Executed);
    assert!(!s.enable_prompt);
    assert_eq!(s.mode, Mode::Priv);
    assert_eq!(prompt(&eng, &s, d), "R1#");
}

#[test]
fn ios_enable_gate_does_not_leak_secret_to_device_output() {
    let (mut eng, d) = lab();
    let mut s = Session::default();
    run(
        &mut eng,
        &mut s,
        d,
        &["enable", "conf t", "enable secret Cisco@123", "end"],
    );
    let r = exec_line_result(&mut eng, &mut s, d, "enable");
    assert!(!r.device_output.contains("Cisco@123"));
    let r = exec_line_result(&mut eng, &mut s, d, "Cisco@123");
    assert!(
        !r.device_output.contains("Cisco@123"),
        "口令回显不得进入设备输出: {}",
        r.device_output
    );
}

#[test]
fn ios_privilege_above_15_rejected_and_stateless() {
    let (mut eng, d) = lab();
    let mut s = Session::default();
    run(&mut eng, &mut s, d, &["enable", "conf t"]);
    let before = state_fingerprint(&eng);
    let r = exec_line_result(
        &mut eng,
        &mut s,
        d,
        "username admin privilege 16 secret Cisco@123",
    );
    assert_eq!(r.status, CommandStatus::InvalidCommand);
    assert!(!r.state_changed);
    assert_eq!(state_fingerprint(&eng), before, "非法 privilege 不得改状态");
    assert!(
        eng.net.device(d).unwrap().mgmt.users.is_empty(),
        "拒绝后不得创建用户"
    );
}

#[test]
fn ios_privilege_boundary_15_accepted_and_stored() {
    let (mut eng, d) = lab();
    let mut s = Session::default();
    run(&mut eng, &mut s, d, &["enable", "conf t"]);
    let r = exec_line_result(
        &mut eng,
        &mut s,
        d,
        "username admin privilege 15 secret Cisco@123",
    );
    assert_eq!(r.status, CommandStatus::Executed);
    let users = &eng.net.device(d).unwrap().mgmt.users;
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "admin");
    assert_eq!(users[0].privilege, 15);
}

#[test]
fn ios_mgmt_config_view_masks_secrets_and_renders_vty() {
    let (mut eng, d) = lab();
    let mut s = Session::default();
    run(
        &mut eng,
        &mut s,
        d,
        &[
            "enable",
            "conf t",
            "enable secret Enable@123",
            "username admin privilege 7 secret Cisco@123",
            "line vty 0 4",
            "login local",
            "transport input ssh",
            "exit",
            "end",
        ],
    );
    let cfg = exec_line_result(&mut eng, &mut s, d, "show running-config").device_output;
    assert!(
        cfg.contains("enable secret ***"),
        "应掩码渲染 enable secret: {cfg}"
    );
    assert!(
        cfg.contains("username admin privilege 7 secret ***"),
        "应渲染真实 privilege 并掩码口令: {cfg}"
    );
    assert!(cfg.contains("line vty 0 4"), "应有 VTY 块: {cfg}");
    assert!(cfg.contains(" login local"), "应渲染 login local: {cfg}");
    assert!(
        cfg.contains(" transport input ssh"),
        "应渲染 transport input ssh: {cfg}"
    );
    assert!(!cfg.contains("Cisco@123"), "不得回显用户明文口令: {cfg}");
    assert!(
        !cfg.contains("Enable@123"),
        "不得回显 enable 明文口令: {cfg}"
    );
}
