//! 阶段 4A：实验二 `office-l3` 在真实 WASM `Sim` 上的 Comware 原生 CLI、
//! 纯读 / 主动判分契约与会话级纯读 inspection 回归。
//!
//! 与浏览器验收使用同一份 WASM 方法（exec_command_result / read_only_lab_checks /
//! run_lab_checks / session_id / verify_device_command），不使用任何假结果。

use super::*;
use serde_json::Value;

const OFFICE_L3_DEFS: &str = r#"[
  {"label":"VLAN10 SVI = 192.168.10.1","kind":{"type":"sviIp","dev":"SW1","vlan":10,"ip":"192.168.10.1"}},
  {"label":"VLAN20 SVI = 192.168.20.1","kind":{"type":"sviIp","dev":"SW1","vlan":20,"ip":"192.168.20.1"}},
  {"label":"PC1 → PC2 跨 VLAN 连通","kind":{"type":"ping","from":"PC1","to":"192.168.20.12"}}
]"#;

const LABS_CN: &str = include_str!("../../../web/src/lib/labs.ts");
const LABS_EN: &str = include_str!("../../../web/src/content/labs-en.ts");

/// 与浏览器验收脚本、cli-dialects 集成测试使用同一套课程原文解析规则：
/// 去括号附注 → 取标签冒号之后 → 按 `→` / `->` 切分。绝不另写一份「验收命令」。
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
    // 阶段 5A：标签冒号只在第一个箭头之前且后跟空格；避免 tmsh 的
    // `destination 10.0.0.100:80` 被误当成标签分隔符。
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

/// 实验二前三步的 CLI 序列直接来自课程原文；中英文必须完全一致。
fn course_cli() -> Vec<String> {
    let cn = lab2_task_lines(LABS_CN, "export const lab2: LabDef = {");
    let en = lab2_task_lines(LABS_EN, "'office-l3': {");
    assert_eq!(cn.len(), 5, "中文实验二应有 5 步任务: {cn:?}");
    assert_eq!(en.len(), 5, "英文实验二应有 5 步任务: {en:?}");
    let cn_cli: Vec<String> = cn[..3].iter().flat_map(|t| task_cli(t)).collect();
    let en_cli: Vec<String> = en[..3].iter().flat_map(|t| task_cli(t)).collect();
    assert_eq!(cn_cli, en_cli, "中英文课程原文必须给出同一条 CLI 序列");
    cn_cli
}

fn office_l3() -> (Sim, u32, u32, u32) {
    let mut sim = Sim::new();
    let pc1 = sim.add_device("Host", "Ios").unwrap();
    let sw = sim.add_device("Switch", "Comware").unwrap();
    let pc2 = sim.add_device("Host", "Ios").unwrap();
    sim.rename_device(pc1, "PC1").unwrap();
    sim.rename_device(sw, "SW1").unwrap();
    sim.rename_device(pc2, "PC2").unwrap();
    sim.add_link(pc1, sw).unwrap();
    sim.add_link(sw, pc2).unwrap();
    // 实验二任务要求「在属性面板填网关」；这里通过真实 WASM API 设置，等价且不构造假结果。
    sim.set_host_config(pc1, "192.168.10.11", 24, "192.168.10.1")
        .unwrap();
    sim.set_host_config(pc2, "192.168.20.12", 24, "192.168.20.1")
        .unwrap();
    (sim, pc1, sw, pc2)
}

fn exec(sim: &mut Sim, dev: u32, line: &str) -> Value {
    let v: Value = serde_json::from_str(&sim.exec_command_result(dev, line).unwrap()).unwrap();
    assert_eq!(v["status"], "Executed", "{line}: {v}");
    v
}

fn configure(sim: &mut Sim, sw: u32) {
    for line in course_cli() {
        exec(sim, sw, &line);
    }
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
fn stage4a_office_l3_native_cli_read_only_is_pure_and_active_is_three_of_three() {
    let (mut sim, pc1, sw, _pc2) = office_l3();
    configure(&mut sim, sw);
    // 原生配置视图：不出现任何 VRP 写法。
    let cfg = exec(&mut sim, sw, "display current-configuration");
    let cfg = cfg["deviceOutput"].as_str().unwrap();
    for forbidden in ["vlan batch", "port default vlan", "allow-pass", "Vlanif"] {
        assert!(
            !cfg.contains(forbidden),
            "{forbidden} 不得出现在 Comware 配置: {cfg}"
        );
    }
    assert!(cfg.contains("port access vlan 10"));
    assert!(cfg.contains("interface Vlan-interface10"));

    // 纯读：2 项 SVI 通过，主动 Ping 跳过；状态 / 虚拟时间 / 统计完全不变。
    let dump_before = sim.dump();
    let stats_before = sim.stats();
    let read_only_json = sim.read_only_lab_checks(OFFICE_L3_DEFS).unwrap();
    let read_only = results(&read_only_json);
    assert_eq!(read_only.len(), 3);
    assert_eq!(read_only[0]["behavior"], "readOnly");
    assert_eq!(read_only[1]["behavior"], "readOnly");
    assert_eq!(read_only[2]["behavior"], "activeExperiment");
    assert_eq!(read_only[0]["passed"], true);
    assert_eq!(read_only[1]["passed"], true);
    assert_eq!(read_only[0]["skipped"], false);
    assert_eq!(read_only[1]["skipped"], false);
    assert_eq!(read_only[2]["skipped"], true);
    assert_eq!(sim.dump(), dump_before, "纯读不得改变状态摘要");
    assert_eq!(sim.stats(), stats_before, "纯读不得推进虚拟时间/协议统计");

    // 主动判分：3/3 全通过，跨 VLAN Ping 真实成功。
    let active_json = sim.run_lab_checks(OFFICE_L3_DEFS).unwrap();
    let active = results(&active_json);
    assert_eq!(active.len(), 3);
    assert!(
        active
            .iter()
            .all(|r| r["passed"] == true && r["skipped"] == false),
        "{active:?}"
    );
    let ping = exec(&mut sim, pc1, "ping 192.168.20.12");
    let out = ping["deviceOutput"].as_str().unwrap();
    assert!(
        out.contains("5 packets transmitted, 5 received"),
        "跨 VLAN Ping 应通: {out}"
    );
}

#[test]
fn stage4a_rejudge_reflects_config_changes_without_cached_results() {
    let (mut sim, _pc1, sw, _pc2) = office_l3();
    configure(&mut sim, sw);
    let active = judge_active(&mut sim, OFFICE_L3_DEFS);
    assert!(active.iter().all(|r| r["passed"] == true), "{active:?}");

    // 改动 SVI 地址 → 重新判分必须反映失败（不缓存旧结果）。
    for line in [
        "interface Vlan-interface 10",
        "ip address 192.168.10.9 24",
        "quit",
    ] {
        exec(&mut sim, sw, line);
    }
    let broken = judge_active(&mut sim, OFFICE_L3_DEFS);
    assert!(
        !broken.iter().all(|r| r["passed"] == true),
        "改坏 SVI 后不得仍 3/3: {broken:?}"
    );
    assert_eq!(broken[0]["passed"], false);

    // 恢复 → 再次 3/3。
    for line in [
        "interface Vlan-interface 10",
        "ip address 192.168.10.1 24",
        "quit",
    ] {
        exec(&mut sim, sw, line);
    }
    let restored = judge_active(&mut sim, OFFICE_L3_DEFS);
    assert!(restored.iter().all(|r| r["passed"] == true), "{restored:?}");

    // 改动 Access VLAN → 判分再次反映失败；恢复后回绿。
    for line in ["interface g1/0/1", "port access vlan 30", "quit"] {
        exec(&mut sim, sw, line);
    }
    let access_broken = judge_read_only(&sim, OFFICE_L3_DEFS);
    assert!(access_broken.iter().any(|r| r["skipped"] == true));
    let access_broken_active = judge_active(&mut sim, OFFICE_L3_DEFS);
    assert!(!access_broken_active.iter().all(|r| r["passed"] == true));
    for line in ["interface g1/0/1", "port access vlan 10", "quit"] {
        exec(&mut sim, sw, line);
    }
    let access_restored = judge_active(&mut sim, OFFICE_L3_DEFS);
    assert!(
        access_restored.iter().all(|r| r["passed"] == true),
        "{access_restored:?}"
    );
}

#[test]
fn stage4a_session_level_comware_inspection_is_pure_and_token_scoped() {
    let (mut sim, _pc1, sw, _pc2) = office_l3();
    configure(&mut sim, sw);
    // 进入接口视图，使会话上下文为 If。
    exec(&mut sim, sw, "interface g1/0/1");
    let token = sim.session_id(sw).unwrap();
    let dump_before = sim.dump();
    let stats_before = sim.stats();
    let i: Value = serde_json::from_str(
        &sim.verify_device_command(sw, &token, "port access vlan 10")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(i["status"], "Executed");
    assert_eq!(i["support"], "full-support");
    assert!(i["detail"]
        .as_str()
        .unwrap()
        .contains("comware-generic-l3-switch"));
    assert_eq!(sim.dump(), dump_before);
    assert_eq!(sim.stats(), stats_before);
    // 错误令牌被拒绝（用不经过 JsValue 的原生纯逻辑入口，等价真实 WASM 校验）。
    assert_eq!(
        sim.verify_device_command_checked(sw, "wrong-token", "port access vlan 10")
            .unwrap_err(),
        "session id mismatch"
    );
    // Profile 级纯读：VRP 专属写法在 Comware 上是命令错误。
    let bad: Value = serde_json::from_str(
        &sim.inspect_command("comware-generic-l3-switch", "Config", "vlan batch 10 20")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(bad["support"], "invalid-command");
    // security 负向门禁仍有效。
    let sec: Value = serde_json::from_str(
        &sim.inspect_command(
            "comware-generic-router",
            "Config",
            "security-zone name trust",
        )
        .unwrap(),
    )
    .unwrap();
    assert_ne!(sec["support"], "full-support");
}

/// 真实 WASM `Sim` 上直接按课程原文（labs.ts / labs-en.ts 解析所得）逐条执行，
/// 而不是验证一份手写的单条命令清单。这是浏览器同款验收的 Rust 契约测试。
#[test]
fn stage4a_course_original_sequence_runs_on_real_wasm_sim() {
    let seq = course_cli();
    assert_eq!(seq.first().map(String::as_str), Some("system-view"));
    assert_eq!(seq.last().map(String::as_str), Some("quit"));
    assert!(seq
        .windows(3)
        .any(|w| w[0] == "vlan 10" && w[1] == "quit" && w[2] == "vlan 20"));
    assert!(seq.windows(3).any(|w| {
        w[0] == "port access vlan 10" && w[1] == "quit" && w[2] == "interface g1/0/2"
    }));
    assert!(seq.windows(3).any(|w| {
        w[0] == "ip address 192.168.10.1 24"
            && w[1] == "quit"
            && w[2] == "interface Vlan-interface 20"
    }));

    let (mut sim, pc1, sw, _pc2) = office_l3();
    let mut trace: Vec<(String, String)> = Vec::new();
    for line in &seq {
        exec(&mut sim, sw, line);
        trace.push((line.clone(), sim.prompt(sw).unwrap()));
    }
    assert_eq!(trace.last().unwrap().1, "[SW1]");
    assert!(
        trace.iter().any(|(_, p)| p == "[SW1-vlan10]"),
        "课程序列必须进入官方 VLAN 视图: {trace:?}"
    );
    assert!(
        trace.iter().any(|(_, p)| p == "[SW1-vlan20]"),
        "课程序列必须进入官方 VLAN 视图: {trace:?}"
    );

    // 原生配置视图 + 任务 5 的 display / ping。
    let cfg = exec(&mut sim, sw, "display current-configuration");
    let cfg_text = cfg["deviceOutput"].as_str().unwrap();
    for forbidden in ["vlan batch", "port default vlan", "allow-pass", "Vlanif"] {
        assert!(!cfg_text.contains(forbidden), "{forbidden}: {cfg_text}");
    }
    assert!(cfg_text.contains("port access vlan 10"));
    let ping = exec(&mut sim, pc1, "ping 192.168.20.12");
    assert!(
        ping["deviceOutput"]
            .as_str()
            .unwrap()
            .contains("5 packets transmitted, 5 received"),
        "{ping}"
    );

    // 纯读 2 通过 + 1 跳过且状态/时间/统计不变；主动 3/3。
    let dump_before = sim.dump();
    let stats_before = sim.stats();
    let ro = judge_read_only(&sim, OFFICE_L3_DEFS);
    assert_eq!(ro.len(), 3);
    assert_eq!(ro[0]["passed"], true);
    assert_eq!(ro[1]["passed"], true);
    assert_eq!(ro[2]["skipped"], true);
    assert_eq!(sim.dump(), dump_before);
    assert_eq!(sim.stats(), stats_before);
    let active = judge_active(&mut sim, OFFICE_L3_DEFS);
    assert!(
        active.iter().all(|r| r["passed"] == true),
        "课程原文执行完应 3/3: {active:?}"
    );
}
