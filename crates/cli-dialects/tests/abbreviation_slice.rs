//! 阶段 2A 横切：歧义缩写在真实方言执行链上的行为。
//!
//! 验证 Ambiguous 是独立解析结果（非 Unknown/TooMany），经 exec_line_result 落到
//! InvalidCommand + 方言专属报错文本，且不改状态；唯一缩写与全称执行等价。

use cli_dialects::{exec_and_run_result, exec_line_result, state_fingerprint};
use cli_engine::spec::CommandStatus;
use cli_engine::Session;
use sim_core::device::{DeviceKind, Dialect};
use sim_core::engine::Engine;
use sim_core::Net;

fn eng_dev(dialect: Dialect) -> (Engine, u32) {
    let mut net = Net::new();
    let d = net.add_device("D1".into(), DeviceKind::Router, dialect);
    (Engine::new(net), d)
}

#[test]
fn ios_ambiguous_abbreviation_is_invalid_and_stateless() {
    let (mut eng, d) = eng_dev(Dialect::Ios);
    let mut s = Session::default();
    exec_line_result(&mut eng, &mut s, d, "enable");
    let before = state_fingerprint(&eng);
    // show i 同时命中 ip / ipv6：必须 Ambiguous，而非 Unknown 或 TooMany。
    let r = exec_line_result(&mut eng, &mut s, d, "show i");
    assert_eq!(r.status, CommandStatus::InvalidCommand);
    assert_eq!(r.device_output, "% Ambiguous command:  \"i\"");
    assert!(!r.state_changed);
    assert_eq!(state_fingerprint(&eng), before, "歧义命令不得改状态");
}

#[test]
fn vrp_ambiguous_abbreviation_is_invalid_and_stateless() {
    let (mut eng, d) = eng_dev(Dialect::Vrp);
    let mut s = Session::default();
    let before = state_fingerprint(&eng);
    let r = exec_line_result(&mut eng, &mut s, d, "display i");
    assert_eq!(r.status, CommandStatus::InvalidCommand);
    assert_eq!(
        r.device_output,
        "Error: Ambiguous command found at '^' position."
    );
    assert!(!r.state_changed);
    assert_eq!(state_fingerprint(&eng), before);
}

#[test]
fn comware_ambiguous_abbreviation_is_invalid_and_stateless() {
    let (mut eng, d) = eng_dev(Dialect::Comware);
    let mut s = Session::default();
    let before = state_fingerprint(&eng);
    let r = exec_line_result(&mut eng, &mut s, d, "display i");
    assert_eq!(r.status, CommandStatus::InvalidCommand);
    assert_eq!(
        r.device_output,
        "% Ambiguous command found at '^' position."
    );
    assert!(!r.state_changed);
    assert_eq!(state_fingerprint(&eng), before);
}

#[test]
fn ios_unique_abbreviation_equals_full_command() {
    let (mut eng, d) = eng_dev(Dialect::Ios);
    let mut s = Session::default();
    exec_line_result(&mut eng, &mut s, d, "enable");
    let full = exec_and_run_result(&mut eng, &mut s, d, "show running-config");
    let abbr = exec_and_run_result(&mut eng, &mut s, d, "show run");
    assert_eq!(full.status, CommandStatus::Executed);
    assert_eq!(abbr.status, CommandStatus::Executed);
    assert_eq!(
        full.device_output, abbr.device_output,
        "唯一缩写 show run 应与 show running-config 完全等价"
    );
}

#[test]
fn vrp_unique_abbreviation_equals_full_command() {
    let (mut eng, d) = eng_dev(Dialect::Vrp);
    let mut s = Session::default();
    let full = exec_and_run_result(&mut eng, &mut s, d, "display current-configuration");
    let abbr = exec_and_run_result(&mut eng, &mut s, d, "display current");
    assert_eq!(full.status, CommandStatus::Executed);
    assert_eq!(abbr.status, CommandStatus::Executed);
    assert_eq!(
        full.device_output, abbr.device_output,
        "唯一缩写 display current 应与全称完全等价"
    );
}

#[test]
fn unknown_command_stays_distinct_from_ambiguous() {
    let (mut eng, d) = eng_dev(Dialect::Ios);
    let mut s = Session::default();
    exec_line_result(&mut eng, &mut s, d, "enable");
    let unknown = exec_line_result(&mut eng, &mut s, d, "show zzznope");
    assert_eq!(unknown.status, CommandStatus::InvalidCommand);
    assert!(
        unknown.device_output.contains("Invalid input detected"),
        "未知命令应走 Invalid input，而非 Ambiguous: {}",
        unknown.device_output
    );
    assert!(!unknown.device_output.contains("Ambiguous"));
}
