//! 阶段 5A：F5 TMOS 17.x / tmsh 在真实 WASM `Sim` 上的契约测试。
//!
//! 与 Rust 集成测试、浏览器验收脚本共用同一套「课程原文解析」规则，
//! 不维护手写的验收命令清单；全部结论来自真实 WASM 方法。

use super::*;
use serde_json::Value;

const F5_LTM_DEFS: &str = r#"[
  {"label":"LB1 虚拟服务器 VS = 10.0.0.100:80（纯读）","kind":{"type":"ltmVirtual","dev":"LB1","name":"VS","vip":"10.0.0.100","port":80}},
  {"label":"三次 TCP 会话实际分配到不同 Member（主动探测）","kind":{"type":"ltmDistribution","dev":"LB1","from":"PC1","to":"10.0.0.100","port":80,"sessions":3,"minDistinct":2}}
]"#;

const LABS_CN: &str = include_str!("../../../web/src/lib/labs.ts");
const LABS_EN: &str = include_str!("../../../web/src/content/labs-en.ts");

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
        .filter(|x| !x.is_empty())
        .map(String::from)
        .collect()
}

fn lab_tasks(src: &str, marker: &str) -> Vec<String> {
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

fn course_cli() -> Vec<String> {
    let cn = lab_tasks(LABS_CN, "export const lab19: LabDef = {");
    let en = lab_tasks(LABS_EN, "'f5-ltm': {");
    assert_eq!(cn.len(), 5, "中文 F5 实验应有 5 步: {cn:?}");
    assert_eq!(en.len(), 5, "英文 F5 实验应有 5 步: {en:?}");
    let cn_cli: Vec<String> = cn[..3].iter().flat_map(|t| task_cli(t)).collect();
    let en_cli: Vec<String> = en[..3].iter().flat_map(|t| task_cli(t)).collect();
    assert_eq!(cn_cli, en_cli, "中英文课程原文必须给出同一条 tmsh 序列");
    cn_cli
}

fn f5_lab() -> (Sim, u32, u32, u32) {
    let mut sim = Sim::new();
    let pc = sim.add_device("Host", "Ios").unwrap();
    let lb = sim.add_device("LoadBalancer", "Tmos").unwrap();
    let s1 = sim.add_device("Host", "Ios").unwrap();
    let s2 = sim.add_device("Host", "Ios").unwrap();
    sim.rename_device(pc, "PC1").unwrap();
    sim.rename_device(lb, "LB1").unwrap();
    sim.rename_device(s1, "SRV1").unwrap();
    sim.rename_device(s2, "SRV2").unwrap();
    sim.add_link(pc, lb).unwrap();
    sim.add_link(lb, s1).unwrap();
    sim.add_link(lb, s2).unwrap();
    sim.set_host_config(pc, "10.0.0.10", 24, "10.0.0.1")
        .unwrap();
    sim.set_host_config(s1, "10.0.1.10", 24, "10.0.1.1")
        .unwrap();
    sim.set_host_config(s2, "10.0.2.11", 24, "10.0.2.1")
        .unwrap();
    sim.set_port_ip(lb, 0, "10.0.0.1", 24).unwrap();
    sim.set_port_ip(lb, 1, "10.0.1.1", 24).unwrap();
    sim.set_port_ip(lb, 2, "10.0.2.1", 24).unwrap();
    (sim, pc, lb, s2)
}

fn exec(sim: &mut Sim, dev: u32, line: &str) -> Value {
    let v: Value = serde_json::from_str(&sim.exec_command_result(dev, line).unwrap()).unwrap();
    assert_eq!(v["status"], "Executed", "{line}: {v}");
    v
}

fn results(json: &str) -> Vec<Value> {
    serde_json::from_str::<Vec<Value>>(json)
        .unwrap_or_else(|e| panic!("判分结果不是数组: {e} / {json}"))
}

fn judge_read_only(sim: &Sim, defs: &str) -> Vec<Value> {
    let json = sim.read_only_lab_checks(defs).unwrap();
    results(&json)
}

fn judge_active(sim: &mut Sim, defs: &str) -> Vec<Value> {
    let json = sim.run_lab_checks(defs).unwrap();
    results(&json)
}

#[test]
fn stage5a_f5_lab_course_original_sequence_runs_on_real_wasm_sim() {
    let seq = course_cli();
    assert_eq!(seq.first().map(String::as_str), Some("ltm"));
    assert_eq!(seq.last().map(String::as_str), Some("quit"));
    assert_eq!(seq.len(), 12, "前三步共 12 条命令: {seq:?}");
    assert!(seq
        .windows(2)
        .any(|w| w[0] == "sys" && w[1] == "list version"));

    let (mut sim, pc, lb, _srv2) = f5_lab();
    let mut trace: Vec<(String, String)> = Vec::new();
    for line in &seq {
        exec(&mut sim, lb, line);
        trace.push((line.clone(), sim.prompt(lb).unwrap()));
    }
    assert_eq!(trace.last().unwrap().1, "(tmos)#");
    assert!(
        trace.iter().any(|(_, p)| p == "(tmos.ltm)#"),
        "tmsh ltm 模块提示符应出现: {trace:?}"
    );
    assert!(
        trace.iter().any(|(_, p)| p == "(tmos.sys)#"),
        "tmsh sys 模块提示符应出现: {trace:?}"
    );

    // 纯读：ltmVirtual 通过，ldistribution 跳过；状态 / 时间 / 统计不变。
    let dump_before = sim.dump();
    let stats_before = sim.stats();
    let ro = judge_read_only(&sim, F5_LTM_DEFS);
    assert_eq!(ro.len(), 2);
    assert_eq!(ro[0]["passed"], true);
    assert_eq!(ro[1]["skipped"], true);
    assert_eq!(sim.dump(), dump_before, "纯读不得改变状态摘要");
    assert_eq!(sim.stats(), stats_before, "纯读不得推进虚拟时间/统计");

    // 主动：真实 TCP 会话分配到不同 Member。
    let active = judge_active(&mut sim, F5_LTM_DEFS);
    assert!(
        active.iter().all(|r| r["passed"] == true),
        "主动判分应 2/2: {active:?}"
    );
    assert!(
        active[1]["detail"]
            .as_str()
            .unwrap_or("")
            .contains("不同 Member"),
        "{active:?}"
    );
    let _ = pc;
    let _ = lb;
}

#[test]
fn stage5a_tmos_dual_channel_and_session_inspection() {
    let (mut sim, _pc, lb, _srv2) = f5_lab();
    for line in course_cli() {
        exec(&mut sim, lb, &line);
    }
    sim.exec_command_result(lb, "ltm").unwrap();
    let before = sim.dump();
    let token = sim.session_id(lb).unwrap();
    let ok = sim
        .verify_device_command_checked(lb, &token, "list pool /Common/WEB")
        .unwrap();
    assert_eq!(ok.support.label(), "full-support");
    assert_eq!(sim.dump(), before, "会话级 inspection 必须纯读");
    assert_eq!(
        sim.verify_device_command_checked(lb, "wrong-token", "list pool /Common/WEB")
            .unwrap_err(),
        "session id mismatch"
    );

    // 双通道：正常查询只有设备输出；非 /Common 分区只有模拟器提示。
    let normal = exec(&mut sim, lb, "list pool /Common/WEB");
    assert!(normal["deviceOutput"]
        .as_str()
        .unwrap()
        .contains("/Common/WEB"));
    assert_eq!(normal["simulatorNotices"].as_array().unwrap().len(), 0);
    // 当前会话在 (tmos.ltm)# 模块内，使用组件名相对写法。
    let not_sim: Value = serde_json::from_str(
        &sim.exec_command_result(lb, "create pool /TenantA/WEB")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(not_sim["status"], "NotSimulated");
    assert_eq!(not_sim["deviceOutput"], "");
    assert!(!not_sim["simulatorNotices"].as_array().unwrap().is_empty());
    let legacy = sim.exec_command(lb, "create pool /TenantA/WEB").unwrap();
    // 旧文本入口把模拟器提示合并进单通道；这里每条提示各占一行，不重复渲染设备输出。
    assert!(
        legacy.matches("[netemu]").count() >= 1,
        "旧文本入口必须保留模拟器提示: {legacy}"
    );
    assert!(
        !legacy.contains("ltm pool /Common/WEB {"),
        "不得混入设备输出: {legacy}"
    );
}

#[test]
fn stage5a_legacy_f5_save_is_migrated_with_visible_notices_and_state_preserved() {
    // 1) 用当前 Tmos 拓扑生成一份真实存档。
    let (mut sim, _pc, lb, _srv2) = f5_lab();
    for line in course_cli() {
        exec(&mut sim, lb, &line);
    }
    let mut summary: Value = serde_json::from_str(&sim.dump()).unwrap();
    let devices = summary["devices"].as_array_mut().unwrap();
    let lb_json = devices.iter_mut().find(|d| d["id"] == lb).unwrap();
    assert!(!lb_json["ltm"]["pools"].as_array().unwrap().is_empty());
    // 2) 退回阶段 5A 之前的旧形态：LoadBalancer + Vrp + 旧 Profile（无 Tmos 字段）。
    lb_json["dialect"] = Value::String("Vrp".into());
    lb_json["profile"] = Value::String("vrp-generic-router".into());
    let legacy_json = serde_json::to_string(&summary).unwrap();

    // 3) 加载旧档：映射为 Tmos、保留 Pool/VS、提示可见、会话清空。
    let old_token = sim.session_id(lb).unwrap();
    let report = sim.apply_summary_checked(&legacy_json).unwrap();
    assert!(
        report
            .notices
            .iter()
            .any(|n| n.contains("旧 F5 存档已映射")),
        "{report:?}"
    );
    assert!(
        report
            .notices
            .iter()
            .any(|n| n.contains("Dialect::Tmos") && n.contains("LB1")),
        "{report:?}"
    );
    assert!(
        report
            .notices
            .iter()
            .any(|n| n.contains("重置所有终端会话")),
        "{report:?}"
    );
    assert!(report.mapped_profiles >= 1);

    let after: Value = serde_json::from_str(&sim.dump()).unwrap();
    let lb_after = after["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == lb)
        .unwrap();
    assert_eq!(lb_after["dialect"], "Tmos");
    assert_eq!(lb_after["profile"], profile_mod::TMOS_LB_PROFILE);
    assert_eq!(
        lb_after["ltm"]["pools"][0]["members"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "旧档 Pool 成员必须原样迁移"
    );
    assert_eq!(
        lb_after["ltm"]["virtuals"][0]["vip"], "10.0.0.100",
        "旧档 Virtual Server 必须原样迁移"
    );
    // 加载后旧会话令牌失效，且 tmsh 命令在新方言下可解析、抽象命令不可解析。
    assert_eq!(
        sim.verify_device_command_checked(lb, &old_token, "list ltm pool /Common/WEB")
            .unwrap_err(),
        "no active device session"
    );
    let new_token = sim.session_id(lb).unwrap();
    let ok = sim
        .verify_device_command_checked(lb, &new_token, "list ltm pool /Common/WEB")
        .unwrap();
    assert_eq!(ok.support.label(), "full-support");
}
