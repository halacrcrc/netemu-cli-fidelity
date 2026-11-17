//! C6b 工作项 C：Lab 6（`mgmt-ssh`）课程原文的三层验收——Rust 层。
//!
//! **解析规则（决策 D2，与 WASM 层同源）**
//!
//! 任务原文是 `labs.ts` / `labs-en.ts` 的 `tasks: string[]`，不是 Markdown 代码块，
//! 因此不能用代码块锚点，改为固定文法：
//!
//! 1. 先去括号附注（`（...）` / `(...)`），附注是教学说明，不是命令；
//! 2. 标签 = 第一个箭头之前的 `X：` / `X: `：`R1` 表示在 R1 上执行，
//!    `PC1 终端` / `PC1 terminal` 归一化为 `PC1` 并标记为**交互终端**；
//! 3. 正文按 `→` / `->` 切成单条步骤（一个箭头一条，不得合并）；
//! 4. 每段可按 `——` / `—` / ` -- ` 再切出「期望结果」；
//! 5. 步骤分三类：
//!    - `用户名 X、口令 Y` / `username X, password Y` → 认证输入（不是 CLI）；
//!    - `看到 R1# ...` / `seeing R1# ...` → 提示符断言（登录成功的判据）；
//!    - 其余 → CLI 命令；期望结果含「拒绝 / refused」时断言为连接被拒。
//!
//! 中英文必须解析出**完全相同**的步骤序列（标签归一化后比较），因此任何一侧
//! 改文案都会让测试失败，而不是悄悄少跑几条命令。解析不出步骤即失败，
//! 不接受手写命令清单。

use cli_dialects::{exec_and_run, prompt};
use cli_engine::Session;
use sim_core::device::{DeviceKind, Dialect};
use sim_core::engine::Engine;
use sim_core::Net;
use sim_packets::ipv4_from_str;

const LABS_CN: &str = include_str!("../../../web/src/lib/labs.ts");
const LABS_EN: &str = include_str!("../../../web/src/content/labs-en.ts");
const LESSON47: &str = include_str!("../../../web/src/content/lessons/47-ssh-management.md");
const CN_MARKER: &str = "export const lab6: LabDef = {";
const EN_MARKER: &str = "'mgmt-ssh': {";

fn ip(s: &str) -> u32 {
    ipv4_from_str(s).unwrap()
}

// ---------------------------------------------------------------- 原文解析

/// 去掉括号附注（中英文括号都算），保留其余文本。
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

/// 取出某个 LabDef 的 `tasks: [...]` 里的单引号字符串。
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expect {
    /// 期望连接被拒绝（VTY 传输限制）。
    Refused,
    /// 期望输出包含指定文本。
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
    /// 执行设备（已归一化：`PC1 终端` → `PC1`）。
    dev: String,
    /// 是否为「PC 终端」交互步骤（需要提示符断言）。
    interactive: bool,
    steps: Vec<Step>,
}

/// 标签归一化：`PC1 终端` / `PC1 terminal` → (`PC1`, true)。
fn normalize_label(label: &str) -> (String, bool) {
    let l = label.trim();
    for suffix in ["终端", "terminal", "Terminal"] {
        if let Some(name) = l.strip_suffix(suffix) {
            return (name.trim().to_string(), true);
        }
    }
    (l.to_string(), false)
}

/// 认证段：`用户名 admin、口令 Cisco@123` / `username admin, password Cisco@123`。
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
    let after = after.trim();
    let pass = after.strip_prefix(pass_kw)?.trim();
    Some(Step::Credentials {
        user: user.trim().to_string(),
        pass: pass.trim().to_string(),
    })
}

/// 提示符断言段：`看到 R1# 说明登录成功` / `seeing R1# means ...`。
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

/// 单段 → 步骤。
fn parse_segment(seg: &str) -> Option<Step> {
    let seg = seg.trim();
    if seg.is_empty() {
        return None;
    }
    // 期望结果切分：`命令 —— 应提示...` / `command — it should ...`
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

/// 解析一条任务原文。
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
    let steps: Vec<Step> = body
        .replace('→', "->")
        .split("->")
        .filter_map(parse_segment)
        .collect();
    Task {
        dev,
        interactive,
        steps,
    }
}

fn lab6_course() -> Vec<Task> {
    let cn = lab_tasks(LABS_CN, CN_MARKER);
    let en = lab_tasks(LABS_EN, EN_MARKER);
    assert!(!cn.is_empty() && !en.is_empty(), "lab6 任务不得为空");
    assert_eq!(cn.len(), en.len(), "中英文 lab6 步数必须一致");
    let cn_parsed: Vec<Task> = cn.iter().map(|t| parse_task(t)).collect();
    let en_parsed: Vec<Task> = en.iter().map(|t| parse_task(t)).collect();
    assert_eq!(
        cn_parsed, en_parsed,
        "中英文 lab6 必须解析出同一条可执行序列（标签归一化后比较）"
    );
    for (i, t) in cn_parsed.iter().enumerate() {
        assert!(!t.steps.is_empty(), "第 {i} 条任务解析不出步骤: {}", cn[i]);
        assert!(!t.dev.is_empty(), "第 {i} 条任务缺少执行设备标签: {}", cn[i]);
    }
    cn_parsed
}

// ---------------------------------------------------------------- 执行

/// 与 `labs.ts` 的 `lab6.build` 完全一致的拓扑：PC1 —— R1（R1 侧只有 g0/0）。
fn lab6_topo() -> (Engine, u32, u32) {
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Ios);
    net.add_link(pc, r1).unwrap();
    net.set_host(pc, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    (Engine::new(net), pc, r1)
}

#[test]
fn c6b_lab6_course_original_sequence_runs_on_real_engine() {
    let course = lab6_course();
    assert_eq!(course.len(), 6, "lab6 应为 6 条任务: {course:?}");

    let (mut eng, pc, r1) = lab6_topo();
    let mut r1_sess = Session::default();
    let mut pc_sess = Session::default();
    let mut prompts: Vec<String> = vec![];
    let mut remote_show = String::new();
    let mut telnet_out = String::new();

    for task in &course {
        let dev = match task.dev.as_str() {
            "R1" => r1,
            "PC1" => pc,
            other => panic!("未知任务标签 {other}"),
        };
        let sess = if dev == r1 {
            &mut r1_sess
        } else {
            &mut pc_sess
        };
        for step in &task.steps {
            match step {
                Step::Cli { cmd, expect } => {
                    let out = exec_and_run(&mut eng, sess, dev, cmd);
                    assert!(
                        !out.contains("% Invalid")
                            && !out.contains("% Unrecognized")
                            && !out.contains("Error:")
                            && !out.contains("Incomplete"),
                        "课程原文命令 {cmd:?} 执行失败: {out}"
                    );
                    match expect {
                        Some(Expect::Refused) => assert!(
                            out.contains("Connection refused") || out.contains("refused"),
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
                    prompts.push(prompt(&eng, sess, dev));
                }
                Step::Credentials { user, pass } => {
                    assert_eq!(prompt(&eng, sess, dev), "Username: ");
                    let _ = exec_and_run(&mut eng, sess, dev, user);
                    assert_eq!(prompt(&eng, sess, dev), "Password: ");
                    let out = exec_and_run(&mut eng, sess, dev, pass);
                    assert_eq!(out, "", "口令正确时不应有额外输出: {out:?}");
                    prompts.push(prompt(&eng, sess, dev));
                }
                Step::ExpectPrompt(want) => {
                    assert_eq!(&prompt(&eng, sess, dev), want, "登录成功判据不成立");
                }
            }
        }
    }

    // 提示符链（逐条命令执行后的真实提示符）：
    //   R1# → R1(config)# → R1(config-if)#×3 → R1(config)# → R1(config)# →
    //   R1(config-line)#×3 → R1(config)# → Username: → R1# → R1# → PC1> → PC1>
    assert_eq!(
        prompts,
        vec![
            "R1#",             // enable
            "R1(config)#",     // configure terminal
            "R1(config-if)#",  // interface g0/0
            "R1(config-if)#",  // ip address ...
            "R1(config-if)#",  // no shutdown
            "R1(config)#",     // exit
            "R1(config)#",     // username ...
            "R1(config-line)#", // line vty 0 4
            "R1(config-line)#", // login local
            "R1(config-line)#", // transport input ssh
            "R1(config)#",     // exit
            "Username: ",      // ssh 192.168.1.1（用户名提示）
            "R1#",             // 口令通过 → 远端特权提示符
            "R1#",             // show ip interface brief（远端执行）
            "PC1>",            // exit 断开
            "PC1>",            // telnet 被拒
        ],
        "提示符链必须与课程原文一致"
    );

    // 远端 `show ip interface brief` 的输出必须来自 R1。
    assert!(
        remote_show.contains("GigabitEthernet0/0") && remote_show.contains("192.168.1.1"),
        "远端 show 输出应来自 R1: {remote_show:?}"
    );
    assert!(
        telnet_out.contains("refused"),
        "telnet 应提示连接被拒绝: {telnet_out:?}"
    );
    assert!(pc_sess.login.is_none() && pc_sess.remote.is_none());
    assert!(r1_sess.remote.is_none(), "R1 本地会话不应有远端上下文");
}

/// 解析器本身的门禁：没有设备标签的散文必须被识别为「无法路由到设备」，
/// 而不是被当成一条命令偷偷执行（这正是 C6b 之前课程原文无人执行的原因）。
#[test]
fn c6b_lab6_parser_rejects_prose_without_device_label() {
    let t = parse_task("登录后执行 show ip interface brief 查看 R1 接口，再 exit 断开");
    assert!(t.dev.is_empty(), "无标签任务不得凭空获得执行设备: {t:?}");
    // 课程驱动只接受明确标签；空标签会在执行前被拒绝（见 `c6b_lab6_course_...` 的 match）。
    assert!(!["R1", "PC1"].contains(&t.dev.as_str()));

    // 规范写法必须解析出设备 + 单条命令（本包已按此规则归一化第 5 条任务）。
    let t = parse_task("PC1 终端：show ip interface brief → exit（登录后远端执行）");
    assert_eq!(t.dev, "PC1");
    assert!(t.interactive);
    assert_eq!(t.steps.len(), 2, "{t:?}");
    assert_eq!(
        t.steps[0],
        Step::Cli {
            cmd: "show ip interface brief".into(),
            expect: None
        }
    );

    // 真实课程里每条任务都必须带可路由标签，并且标签唯一可枚举。
    for task in lab6_course() {
        assert!(
            task.dev == "R1" || task.dev == "PC1",
            "课程任务标签必须可路由: {task:?}"
        );
    }
}

/// 讲义 47 的边界处置（D3 抓包文案 / D4 `show ip ssh`）必须被文本门禁锁住，
/// 否则下一轮很容易把「抓包对比 Telnet 与 SSH 报文」这类不可支撑的说法写回去。
#[test]
fn c6b_lesson47_states_capture_and_show_ssh_boundaries() {
    // D3：抓包只有链路级仿真帧，登录是 CLI 层抽象 → 不得宣称抓包对比报文。
    assert!(
        !LESSON47.contains("抓包对比"),
        "讲义不得声称抓包对比 Telnet 与 SSH 的报文内容"
    );
    assert!(
        LESSON47.contains("不模拟 TCP 握手") && LESSON47.contains("看不到"),
        "必须如实声明抓包看不到登录报文: {LESSON47}"
    );
    assert!(
        LESSON47.contains("ping") && LESSON47.contains("SYN"),
        "必须说明抓包面板真实可支撑的内容（ICMP / TCP SYN）"
    );
    // D4：两条查询命令保持未实现，并标注为真机独占。
    assert!(
        LESSON47.contains("show ip ssh") && LESSON47.contains("display ssh server status"),
        "排障第 1 步必须保留真机命令名"
    );
    assert!(
        LESSON47.contains("没有实现"),
        "必须声明模拟器未实现这两条查询"
    );
    assert!(
        LESSON47.contains("真实设备"),
        "必须把第 1 步标注为真机排障步骤"
    );
    // 接地教学：登录受数据面约束 + self 例外不回退。
    assert!(
        LESSON47.contains("登录也是数据面流量"),
        "讲义必须把登录与数据面可达性绑在一起"
    );
    assert!(
        LESSON47.contains("self"),
        "讲义必须保留 self 区域例外说明"
    );
    // 讲义与实验六的绑定关系不变。
    assert!(LESSON47.contains("lab: mgmt-ssh"), "讲义必须仍绑定实验六");
}
