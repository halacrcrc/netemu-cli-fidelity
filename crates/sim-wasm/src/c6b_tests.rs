//! C6b 工作项 C：Lab 6（`mgmt-ssh`）课程原文在真实 WASM `Sim` 上的验收。
//!
//! 与 `crates/cli-dialects/tests/c6b_courses.rs` 共用同一套「课程原文解析」规则
//! （决策 D2，逐条规则见该文件头注释），不维护手写命令清单：
//! 标签归一化 → 箭头拆单条 → 认证输入 / 提示符断言 / CLI 三类步骤。
//!
//! 这里覆盖三件事：
//! 1. 中英 tasks 解析出同一条序列，并在真实 `Sim` 上逐条执行 0 失败；
//! 2. 提示符链 `PC1>` → `Username:` → `Password:` → `R1#` → 远端 show → `exit` → `PC1>`
//!    与 telnet 被拒；
//! 3. 主动判分 `run_lab_checks`（4 项）全绿；**破坏配置后重判分反映变化**
//!    （改 VTY transport、以及对端接口 shutdown 两种破坏）。

use super::*;
use serde_json::Value;

const LABS_CN: &str = include_str!("../../../web/src/lib/labs.ts");
const LABS_EN: &str = include_str!("../../../web/src/content/labs-en.ts");
const CN_MARKER: &str = "export const lab6: LabDef = {";
const EN_MARKER: &str = "'mgmt-ssh': {";

/// lab6 的检查项定义，与 `labs.ts` 的 `lab6.checks` 同形（kind 与参数由
/// `lab6_checks_block` 的文本门禁保证没有漂移）。
const LAB6_CHECKS: &str = r#"[
  {"label":"R1 存在本地用户 admin","kind":{"type":"mgmtUser","dev":"R1","name":"admin"}},
  {"label":"R1 VTY 仅允许 SSH 接入","kind":{"type":"vtyTransport","dev":"R1","transport":"ssh"}},
  {"label":"PC1 用 SSH 登录 R1 成功","kind":{"type":"login","from":"PC1","to":"192.168.1.1","proto":"ssh","user":"admin","pass":"Cisco@123"}},
  {"label":"PC1 的 Telnet 登录被拒绝","kind":{"type":"login","from":"PC1","to":"192.168.1.1","proto":"telnet","user":"admin","pass":"Cisco@123","expect":false}}
]"#;

// ---------------------------------------------------------------- 原文解析（与 Rust 层同源）

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

fn block_after(src: &str, marker: &str, open: &str) -> String {
    let a = src.find(marker).unwrap_or_else(|| panic!("缺少 {marker}"));
    let rest = &src[a..];
    let b = rest.find(open).unwrap_or_else(|| panic!("缺少 {open}"));
    let body = &rest[b + open.len()..];
    let e = body
        .find("\n    ],")
        .or_else(|| body.find("\n  ],"))
        .unwrap_or_else(|| panic!("缺少 {open} 结束"));
    body[..e].to_string()
}

fn lab_tasks(src: &str, marker: &str) -> Vec<String> {
    block_after(src, marker, "tasks: [")
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let s = l.find('\'')?;
            let e = l[s + 1..].find('\'')? + s + 1;
            Some(l[s + 1..e].to_string())
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expect {
    Refused,
    Contains(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    Cli { cmd: String, expect: Option<Expect> },
    Credentials { user: String, pass: String },
    ExpectPrompt(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Task {
    dev: String,
    interactive: bool,
    steps: Vec<Step>,
}

fn normalize_label(label: &str) -> (String, bool) {
    let l = label.trim();
    for suffix in ["终端", "terminal", "Terminal"] {
        if let Some(name) = l.strip_suffix(suffix) {
            return (name.trim().to_string(), true);
        }
    }
    (l.to_string(), false)
}

fn parse_credentials(seg: &str) -> Option<Step> {
    let seg = seg.trim();
    let (user_kw, pass_kw, sep) = if seg.starts_with("用户名") {
        ("用户名", "口令", '、')
    } else if seg.to_ascii_lowercase().starts_with("username") {
        ("username", "password", ',')
    } else {
        return None;
    };
    let rest = seg[user_kw.len()..].trim();
    let (user, after) = rest.split_once(sep)?;
    let pass = after.trim().strip_prefix(pass_kw)?.trim();
    Some(Step::Credentials {
        user: user.trim().to_string(),
        pass: pass.trim().to_string(),
    })
}

fn parse_prompt_assert(seg: &str) -> Option<Step> {
    let seg = seg.trim();
    let rest = seg
        .strip_prefix("看到")
        .or_else(|| seg.strip_prefix("seeing"))?
        .trim();
    let token = rest.split_whitespace().next()?;
    let prompt = token.trim_end_matches(['.', ',', '。', '，']);
    if prompt.ends_with('#') || prompt.ends_with('>') {
        Some(Step::ExpectPrompt(prompt.to_string()))
    } else {
        None
    }
}

fn parse_segment(seg: &str) -> Option<Step> {
    let seg = seg.trim();
    if seg.is_empty() {
        return None;
    }
    let (head, tail) = if let Some((a, b)) = seg.split_once("——") {
        (a, Some(b))
    } else if let Some((a, b)) = seg.split_once('—') {
        (a, Some(b))
    } else if let Some((a, b)) = seg.split_once(" -- ") {
        (a, Some(b))
    } else {
        (seg, None)
    };
    let head = head.trim();
    if let Some(step) = parse_credentials(head) {
        return Some(step);
    }
    if let Some(step) = parse_prompt_assert(head) {
        return Some(step);
    }
    if head.is_empty() {
        return None;
    }
    let expect = tail.map(|t| {
        let t = t.trim();
        if t.contains("拒绝") || t.contains("refused") {
            Expect::Refused
        } else {
            Expect::Contains(t.to_string())
        }
    });
    Some(Step::Cli {
        cmd: head.to_string(),
        expect,
    })
}

fn parse_task(task: &str) -> Task {
    let clean = strip_task_notes(task);
    let arrow = clean
        .find("->")
        .into_iter()
        .chain(clean.find('→'))
        .min()
        .unwrap_or(clean.len());
    let head = &clean[..arrow];
    let (label, body) = if let Some(p) = head.find('：') {
        (&head[..p], clean[p + '：'.len_utf8()..].to_string())
    } else if let Some(p) = head.find(": ") {
        (&head[..p], clean[p + 2..].to_string())
    } else {
        ("", clean.clone())
    };
    let (dev, interactive) = normalize_label(label);
    Task {
        dev,
        interactive,
        steps: body
            .replace('→', "->")
            .split("->")
            .filter_map(parse_segment)
            .collect(),
    }
}

fn lab6_course() -> Vec<Task> {
    let cn = lab_tasks(LABS_CN, CN_MARKER);
    let en = lab_tasks(LABS_EN, EN_MARKER);
    assert_eq!(cn.len(), en.len(), "中英文 lab6 步数必须一致");
    let cn_parsed: Vec<Task> = cn.iter().map(|t| parse_task(t)).collect();
    let en_parsed: Vec<Task> = en.iter().map(|t| parse_task(t)).collect();
    assert_eq!(cn_parsed, en_parsed, "中英文 lab6 必须是同一条序列");
    assert!(!cn_parsed.is_empty());
    for t in &cn_parsed {
        assert!(!t.steps.is_empty(), "任务解析不出步骤: {t:?}");
        assert!(t.dev == "R1" || t.dev == "PC1", "任务标签不可路由: {t:?}");
    }
    cn_parsed
}

/// `checks: [...]` 原文块（只存在于 CN 源文件），用于确认判分定义与课程文件同形。
fn lab6_checks_block(src: &str, marker: &str) -> String {
    block_after(src, marker, "checks: [")
}

// ---------------------------------------------------------------- 真实 Sim

fn lab6_sim() -> (Sim, u32, u32) {
    let mut sim = Sim::new();
    let pc = sim.add_device("Host", "Ios").unwrap();
    let r1 = sim.add_device("Router", "Ios").unwrap();
    sim.rename_device(pc, "PC1").unwrap();
    sim.rename_device(r1, "R1").unwrap();
    sim.add_link(pc, r1).unwrap();
    sim.set_host_config(pc, "192.168.1.10", 24, "192.168.1.1")
        .unwrap();
    (sim, pc, r1)
}

fn exec(sim: &mut Sim, dev: u32, line: &str) -> Value {
    serde_json::from_str(&sim.exec_command_result(dev, line).unwrap()).unwrap()
}

fn exec_ok(sim: &mut Sim, dev: u32, line: &str) -> String {
    let v = exec(sim, dev, line);
    assert_eq!(v["status"], "Executed", "{line}: {v}");
    assert!(
        v["simulatorNotices"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(true),
        "{line} 不应产生模拟器提示: {v}"
    );
    v["deviceOutput"].as_str().unwrap_or("").to_string()
}

fn results(json: &str) -> Vec<Value> {
    serde_json::from_str(json).unwrap_or_else(|e| panic!("判分结果不是数组: {e} / {json}"))
}

fn judge(sim: &mut Sim) -> Vec<Value> {
    results(&sim.run_lab_checks(LAB6_CHECKS).unwrap())
}

/// 把设备会话退回特权模式（只用 `exit`，与真实 IOS 逐级退出一致）。
fn to_priv(sim: &mut Sim, dev: u32, want: &str) {
    for _ in 0..4 {
        if sim.prompt(dev).unwrap() == want {
            return;
        }
        exec_ok(sim, dev, "exit");
    }
    assert_eq!(sim.prompt(dev).unwrap(), want);
}

/// 按课程原文执行完整序列，返回（所有提示符, 远端 show 输出, telnet 输出）。
fn run_course(sim: &mut Sim, pc: u32, r1: u32) -> (Vec<String>, String, String) {
    let mut prompts: Vec<String> = vec![];
    let mut remote_show = String::new();
    let mut telnet_out = String::new();
    for task in lab6_course() {
        let dev = match task.dev.as_str() {
            "R1" => r1,
            "PC1" => pc,
            other => panic!("未知任务标签 {other}"),
        };
        for step in &task.steps {
            match step {
                Step::Cli { cmd, expect } => {
                    let out = exec_ok(sim, dev, cmd);
                    match expect {
                        Some(Expect::Refused) => assert!(
                            out.contains("refused"),
                            "telnet 应被 VTY 传输策略拒绝: {out:?}"
                        ),
                        Some(Expect::Contains(t)) => assert!(out.contains(t.as_str()), "{out:?}"),
                        None => {}
                    }
                    if cmd == "show ip interface brief" {
                        remote_show = out.clone();
                    }
                    if cmd == "telnet 192.168.1.1" {
                        telnet_out = out.clone();
                    }
                    prompts.push(sim.prompt(dev).unwrap());
                }
                Step::Credentials { user, pass } => {
                    assert_eq!(sim.prompt(dev).unwrap(), "Username: ");
                    exec_ok(sim, dev, user);
                    assert_eq!(sim.prompt(dev).unwrap(), "Password: ");
                    assert_eq!(exec_ok(sim, dev, pass), "", "口令正确时不应有额外输出");
                    prompts.push(sim.prompt(dev).unwrap());
                }
                Step::ExpectPrompt(want) => {
                    assert_eq!(&sim.prompt(dev).unwrap(), want, "登录成功判据不成立");
                }
            }
        }
    }
    (prompts, remote_show, telnet_out)
}

#[test]
fn c6b_lab6_course_original_sequence_runs_on_real_wasm_sim() {
    let course = lab6_course();
    assert_eq!(course.len(), 6, "lab6 应为 6 条任务: {course:?}");

    let (mut sim, pc, r1) = lab6_sim();
    let (prompts, remote_show, telnet_out) = run_course(&mut sim, pc, r1);

    assert_eq!(
        prompts,
        vec![
            "R1#",
            "R1(config)#",
            "R1(config-if)#",
            "R1(config-if)#",
            "R1(config-if)#",
            "R1(config)#",
            "R1(config)#",
            "R1(config-line)#",
            "R1(config-line)#",
            "R1(config-line)#",
            "R1(config)#",
            "Username: ",
            "R1#",
            "R1#",
            "PC1>",
            "PC1>",
        ],
        "提示符链必须与课程原文一致"
    );
    assert!(
        remote_show.contains("GigabitEthernet0/0") && remote_show.contains("192.168.1.1"),
        "远端 show 输出应来自 R1: {remote_show:?}"
    );
    assert!(telnet_out.contains("refused"), "{telnet_out:?}");
    assert_eq!(sim.prompt(pc).unwrap(), "PC1>");
}

#[test]
fn c6b_lab6_active_scoring_is_four_of_four_and_reacts_to_breakage() {
    // 判分定义与课程文件同形：kind 顺序与关键参数逐一核对（CN 承载 kind，EN 承载译文标签）。
    let cn_block = lab6_checks_block(LABS_CN, CN_MARKER);
    let kinds: Vec<&str> = cn_block
        .match_indices("type: '")
        .map(|(i, _)| {
            let rest = &cn_block[i + "type: '".len()..];
            &rest[..rest.find('\'').unwrap()]
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["mgmtUser", "vtyTransport", "login", "login"],
        "lab6 判分项必须覆盖本地用户 / VTY 传输 / 登录正 / 登录负: {cn_block}"
    );
    assert!(
        cn_block.contains("dev: 'R1'") && cn_block.contains("name: 'admin'"),
        "{cn_block}"
    );
    assert!(cn_block.contains("transport: 'ssh'"), "{cn_block}");
    assert!(
        cn_block.contains("proto: 'ssh'") && cn_block.contains("proto: 'telnet'"),
        "{cn_block}"
    );
    assert!(
        cn_block.contains("expect: false"),
        "telnet 负向判分必须显式 expect: false: {cn_block}"
    );
    // 英文只提供 checkLabels：数量必须与 CN 的 4 个判分项一一对应。
    let en_labels = block_after(LABS_EN, EN_MARKER, "checkLabels: [");
    let en_count = en_labels.lines().filter(|l| l.trim().starts_with('\'')).count();
    assert_eq!(en_count, 4, "英文 checkLabels 必须与 4 个判分项对应: {en_labels}");

    let (mut sim, pc, r1) = lab6_sim();
    let _ = run_course(&mut sim, pc, r1);

    // 课程末态：4 项全绿。
    let full = judge(&mut sim);
    assert_eq!(full.len(), 4, "{full:?}");
    for (i, r) in full.iter().enumerate() {
        assert_eq!(r["passed"], true, "第 {i} 项应通过: {r}");
    }

    // 破坏 1：VTY 只放行 Telnet → 传输项、SSH 登录项、Telnet 拒绝项全部翻转。
    to_priv(&mut sim, r1, "R1#");
    exec_ok(&mut sim, r1, "configure terminal");
    exec_ok(&mut sim, r1, "line vty 0 4");
    exec_ok(&mut sim, r1, "transport input telnet");
    let broken = judge(&mut sim);
    assert_eq!(
        broken[0]["passed"], true,
        "本地用户项不应受传输策略影响: {broken:?}"
    );
    assert_eq!(
        broken[1]["passed"], false,
        "VTY 仅 SSH 项必须失败: {broken:?}"
    );
    assert_eq!(broken[2]["passed"], false, "SSH 登录应被拒绝: {broken:?}");
    assert_eq!(
        broken[3]["passed"], false,
        "Telnet 现在能登录，负向项必须失败: {broken:?}"
    );

    // 恢复：立即重新全绿（判分不缓存旧结论）。
    exec_ok(&mut sim, r1, "transport input ssh");
    to_priv(&mut sim, r1, "R1#");
    let restored = judge(&mut sim);
    assert!(
        restored.iter().all(|r| r["passed"] == true),
        "{restored:?}"
    );

    // 破坏 2：对端接口 shutdown（数据面接地）→ SSH 登录项必须失败，其余保持。
    exec_ok(&mut sim, r1, "configure terminal");
    exec_ok(&mut sim, r1, "interface g0/0");
    exec_ok(&mut sim, r1, "shutdown");
    let down = judge(&mut sim);
    assert_eq!(down[0]["passed"], true, "{down:?}");
    assert_eq!(down[1]["passed"], true, "VTY 配置未变: {down:?}");
    assert_eq!(
        down[2]["passed"], false,
        "接口 shutdown 后 SSH 登录必须失败（数据面接地）: {down:?}"
    );
    assert_eq!(down[3]["passed"], true, "Telnet 仍被 VTY 拒绝: {down:?}");

    exec_ok(&mut sim, r1, "no shutdown");
    to_priv(&mut sim, r1, "R1#");
    let back = judge(&mut sim);
    assert!(
        back.iter().all(|r| r["passed"] == true),
        "恢复后应重新全绿: {back:?}"
    );
}

/// 课程原文里的交互登录在 WASM 契约上走同一套 RemoteSession：远端命令落到 R1，
/// `exit` 后回到 PC1，且设备输出通道不含模拟器提示。
#[test]
fn c6b_lab6_remote_session_channel_is_clean() {
    let (mut sim, pc, r1) = lab6_sim();
    let _ = run_course(&mut sim, pc, r1);

    // 重新登录一次，逐条检查通道分离。
    let out = exec(&mut sim, pc, "ssh 192.168.1.1");
    assert_eq!(out["status"], "Executed");
    assert!(
        out["deviceOutput"]
            .as_str()
            .unwrap_or("")
            .contains("Trying 192.168.1.1"),
        "{out}"
    );
    assert_eq!(sim.prompt(pc).unwrap(), "Username: ");
    let _ = exec_ok(&mut sim, pc, "admin");
    let _ = exec_ok(&mut sim, pc, "Cisco@123");
    assert_eq!(sim.prompt(pc).unwrap(), "R1#");

    let show = exec(&mut sim, pc, "show ip interface brief");
    assert_eq!(show["status"], "Executed");
    assert!(
        show["deviceOutput"]
            .as_str()
            .unwrap_or("")
            .contains("GigabitEthernet0/0"),
        "{show}"
    );
    assert!(
        show["simulatorNotices"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(true),
        "远端查询不得产生模拟器提示: {show}"
    );

    let bye = exec(&mut sim, pc, "exit");
    assert!(
        bye["deviceOutput"]
            .as_str()
            .unwrap_or("")
            .contains("Connection closed"),
        "{bye}"
    );
    assert_eq!(sim.prompt(pc).unwrap(), "PC1>");
    // R1 自身的本地会话仍停在课程最后一步的配置模式，不受 PC1 远端会话影响。
    assert_eq!(sim.prompt(r1).unwrap(), "R1(config)#");
}
