//! 阶段 3B：真实课程文件与真实 Cisco IOS XE 17.x ZBF 对照的回归门禁。
//!
//! 所有课程数据都通过 `include_str!` 直接读取 Web 源文件，不维护第二份表格。
//! 这里验证「真实 IOS ZBF 语法仅识别 / 未模拟」「旧简化流程是教学抽象」
//! 以及四态、双通道、Comware 阶段 4 门禁与规格 lint 不回归。

use cli_dialects::{exec_line_result, inspect_command, specs, state_fingerprint, trees};
use cli_engine::spec::{Effect, SemanticSupport};
use cli_engine::{parse, Mode, Session};
use sim_core::device::{DeviceKind, Dialect};
use sim_core::engine::Engine;
use sim_core::profile::{
    EvidenceLevel, COMWARE_ROUTER_PROFILE, IOS_ROUTER_PROFILE, VRP_ROUTER_PROFILE,
};
use sim_core::Net;

const LABS_CN: &str = include_str!("../../../web/src/lib/labs.ts");
const LABS_EN: &str = include_str!("../../../web/src/content/labs-en.ts");
const CMDMAP: &str = include_str!("../../../web/src/content/cmdmap.ts");
const LESSON_CN: &str = include_str!("../../../web/src/content/lessons/08-firewall-zone.md");
const LESSON_EN: &str = include_str!("../../../web/src/content/lessons-en/08-firewall-zone.md");

fn section<'a>(src: &'a str, start: &str, end: &str) -> &'a str {
    let a = src
        .find(start)
        .unwrap_or_else(|| panic!("找不到课程段落起点 {start}"));
    let b = src[a..].find(end).map(|i| a + i).unwrap_or(src.len());
    &src[a..b]
}

fn campus_cn() -> &'static str {
    section(LABS_CN, "id: 'campus-capstone'", "// ---------- 实验十")
}

fn campus_en() -> &'static str {
    section(LABS_EN, "'campus-capstone': {", "'firewall-zone': {")
}

fn enterprise_cn() -> &'static str {
    section(LABS_CN, "id: 'enterprise'", "// ---------- 实验五")
}

fn enterprise_en() -> &'static str {
    section(LABS_EN, "enterprise: {", "'lag-redundancy': {")
}

/// security 段（真实三厂商对照 + 明确隔离的教学抽象行）。
fn security_rows() -> &'static str {
    section(CMDMAP, "// 防火墙 / IOS XE 17.x ZBF", "// IPsec / SSL VPN")
}

#[test]
fn stage3b_campus_capstone_cn_assigns_vrrp_to_ios_and_eth_trunk_to_vrp() {
    let s = campus_cn();
    assert!(
        !s.contains("display vrrp"),
        "IOS CORE1/CORE2 不得再写 VRP 的 display vrrp"
    );
    assert!(s.contains("show vrrp"), "IOS 必须用 show vrrp");
    assert!(s.contains("show ip ospf neighbor"), "IOS OSPF 查询必须保留");
    assert!(s.contains("display eth-trunk 1"), "VRP 聚合查询必须保留");
    assert!(s.contains("IOS CORE1/CORE2"), "必须写明 IOS 设备归属");
    assert!(s.contains("VRP SW1/SW2"), "必须写明 VRP 设备归属");
    assert_eq!(s.matches("show vrrp").count(), 1);
    // 7 项检查与数据面配置没有因文案修正而改变。
    for check in [
        "lagSelected",
        "sviIp",
        "vrrpMaster",
        "ospfNeighbors",
        "ping",
    ] {
        assert!(s.contains(check), "campus 检查项 {check} 不得删除");
    }
}

#[test]
fn stage3b_campus_capstone_en_matches_cn() {
    let s = campus_en();
    assert!(
        !s.contains("display vrrp"),
        "英文同样不得给 IOS 写 display vrrp"
    );
    assert!(s.contains("show vrrp"));
    assert!(s.contains("show ip ospf neighbor"));
    assert!(s.contains("display eth-trunk 1"));
    assert!(s.contains("IOS CORE1/CORE2"));
    assert!(s.contains("VRP SW1/SW2"));
    assert!(s.contains("Selected members"));
}

#[test]
fn stage3b_enterprise_ping_does_not_claim_nat_in_either_language() {
    let cn = enterprise_cn();
    let en = enterprise_en();
    assert!(!cn.contains("流量已经 NAT 成公网地址"));
    assert!(!en.contains("has been NATed to a public address"));
    assert!(cn.contains("show ip nat translations"));
    assert!(en.contains("show ip nat translations"));
    assert!(cn.contains("不能单独证明"));
    assert!(en.contains("alone does not prove"));
    // NAT 配置与真实 Ping 仍保留，只是不再由连通性单独证明转换。
    assert!(cn.contains("ip nat inside") && cn.contains("ip nat outside"));
    assert!(en.contains("ip nat inside") && en.contains("ip nat outside"));
    assert!(cn.contains("PC1 ping 202.100.1.100") && en.contains("PC1 ping 202.100.1.100"));
}

#[test]
fn stage3b_security_real_rows_replace_simplified_ios_zbf_sequence() {
    let s = security_rows();
    for bad in [
        "ios: 'source trust'",
        "ios: 'destination untrust'",
        "ios: 'action permit'",
    ] {
        assert!(
            !s.contains(bad),
            "简化 IOS ZBF 子命令不得再作为真实厂商对照: {bad}"
        );
    }
    for real in [
        "class-map type inspect match-any WEB",
        "policy-map type inspect PMAP",
        "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
        "service-policy type inspect PMAP",
        "show policy-map type inspect zone-pair",
        "match protocol icmp",
        "class type inspect WEB",
        "inspect",
        "pass",
        "drop",
    ] {
        assert!(s.contains(real), "真实 IOS XE 17.x ZBF 命令缺失: {real}");
    }
    // 阶段 6B：教学抽象已移除，不再有同名简化路径或 teachingVendors 行。
    assert!(!s.contains("ios: 'zone-pair security ZP'"));
    assert!(!s.contains("teachingVendors"), "教学抽象行必须移除");
    assert!(s.contains("topic: 'security'"));
}

#[test]
fn stage3b_lesson_documents_real_zbf_without_teaching_abstraction() {
    for (md, label) in [(LESSON_CN, "cn"), (LESSON_EN, "en")] {
        assert!(
            !md.contains("netemu 教学抽象") && !md.contains("netemu teaching abstraction"),
            "{label} 课程不得再保留教学抽象"
        );
        assert!(md.contains("class-map type inspect match-any"), "{label}");
        assert!(md.contains("policy-map type inspect"), "{label}");
        assert!(md.contains("service-policy type inspect"), "{label}");
        assert!(md.contains("config-sec-zone-pair"), "{label}");
        assert!(md.contains("Cisco"), "{label} 必须写明官方证据来源");
        assert!(
            md.contains("inspect") && md.contains("pass") && md.contains("drop"),
            "{label} 必须覆盖三种动作"
        );
        assert!(
            md.contains("已实现") || md.contains("implemented"),
            "{label} 必须写明 ZBF 数据面已实现"
        );
    }
}

/// 真实 IOS ZBF 命令（正确模式）→ `Executed / full-support / DocumentationOnly`。
const ZBF_CASES: &[(Mode, &str, &str)] = &[
    (
        Mode::Config,
        "class-map type inspect match-any WEB",
        "config/class-map/type/inspect/match-any/@0word",
    ),
    (
        Mode::Config,
        "policy-map type inspect PMAP",
        "config/policy-map/type/inspect/@0word",
    ),
    (
        Mode::Config,
        "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
        "config/zone-pair/security/@0word/source/@0word/destination/@0word",
    ),
    (
        Mode::SecurityRule,
        "service-policy type inspect PMAP",
        "security_rule/service-policy/type/inspect/@0word",
    ),
    (
        Mode::Exec,
        "show policy-map type inspect zone-pair",
        "exec/show/policy-map/type/inspect/zone-pair",
    ),
];

#[test]
fn stage3b_real_zbf_commands_are_implemented_and_full_support() {
    for (mode, line, id) in ZBF_CASES {
        let i = inspect_command(IOS_ROUTER_PROFILE, *mode, line);
        assert_eq!(i.status.label(), "Executed", "{line}: {i:?}");
        assert_eq!(i.support.label(), "full-support", "{line}: {i:?}");
        assert_eq!(
            i.evidence,
            EvidenceLevel::DocumentationOnly.label(),
            "{line}"
        );
        assert_eq!(i.spec_id, *id, "{line}");
        assert!(i.passed, "{line}: {i:?}");
        assert!(i.detail.contains("平台级结果"), "{line}: {i:?}");
        let spec = specs::find(Dialect::Ios, id).unwrap_or_else(|| panic!("规格旁表缺失 {id}"));
        assert_eq!(spec.semantics, SemanticSupport::Implemented, "{id}");
        assert_eq!(spec.evidence, EvidenceLevel::DocumentationOnly, "{id}");
        assert!(spec.strict, "{id} 必须进入严格门禁");
        assert!(
            spec.references.iter().any(|r| r.contains("cisco.com")),
            "每条真实 ZBF 规格必须挂官方 IOS XE 17.x 证据"
        );
    }
}

#[test]
fn stage3b_real_zbf_commands_reject_wrong_mode() {
    let wrong: &[(Mode, &str)] = &[
        (Mode::Exec, "class-map type inspect match-any WEB"),
        (Mode::If, "policy-map type inspect PMAP"),
        (
            Mode::If,
            "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
        ),
        (Mode::Config, "service-policy type inspect PMAP"),
        (Mode::Config, "show policy-map type inspect zone-pair"),
    ];
    for (mode, line) in wrong {
        let i = inspect_command(IOS_ROUTER_PROFILE, *mode, line);
        assert_eq!(
            i.support.label(),
            "invalid-command",
            "{line} @ {mode:?} 应是模式错误"
        );
        assert_eq!(i.status.label(), "InvalidCommand");
    }
    // 同一命令在 VRP Profile 下不是 IOS 命令，保持跨方言隔离。
    let vrp = inspect_command(
        VRP_ROUTER_PROFILE,
        Mode::Config,
        "class-map type inspect match-any WEB",
    );
    assert_eq!(vrp.status.label(), "InvalidCommand");
}

#[test]
fn stage3b_not_simulated_notices_never_pollute_device_output() {
    let mut net = Net::new();
    let fw = net.add_device("FW1".into(), DeviceKind::Firewall, Dialect::Ios);
    let mut eng = Engine::new(net);
    let mut sess = Session {
        mode: Mode::Config,
        ..Default::default()
    };
    let before = state_fingerprint(&eng);
    // 阶段 6B：真实 ZBF 已实现；这里用仍属边界的 parameter-map 验证
    // 「语法有效但语义未模拟」的独立提示通道。
    let r = exec_line_result(&mut eng, &mut sess, fw, "parameter-map type inspect PMAP");
    assert_eq!(r.status.label(), "NotSimulated");
    assert!(!r.state_changed, "未模拟命令不得改变设备状态");
    assert_eq!(
        state_fingerprint(&eng),
        before,
        "纯读 / 未模拟路径前后状态指纹必须一致"
    );
    assert!(!r.device_output.contains("[netemu]"));
    assert!(!r.device_output.contains("netemu"));
    assert!(!r.device_output.contains("% "), "不得伪造厂商错误文本");
    assert!(!r.simulator_notices.is_empty());
    assert!(r
        .simulator_notices
        .iter()
        .all(|n| n.starts_with("[netemu]")));
    assert!(
        !r.device_text().contains("netemu"),
        "旧文本 API 只渲染设备文本"
    );
}

#[test]
fn stage3b_legacy_simplified_zone_pair_path_is_removed_in_favor_of_real_zbf() {
    let t = trees(Dialect::Ios);
    // 官方 zone-pair 必须带 source/destination；仅名字是 Incomplete，
    // 旧的简化子命令（source/destination/action）已不存在。
    assert!(matches!(
        parse(t.root_for(Mode::Config), "zone-pair security ZP"),
        Err(cli_engine::CliError::Incomplete { .. })
    ));
    for line in ["source trust", "destination untrust", "action permit"] {
        assert!(
            !matches!(parse(t.root_for(Mode::SecurityRule), line), Ok(Some(_))),
            "{line} 简化子命令必须移除"
        );
    }
    let i = inspect_command(IOS_ROUTER_PROFILE, Mode::Config, "zone-pair security ZP");
    assert_eq!(i.support.label(), "invalid-command");

    // 真实路径的命令全部保留可执行。
    for (mode, line) in [
        (Mode::Config, "zone security trust"),
        (Mode::If, "zone-member security trust"),
        (
            Mode::Config,
            "zone-pair security IN-OUT source trust destination untrust",
        ),
    ] {
        assert!(
            matches!(parse(t.root_for(mode), line), Ok(Some(_))),
            "{line} @ {mode:?} 真实 IOS ZBF 路径必须可执行"
        );
    }
}

#[test]
fn stage3b_comware_security_rows_still_never_full_support() {
    // 阶段 4A 分层矩阵（替代阶段 3B 的「Comware 全量不得完整支持」）：
    // 1) security 主题仍未收口，Comware 原生写法只可为待验证 / 仅识别；
    // 2) VRP 专属写法在 Comware 上必须被独立命令树明确拒绝；
    // 3) Comware 原生写法在 VRP 上同样必须被拒绝（双向隔离）。
    let comware_native: &[(Mode, &str)] = &[
        (Mode::Config, "security-zone name trust"),
        (Mode::Zone, "import interface g1/0/1"),
        (Mode::Config, "security-policy"),
        (Mode::SecurityRule, "source-zone trust"),
        (Mode::SecurityRule, "destination-zone untrust"),
        (Mode::SecurityRule, "action permit"),
        (Mode::Exec, "display security-zone"),
        (Mode::Exec, "display security-policy"),
    ];
    for (mode, line) in comware_native {
        let i = inspect_command(COMWARE_ROUTER_PROFILE, *mode, line);
        assert_ne!(
            i.support.label(),
            "full-support",
            "Comware security 负向门禁（未收口主题）: {line}"
        );
        assert_ne!(
            i.support.label(),
            "invalid-command",
            "Comware 原生写法应可解析: {line}"
        );
        let vrp = inspect_command(VRP_ROUTER_PROFILE, *mode, line);
        // VRP 树仍有 `security-policy` / security-rule 等共享抽象，
        // 只对明确为 Comware 专属的区域写法断言双向拒绝。
        if line.contains("security-zone name") || line.contains("import interface") {
            assert_eq!(
                vrp.support.label(),
                "invalid-command",
                "Comware 专属写法不得混入 VRP: {line}"
            );
        }
    }
    let vrp_only: &[(Mode, &str)] = &[
        (Mode::Config, "firewall zone trust"),
        (Mode::Zone, "add interface g0/0/0"),
    ];
    for (mode, line) in vrp_only {
        let i = inspect_command(COMWARE_ROUTER_PROFILE, *mode, line);
        assert_eq!(
            i.support.label(),
            "invalid-command",
            "VRP 专属写法不得冒充 Comware: {line}"
        );
    }
}

#[test]
fn stage3b_ios_spec_lint_has_no_drift_and_zbf_rows_are_documented() {
    let report = specs::lint_report(Dialect::Ios);
    assert!(report.drift.is_empty(), "{report:?}");
    assert!(report.dangling.is_empty(), "{report:?}");
    assert!(report.duplicates.is_empty(), "{report:?}");
    assert!(report.strict_ok(), "{report:?}");
    for id in [
        "config/class-map/type/inspect/match-any/@0word",
        "config/class-map/type/inspect/match-all/@0word",
        "config/policy-map/type/inspect/@0word",
        "config/zone-pair/security/@0word/source/@0word/destination/@0word",
        "security_rule/service-policy/type/inspect/@0word",
        "exec/show/policy-map/type/inspect/zone-pair",
        "class_map/match/protocol/@0word",
        "class_map/match/access-group/@0number",
        "policy_map/class/type/inspect/@0word",
        "policy_map_class/inspect",
        "policy_map_class/pass",
        "policy_map_class/drop",
        "config/parameter-map/type/inspect/@0word",
    ] {
        assert!(
            !report.unassociated.iter().any(|u| u == id),
            "ZBF 可执行节点缺少规格: {id}"
        );
        assert!(
            specs::find(Dialect::Ios, id).is_some(),
            "ZBF 规格旁表缺失 {id}"
        );
    }
    // 已实现 ZBF 规格：Implemented + DocumentationOnly + strict + Cisco 正文。
    let implemented = [
        "config/class-map/type/inspect/match-any/@0word",
        "config/class-map/type/inspect/match-all/@0word",
        "class_map/match/protocol/@0word",
        "class_map/match/access-group/@0number",
        "config/policy-map/type/inspect/@0word",
        "policy_map/class/type/inspect/@0word",
        "policy_map/class/class-default",
        "policy_map_class/inspect",
        "policy_map_class/pass",
        "policy_map_class/drop",
        "policy_map_class/drop/log",
        "config/zone-pair/security/@0word/source/@0word/destination/@0word",
        "security_rule/service-policy/type/inspect/@0word",
        "exec/show/policy-map/type/inspect/zone-pair",
        "exec/show/policy-map/type/inspect/zone-pair/session",
        "exec/show/zone-pair/security",
        "exec/show/zone/security",
        "config/zone/security/@0word",
        "if/zone-member/security/@0word",
        "if/no/zone-member/security/@0word",
        "config/no/zone/security/@0word",
        "config/no/zone-pair/security/@0word",
        "config/no/class-map/@0word",
        "config/no/policy-map/@0word",
    ];
    for id in implemented {
        let r = specs::find(Dialect::Ios, id).unwrap_or_else(|| panic!("缺少规格 {id}"));
        assert_eq!(r.semantics, SemanticSupport::Implemented, "{id}");
        assert_eq!(r.evidence, EvidenceLevel::DocumentationOnly, "{id}");
        assert!(r.strict, "{id} 必须进入严格门禁");
        assert!(
            r.references.iter().any(|x| x.contains("cisco.com")),
            "{id} 必须引用 Cisco IOS XE 17.x 正文"
        );
    }
    // 未模拟的邻接命令显式 NotSimulated，不冒充已实现。
    let not_simulated = [
        "config/parameter-map/type/inspect/@0word",
        "class_map/match/class-map/@0word",
        "security_rule/description/@0word-list",
        "security_rule/platform/inspect/match-statistics/per-filter",
        "policy_map_class/service-policy/type/inspect/@0word",
    ];
    for id in not_simulated {
        let r = specs::find(Dialect::Ios, id).unwrap_or_else(|| panic!("缺少规格 {id}"));
        assert_eq!(r.semantics, SemanticSupport::NotSimulated, "{id}");
        assert_eq!(r.effect, Effect::Unimplemented, "{id}");
        assert_eq!(r.evidence, EvidenceLevel::DocumentationOnly, "{id}");
        assert!(!r.strict, "{id}");
        assert!(!r.references.is_empty(), "{id}");
    }
}
