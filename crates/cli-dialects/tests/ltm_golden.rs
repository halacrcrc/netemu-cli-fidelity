//! 阶段 5A：F5 BIG-IP TMOS 17.x / tmsh 独立方言与 LTM 首批闭环。
//!
//! 证据见 `docs/cli-fidelity/stage5a-implementation-record.md` §2（F1–F6）。
//! 无带版本 BIG-IP / TMOS VE 样本：这里只做语法 / 模式 / 结构化状态 / 数据面断言，
//! 不为厂商逐字输出建立黄金。
//!
//! 旧的「所有方言均可解析抽象 ltm 命令」断言已被逐方言接受/拒绝矩阵替换：
//! tmsh 命令只属于 `Dialect::Tmos`，抽象 `ltm …` 在 IOS/VRP/Comware 上必须
//! `InvalidCommand`。原有的池、虚拟服务器、参数、模式、轮询与回程覆盖全部保留并增强。

use cli_dialects::{
    config_render, exec_and_run_result, exec_line_result, inspect_command, prompt,
    scan_simulator_wording, state_fingerprint, trees,
};
use cli_engine::spec::CommandStatus;
use cli_engine::{parse, Mode, Session};
use sim_core::device::{DevId, DeviceKind, Dialect};
use sim_core::engine::Engine;
use sim_core::profile::TMOS_LB_PROFILE;
use sim_core::Net;
use sim_packets::ipv4_from_str;

fn ip(s: &str) -> u32 {
    ipv4_from_str(s).unwrap()
}

fn ok(eng: &mut Engine, s: &mut Session, dev: DevId, line: &str) -> String {
    let r = exec_and_run_result(eng, s, dev, line);
    assert_eq!(
        r.status,
        CommandStatus::Executed,
        "tmsh 命令 {line:?} 应执行，实际 {:?} / {} / {:?}",
        r.status,
        r.device_output,
        r.simulator_notices
    );
    assert!(
        scan_simulator_wording(&r.device_output).is_empty(),
        "设备输出不得混入模拟器措辞: {line}"
    );
    r.device_output
}

fn rejected(eng: &mut Engine, s: &mut Session, dev: DevId, line: &str) -> String {
    let before = state_fingerprint(eng);
    let r = exec_line_result(eng, s, dev, line);
    assert_eq!(
        r.status,
        CommandStatus::InvalidCommand,
        "tmsh 命令 {line:?} 应被拒绝，实际 {:?}",
        r.status
    );
    assert!(!r.state_changed, "被拒绝命令不得改变状态: {line}");
    assert_eq!(state_fingerprint(eng), before, "状态摘要不得变化: {line}");
    r.device_output
}

fn accepts(dialect: Dialect, mode: Mode, line: &str) -> bool {
    matches!(parse(trees(dialect).root_for(mode), line), Ok(Some(_)))
}

/// L2 集成拓扑：PC1 —— LB1(Tmos) —— SRV1 / SRV2。
/// 接口地址/网关属于拓扑预配置（属性面板语义），不属于本实验的 tmsh CLI 任务。
fn ltm_lab() -> (Engine, DevId, DevId, DevId, DevId) {
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let lb = net.add_device("LB1".into(), DeviceKind::LoadBalancer, Dialect::Tmos);
    let s1 = net.add_device("SRV1".into(), DeviceKind::Host, Dialect::Ios);
    let s2 = net.add_device("SRV2".into(), DeviceKind::Host, Dialect::Ios);
    net.add_link(pc, lb).unwrap(); // LB p0
    net.add_link(lb, s1).unwrap(); // LB p1
    net.add_link(lb, s2).unwrap(); // LB p2
    net.set_host(pc, Some(ip("10.0.0.10")), 24, Some(ip("10.0.0.1")));
    net.set_host(s1, Some(ip("10.0.1.10")), 24, Some(ip("10.0.1.1")));
    net.set_host(s2, Some(ip("10.0.2.11")), 24, Some(ip("10.0.2.1")));
    net.set_port_ip(lb, 0, Some(ip("10.0.0.1")), 24);
    net.set_port_ip(lb, 1, Some(ip("10.0.1.1")), 24);
    net.set_port_ip(lb, 2, Some(ip("10.0.2.1")), 24);
    (Engine::new(net), pc, lb, s1, s2)
}

/// 官方 ltm 模块相对写法（F1/F2/F3），调用前会话已在 `(tmos.ltm)#`。
fn configure_in_module(eng: &mut Engine, s: &mut Session, lb: DevId) {
    ok(eng, s, lb, "create pool WEB");
    ok(eng, s, lb, "modify pool WEB members add { 10.0.1.10:80 }");
    ok(eng, s, lb, "modify pool WEB members add { 10.0.2.11:80 }");
    ok(
        eng,
        s,
        lb,
        "create virtual VS destination 10.0.0.100:80 pool WEB",
    );
}

/// 从 root 模块进入 ltm 并配置。
fn configure(eng: &mut Engine, s: &mut Session, lb: DevId) {
    ok(eng, s, lb, "ltm");
    configure_in_module(eng, s, lb);
}

// ---------------------------------------------------------------- 拆树与隔离

#[test]
fn tmos_tree_is_independent_and_abstract_ltm_is_removed_from_three_vendor_trees() {
    // 1) 四棵树互不共用同一个 Trees 对象。
    let all = [Dialect::Ios, Dialect::Vrp, Dialect::Comware, Dialect::Tmos];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert!(
                !std::ptr::eq(trees(*a), trees(*b)),
                "{a:?} 与 {b:?} 必须是独立命令树"
            );
        }
    }
    // 2) tmsh 命令只在 Tmos 可解析；抽象 ltm 在三棵厂商树必须被拒绝。
    let abstract_cmds = [
        (Mode::Config, "ltm pool WEB member 10.0.1.10 80"),
        (Mode::Config, "ltm virtual VS vip 10.0.0.100 80 pool WEB"),
        (Mode::Exec, "show ltm"),
        (Mode::Exec, "display ltm"),
    ];
    for d in [Dialect::Ios, Dialect::Vrp, Dialect::Comware] {
        for (m, line) in &abstract_cmds {
            assert!(
                !accepts(d, *m, line),
                "抽象命令 {line:?} 不得再挂在 {d:?} 树上"
            );
        }
    }
    let tmsh_cmds = [
        (Mode::Exec, "ltm"),
        (Mode::TmosLtm, "create pool WEB"),
        (
            Mode::TmosLtm,
            "create virtual VS destination 10.0.0.100:80 pool WEB",
        ),
        (Mode::TmosLtm, "list pool /Common/WEB"),
        (Mode::Exec, "list ltm pool /Common/WEB"),
    ];
    for (m, line) in &tmsh_cmds {
        assert!(accepts(Dialect::Tmos, *m, line), "Tmos 应可解析 {line:?}");
        for d in [Dialect::Ios, Dialect::Vrp, Dialect::Comware] {
            assert!(
                !accepts(d, *m, line),
                "tmsh 命令 {line:?} 不得在 {d:?} 上可解析"
            );
        }
    }
    // 3) 三棵厂商树里不再出现 ltm 字面量节点。
    for d in [Dialect::Ios, Dialect::Vrp, Dialect::Comware] {
        let ids: Vec<String> = cli_dialects::specs::derived(d)
            .into_iter()
            .map(|x| x.id)
            .collect();
        assert!(
            !ids.iter().any(|id| id.split('/').any(|seg| seg == "ltm")),
            "{d:?} 仍残留 ltm 节点: {ids:?}"
        );
    }
}

#[test]
fn tmos_spec_lint_is_clean_and_layered() {
    let rep = cli_dialects::specs::lint_report(Dialect::Tmos);
    assert!(rep.strict_ok(), "Tmos 规格 lint 必须通过: {rep:?}");
    assert!(rep.duplicates.is_empty(), "{rep:?}");
    assert!(rep.dangling.is_empty(), "{rep:?}");
    assert!(rep.drift.is_empty(), "{rep:?}");
    assert!(rep.strict_missing.is_empty(), "{rep:?}");
    assert_eq!(
        rep.unassociated.len() + cli_dialects::specs::rows(Dialect::Tmos).len(),
        rep.derived_total,
        "未关联节点必须显式列入迁移清单，不得隐藏豁免"
    );
    // 首批闭环命令有官方证据 → 完整支持；未模拟命令不得显示完整支持。
    let good = inspect_command(TMOS_LB_PROFILE, Mode::TmosLtm, "create pool WEB");
    assert_eq!(good.support.label(), "full-support");
    assert_eq!(good.evidence, "DocumentationOnly");
    let bad = inspect_command(TMOS_LB_PROFILE, Mode::Exec, "cd /Other");
    assert_eq!(bad.support.label(), "recognized-only");
}

// ---------------------------------------------------------------- tmsh 层级

#[test]
fn tmsh_module_navigation_prompts_and_exit_quit_hierarchy() {
    let (mut eng, _pc, lb, _s1, _s2) = ltm_lab();
    let mut s = Session::default();
    assert_eq!(prompt(&eng, &s, lb), "(tmos)#");

    // F1：root → ltm 模块。
    ok(&mut eng, &mut s, lb, "ltm");
    assert_eq!(s.mode, Mode::TmosLtm);
    assert_eq!(prompt(&eng, &s, lb), "(tmos.ltm)#");
    // F1：模块内组件名相对当前模块；F4：exit 返回上一级。
    ok(&mut eng, &mut s, lb, "create pool WEB");
    let out = ok(&mut eng, &mut s, lb, "list pool /Common/WEB");
    assert!(out.contains("/Common/WEB"), "{out}");
    ok(&mut eng, &mut s, lb, "exit");
    assert_eq!(s.mode, Mode::Exec);
    assert_eq!(prompt(&eng, &s, lb), "(tmos)#");

    // F1：root → sys 模块；F5：quit 退出整个 tmsh（模拟器回到 root 提示符）。
    ok(&mut eng, &mut s, lb, "sys");
    assert_eq!(s.mode, Mode::TmosSys);
    assert_eq!(prompt(&eng, &s, lb), "(tmos.sys)#");
    let ver = ok(&mut eng, &mut s, lb, "list version");
    assert!(ver.contains("TMOS 17.x"), "{ver}");
    ok(&mut eng, &mut s, lb, "exit");
    assert_eq!(prompt(&eng, &s, lb), "(tmos)#");
    // F6：cd /Common 与 pwd。
    ok(&mut eng, &mut s, lb, "cd /Common");
    assert_eq!(ok(&mut eng, &mut s, lb, "pwd"), "/Common");
    ok(&mut eng, &mut s, lb, "quit");
    assert_eq!(prompt(&eng, &s, lb), "(tmos)#");

    // F4/F5 区别：模块内 exit 只退一级、quit 直接回 root。
    ok(&mut eng, &mut s, lb, "ltm");
    ok(&mut eng, &mut s, lb, "quit");
    assert_eq!(s.mode, Mode::Exec);
    assert_eq!(prompt(&eng, &s, lb), "(tmos)#");
}

#[test]
fn tmos_non_common_partition_is_not_simulated_and_stateless() {
    let (mut eng, _pc, lb, _s1, _s2) = ltm_lab();
    let mut s = Session::default();
    let before = state_fingerprint(&eng);
    let r = exec_line_result(&mut eng, &mut s, lb, "create ltm pool /TenantA/WEB");
    assert_eq!(r.status, CommandStatus::NotSimulated, "{r:?}");
    assert!(r.device_output.is_empty(), "NotSimulated 设备输出必须为空");
    assert!(!r.simulator_notices.is_empty());
    assert!(!r.state_changed);
    assert_eq!(state_fingerprint(&eng), before);
    // Route Domain 语法同样未模拟（但语法上可解析）。
    let r2 = exec_line_result(&mut eng, &mut s, lb, "cd /TenantA%1");
    assert_eq!(r2.status, CommandStatus::NotSimulated, "{r2:?}");
    assert_eq!(state_fingerprint(&eng), before);
}

// ---------------------------------------------------------------- CRUD 与负向

#[test]
fn tmos_pool_virtual_crud_lists_and_negative_paths() {
    let (mut eng, _pc, lb, _s1, _s2) = ltm_lab();
    let mut s = Session::default();
    ok(&mut eng, &mut s, lb, "ltm");

    // 引用不存在的 pool（F3：pool 属性必须指向已有池）。
    rejected(
        &mut eng,
        &mut s,
        lb,
        "create virtual VS destination 10.0.0.100:80 pool MISSING",
    );
    // 地址 / 端口错误。
    rejected(
        &mut eng,
        &mut s,
        lb,
        "modify pool WEB members add { 10.0.1.999:80 }",
    );
    rejected(
        &mut eng,
        &mut s,
        lb,
        "modify pool WEB members add { 10.0.1.10:70000 }",
    );
    rejected(
        &mut eng,
        &mut s,
        lb,
        "modify pool WEB members add { 10.0.1.10 }",
    );
    // 向不存在的池添加成员。
    rejected(
        &mut eng,
        &mut s,
        lb,
        "modify pool MISSING members add { 10.0.1.10:80 }",
    );

    configure_in_module(&mut eng, &mut s, lb);
    // Member 重复必须被拒且状态不变。
    rejected(
        &mut eng,
        &mut s,
        lb,
        "modify pool WEB members add { 10.0.1.10:80 }",
    );
    // 同名 VS 重复创建被拒。
    rejected(
        &mut eng,
        &mut s,
        lb,
        "create virtual VS destination 10.0.0.100:80 pool WEB",
    );

    let pools = ok(&mut eng, &mut s, lb, "list pool /Common/WEB");
    assert!(pools.contains("10.0.1.10:80"), "{pools}");
    assert!(pools.contains("10.0.2.11:80"), "{pools}");
    let vs = ok(&mut eng, &mut s, lb, "list virtual /Common/VS");
    assert!(vs.contains("destination 10.0.0.100:80"), "{vs}");
    assert!(vs.contains("pool /Common/WEB"), "{vs}");

    // F2：被 VS 引用的池不得删除。
    rejected(&mut eng, &mut s, lb, "delete pool WEB");
    // 删除成员 → 查询反映；再删除 VS / 池。
    ok(
        &mut eng,
        &mut s,
        lb,
        "modify pool WEB members delete { 10.0.2.11:80 }",
    );
    let pools = ok(&mut eng, &mut s, lb, "list pool /Common/WEB");
    assert!(!pools.contains("10.0.2.11:80"), "{pools}");
    ok(&mut eng, &mut s, lb, "delete virtual VS");
    ok(&mut eng, &mut s, lb, "delete pool WEB");
    let pools = ok(&mut eng, &mut s, lb, "list pool /Common/WEB");
    assert_eq!(pools, "(none)");
}

// ---------------------------------------------------------------- 数据面

#[test]
fn tmos_ltm_round_robin_distributes_multiple_sessions_to_different_members() {
    let (mut eng, pc, lb, _s1, _s2) = ltm_lab();
    let mut s_lb = Session::default();
    let mut s_pc = Session::default();
    configure(&mut eng, &mut s_lb, lb);

    // 三次以上真实 TCP 会话；断言每次成功且成员集合覆盖两个不同 Member。
    for i in 0..4 {
        let out = ok(&mut eng, &mut s_pc, pc, "tcp 10.0.0.100 80");
        assert!(
            out.contains("连接成功"),
            "第 {} 次 TCP 会话应成功: {out}",
            i + 1
        );
    }
    let sessions = eng.ltm_sessions_of(lb);
    assert!(sessions.len() >= 4, "应产生至少 4 条会话: {sessions:?}");
    let mut members: Vec<u32> = sessions.iter().map(|x| x.member_ip).collect();
    members.sort_unstable();
    members.dedup();
    assert!(
        members.len() >= 2,
        "轮询必须把会话分配到不同 Member: {members:?} / {sessions:?}"
    );
    assert!(members.contains(&ip("10.0.1.10")));
    assert!(members.contains(&ip("10.0.2.11")));

    // show 输出与唯一状态源一致，不得混入模拟器措辞。
    let show = ok(&mut eng, &mut s_lb, lb, "show pool /Common/WEB");
    assert!(show.contains("10.0.1.10"), "{show}");
    assert!(show.contains("10.0.2.11"), "{show}");
    assert!(show.contains("Sessions:"), "{show}");
    assert!(scan_simulator_wording(&show).is_empty());
}

// ---------------------------------------------------------------- 配置渲染

#[test]
fn tmos_config_render_is_tmsh_replayable() {
    let (mut eng, _pc, lb, _s1, _s2) = ltm_lab();
    let mut s = Session::default();
    configure(&mut eng, &mut s, lb);
    let summary = eng.net.summary();
    let lb_summary = summary.devices.iter().find(|d| d.id == lb).unwrap();
    let cfg = config_render::render(lb_summary, Dialect::Tmos);
    assert!(cfg.contains("create ltm pool /Common/WEB"), "{cfg}");
    assert!(cfg.contains("members add { 10.0.1.10:80 }"), "{cfg}");
    assert!(
        cfg.contains("create ltm virtual /Common/VS destination 10.0.0.100:80 pool /Common/WEB"),
        "{cfg}"
    );
    for forbidden in [
        "ltm pool WEB member",
        "ltm virtual VS vip",
        "vlan batch",
        "sysname",
        "hostname",
    ] {
        assert!(!cfg.contains(forbidden), "不得输出 {forbidden}: {cfg}");
    }
    assert_eq!(cfg, config_render::render_tmos(lb_summary));

    // 逐行重执行（root 模块完整路径）还原同一状态。
    let (mut fresh, _pc2, fresh_lb, _s1b, _s2b) = ltm_lab();
    let mut fs = Session::default();
    for line in cfg.lines().map(str::trim).filter(|l| !l.is_empty()) {
        ok(&mut fresh, &mut fs, fresh_lb, line);
    }
    assert_eq!(prompt(&fresh, &fs, fresh_lb), "(tmos)#");
    let a = eng.net.device(lb).unwrap().ltm.clone();
    let b = fresh.net.device(fresh_lb).unwrap().ltm.clone();
    assert_eq!(a.pools.len(), b.pools.len());
    assert_eq!(a.virtuals.len(), b.virtuals.len());
    assert_eq!(a.pools[0].members.len(), b.pools[0].members.len());
    assert_eq!(a.virtuals[0].vip, b.virtuals[0].vip);
    assert_eq!(a.virtuals[0].port, b.virtuals[0].port);
}

// ---------------------------------------------------------------- 课程原文（Rust 层）

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

/// 与 WASM 契约测试、浏览器验收脚本共用同一解析规则。
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

fn f5_lab_cli() -> Vec<String> {
    let cn = lab_tasks(LABS_CN, "export const lab19: LabDef = {");
    let en = lab_tasks(LABS_EN, "'f5-ltm': {");
    assert_eq!(cn.len(), 5, "中文 F5 实验应有 5 步任务: {cn:?}");
    assert_eq!(en.len(), 5, "英文 F5 实验应有 5 步任务: {en:?}");
    let cn_cli: Vec<String> = cn[..3].iter().flat_map(|t| task_cli(t)).collect();
    let en_cli: Vec<String> = en[..3].iter().flat_map(|t| task_cli(t)).collect();
    assert_eq!(cn_cli, en_cli, "中英文课程原文必须给出同一条 tmsh 序列");
    cn_cli
}

#[test]
fn tmos_f5_lab_course_original_sequence_runs_top_to_bottom() {
    let cli = f5_lab_cli();
    assert_eq!(cli.first().map(String::as_str), Some("ltm"));
    assert_eq!(cli.last().map(String::as_str), Some("quit"));
    assert_eq!(cli.len(), 12, "前三步共 12 条命令: {cli:?}");

    let (mut eng, _pc, lb, _s1, _s2) = ltm_lab();
    let mut s = Session::default();
    let mut prompts = Vec::new();
    for line in &cli {
        ok(&mut eng, &mut s, lb, line);
        prompts.push(prompt(&eng, &s, lb));
    }
    assert_eq!(prompts.last().map(String::as_str), Some("(tmos)#"));
    assert!(prompts.iter().any(|p| p == "(tmos.ltm)#"), "{prompts:?}");
    assert!(prompts.iter().any(|p| p == "(tmos.sys)#"), "{prompts:?}");

    let d = eng.net.device(lb).unwrap();
    assert_eq!(d.ltm.pools.len(), 1);
    assert_eq!(d.ltm.pools[0].members.len(), 2);
    assert_eq!(d.ltm.virtuals.len(), 1);
    assert_eq!(d.ltm.virtuals[0].vip, ip("10.0.0.100"));

    // 课程序列执行完后，纯读 ltmVirtual 检查与 Member 分配主动检查均可成立。
    let (mut eng2, pc, _lb, _s1, _s2) = ltm_lab();
    let mut s2 = Session::default();
    for line in &cli {
        ok(&mut eng2, &mut s2, _lb, line);
    }
    let mut s_pc = Session::default();
    for _ in 0..3 {
        let out = ok(&mut eng2, &mut s_pc, pc, "tcp 10.0.0.100 80");
        assert!(out.contains("连接成功"), "{out}");
    }
    let sessions = eng2.ltm_sessions_of(_lb);
    let mut members: Vec<u32> = sessions.iter().map(|x| x.member_ip).collect();
    members.sort_unstable();
    members.dedup();
    assert!(
        members.len() >= 2,
        "课程原文执行后应能轮询分配: {members:?}"
    );
}
