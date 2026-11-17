//! 阶段 6A：防火墙基线（华为 VRP 安全策略）在真实 WASM `Sim` 上的契约测试。
//!
//! 与 Rust 集成测试、浏览器验收脚本共用同一套「课程原文解析」规则，
//! 不维护手写的验收命令清单；全部结论来自真实 `Sim` 方法。

use super::*;
use serde_json::Value;

/// lab10 的检查项（3 纯读 + 2 主动），与 `web/src/lib/labs.ts` 的 `lab10.checks` 同形。
const LAB10_CHECKS: &str = r#"[
  {"label":"FW1 G0/0/0 已划入 trust 区域","kind":{"type":"securityZone","dev":"FW1","port":0,"zone":"trust"}},
  {"label":"FW1 G0/0/1 已划入 untrust 区域","kind":{"type":"securityZone","dev":"FW1","port":1,"zone":"untrust"}},
  {"label":"存在启用的 trust -> untrust permit 策略","kind":{"type":"securityPolicy","dev":"FW1","name":"allow-out","srcZone":"trust","dstZone":"untrust","permit":true}},
  {"label":"SRV -> PC1 被缺省策略拒绝","kind":{"type":"ping","from":"SRV","to":"192.168.1.10","expect":false}},
  {"label":"PC1 -> SRV 放行（回包走会话表）","kind":{"type":"ping","from":"PC1","to":"203.0.113.10"}}
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

/// 去括号附注 → 只在第一个箭头之前识别标签冒号 → 按 `→` / `->` 切分。
fn task_label_and_cli(task: &str) -> (String, Vec<String>) {
    let clean = strip_task_notes(task);
    let arrow = clean
        .find("->")
        .or_else(|| clean.find('→'))
        .unwrap_or(clean.len());
    let head = &clean[..arrow];
    let mut label = String::new();
    let body = if let Some(p) = head.find('：') {
        label = head[..p].trim().to_string();
        clean[p + '：'.len_utf8()..].to_string()
    } else if let Some(p) = head.find(": ") {
        label = head[..p].trim().to_string();
        clean[p + 2..].to_string()
    } else {
        clean.clone()
    };
    let cmds = body
        .replace('→', "->")
        .split("->")
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(String::from)
        .collect();
    (label, cmds)
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

/// 课程原文的 (标签, CLI 序列) 列表；中英文必须完全一致。
fn lab10_course() -> Vec<(String, Vec<String>)> {
    let cn = lab_tasks(LABS_CN, "export const lab10: LabDef = {");
    let en = lab_tasks(LABS_EN, "'firewall-zone': {");
    assert_eq!(cn.len(), en.len(), "中英文 lab10 步数必须一致");
    let cn_parsed: Vec<(String, Vec<String>)> = cn.iter().map(|t| task_label_and_cli(t)).collect();
    let en_parsed: Vec<(String, Vec<String>)> = en.iter().map(|t| task_label_and_cli(t)).collect();
    let cn_cli: Vec<Vec<String>> = cn_parsed.iter().map(|(_, c)| c.clone()).collect();
    let en_cli: Vec<Vec<String>> = en_parsed.iter().map(|(_, c)| c.clone()).collect();
    assert_eq!(cn_cli, en_cli, "中英文课程原文必须给出同一条 CLI 序列");
    cn_parsed
}

fn fw_lab() -> (Sim, u32, u32, u32) {
    let mut sim = Sim::new();
    let pc = sim.add_device("Host", "Ios").unwrap();
    let fw = sim.add_device("Firewall", "Vrp").unwrap();
    let srv = sim.add_device("Host", "Ios").unwrap();
    sim.rename_device(pc, "PC1").unwrap();
    sim.rename_device(fw, "FW1").unwrap();
    sim.rename_device(srv, "SRV").unwrap();
    sim.add_link(pc, fw).unwrap();
    sim.add_link(fw, srv).unwrap();
    sim.set_host_config(pc, "192.168.1.10", 24, "192.168.1.1")
        .unwrap();
    sim.set_host_config(srv, "203.0.113.10", 24, "203.0.113.1")
        .unwrap();
    (sim, pc, fw, srv)
}

fn exec_ok(sim: &mut Sim, dev: u32, line: &str) -> Value {
    let v: Value = serde_json::from_str(&sim.exec_command_result(dev, line).unwrap()).unwrap();
    assert_eq!(v["status"], "Executed", "{line}: {v}");
    v
}

fn results(json: &str) -> Vec<Value> {
    serde_json::from_str::<Vec<Value>>(json)
        .unwrap_or_else(|e| panic!("判分结果不是数组: {e} / {json}"))
}

fn judge_read_only(sim: &Sim, defs: &str) -> Vec<Value> {
    results(&sim.read_only_lab_checks(defs).unwrap())
}

fn judge_active(sim: &mut Sim, defs: &str) -> Vec<Value> {
    results(&sim.run_lab_checks(defs).unwrap())
}

/// 课程原文里所有发往 FW1 的命令，按出现顺序（用于分段执行）。
fn fw_commands() -> Vec<String> {
    lab10_course()
        .into_iter()
        .filter(|(label, _)| label == "FW1")
        .flat_map(|(_, cmds)| cmds)
        .collect()
}

#[test]
fn stage6a_lab10_course_original_sequence_runs_on_real_wasm_sim() {
    let seq = lab10_course();
    assert!(seq.len() >= 10, "lab10 步数: {}", seq.len());
    assert!(
        fw_commands()
            .iter()
            .any(|l| l == "reset firewall session table"),
        "课程必须覆盖清会话表"
    );

    let (mut sim, pc, fw, srv) = fw_lab();
    let mut prompts: Vec<String> = Vec::new();
    for (label, cmds) in &seq {
        let dev = match label.as_str() {
            "FW1" => fw,
            "PC1" => pc,
            "SRV" => srv,
            other => panic!("未知任务标签 {other}"),
        };
        for line in cmds {
            exec_ok(&mut sim, dev, line);
            prompts.push(sim.prompt(dev).unwrap());
        }
    }
    assert!(
        prompts.iter().any(|p| p == "[FW1-zone-trust]"),
        "区域视图提示符应出现: {prompts:?}"
    );
    assert!(
        prompts
            .iter()
            .any(|p| p == "[FW1-security-policy-rule-allow-out]"),
        "规则视图提示符应出现: {prompts:?}"
    );

    // 纯读：3 个检查项通过、2 个主动项跳过；状态/时间/统计不变。
    let dump_before = sim.dump();
    let stats_before = sim.stats();
    let ro = judge_read_only(&sim, LAB10_CHECKS);
    assert_eq!(ro.len(), 5);
    // 区域两项与课程末态一致，通过。
    assert_eq!(ro[0]["passed"], true, "纯读项应通过: {:?}", ro[0]);
    assert_eq!(ro[1]["passed"], true, "纯读项应通过: {:?}", ro[1]);
    for r in ro.iter().skip(3) {
        assert_eq!(r["skipped"], true, "主动项在纯读路径必须跳过: {r}");
    }
    assert_eq!(sim.dump(), dump_before, "纯读不得改变状态摘要");
    assert_eq!(sim.stats(), stats_before, "纯读不得推进虚拟时间/统计");

    // 课程末态：allow-out 已被 `disable` → 策略项不得再判通过（重判分反映现状，
    // 不缓存旧结论；明细里 STATE 显示 disable）。
    assert_eq!(
        ro[2]["passed"], false,
        "规则禁用后不得仍判通过: {:?}",
        ro[2]
    );
    assert!(
        ro[2]["detail"].as_str().unwrap_or("").contains("disable"),
        "明细必须说明规则已禁用: {:?}",
        ro[2]
    );
}

#[test]
fn stage6a_lab10_active_scoring_and_policy_change_are_visible() {
    let cmds = fw_commands();
    let cut = cmds
        .iter()
        .position(|l| l == "disable")
        .expect("课程必须包含规则禁用步骤");

    let (mut sim, _pc, fw, _srv) = fw_lab();
    for line in &cmds[..cut] {
        exec_ok(&mut sim, fw, line);
    }
    let active = judge_active(&mut sim, LAB10_CHECKS);
    assert!(
        active.iter().all(|r| r["passed"] == true),
        "启用状态下主动判分应 5/5: {active:?}"
    );
    assert!(
        active[4]["detail"].as_str().unwrap_or("").contains("连通"),
        "正向 ping 明细应说明连通: {:?}",
        active[4]
    );

    // 继续执行禁用 + 清表，再重判分：正向 ping 不再通过。
    for line in &cmds[cut..] {
        exec_ok(&mut sim, fw, line);
    }
    let after = judge_active(&mut sim, LAB10_CHECKS);
    assert_eq!(
        after[2]["passed"], false,
        "规则禁用后策略项必须失败: {after:?}"
    );
    assert_eq!(
        after[4]["passed"], false,
        "规则禁用后 PC1 -> SRV 必须不再通过: {after:?}"
    );
}

#[test]
fn stage6a_dual_channel_and_device_inspection_are_pure() {
    let (mut sim, _pc, fw, _srv) = fw_lab();
    // 取课程前 12 条：两个接口编址 + trust 区域建好并加入 g0/0/0。
    for line in &fw_commands()[..12] {
        exec_ok(&mut sim, fw, line);
    }
    // 回到用户视图：本模拟器的 `display` 只在用户视图的树中（L2 边界，见实施记录）。
    exec_ok(&mut sim, fw, "return");

    // 会话级纯读 inspection：真实设备 + Profile + 模式上下文。
    let token = sim.session_id(fw).unwrap();
    let before = sim.dump();
    let ok = sim
        .verify_device_command_checked(fw, &token, "display firewall zone")
        .unwrap();
    assert_eq!(ok.support.label(), "full-support");
    assert_eq!(sim.dump(), before, "会话级 inspection 必须纯读");
    assert_eq!(
        sim.verify_device_command_checked(fw, "wrong-token", "display firewall zone")
            .unwrap_err(),
        "session id mismatch"
    );

    // 双通道：查询只有设备输出、无模拟器提示。
    let zone: Value = serde_json::from_str(
        &sim.exec_command_result(fw, "display firewall zone")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(zone["status"], "Executed");
    assert!(
        zone["deviceOutput"].as_str().unwrap().contains("trust"),
        "{zone}"
    );
    assert!(zone["simulatorNotices"].as_array().unwrap().is_empty());

    // 非防火墙设备：VRP 路由器执行 `firewall zone` 必须被拒绝且状态不变。
    let router = sim.add_device("Router", "Vrp").unwrap();
    exec_ok(&mut sim, router, "system-view");
    let r: Value = serde_json::from_str(
        &sim.exec_command_result(router, "firewall zone trust")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(r["status"], "InvalidCommand", "{r}");
    assert!(
        !r["deviceOutput"].as_str().unwrap().is_empty(),
        "设备侧错误必须可见: {r}"
    );
    assert_eq!(r["stateChanged"], false, "拒绝路径不得改变状态");

    // ASA 落选血统：IOS 树不再接受 `nameif`。
    let ios_fw = sim.add_device("Firewall", "Ios").unwrap();
    let mut s = 0;
    for line in ["enable", "configure terminal", "interface g0/0"] {
        exec_ok(&mut sim, ios_fw, line);
        s += 1;
    }
    assert_eq!(s, 3);
    let r: Value =
        serde_json::from_str(&sim.exec_command_result(ios_fw, "nameif inside").unwrap()).unwrap();
    assert_eq!(r["status"], "InvalidCommand", "ASA 命令必须已被移除: {r}");
}
