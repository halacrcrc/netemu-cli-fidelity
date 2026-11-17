//! 阶段 3A：课程对照表 Profile 级结构化 inspection 回归。
//!
//! 数据直接 `include_str!` 真实 `cmdmap.ts`，不维护第二份表格。
//! 验证：状态来自 `inspect_command` 的 support/status/evidence（而非旧 boolean）；
//! router/switch Profile 差异可见。
//! 阶段 4A：原「Comware 一律不得完整支持」改为**分层矩阵**——首批已闭环且
//! 有 H3C 官方证据的行必须 `full-support`；未迁移 / 未模拟 / 证据不足的行不得
//! `full-support`；security 等未收口主题继续保留负向门禁（且不得变成命令错误）。

use cli_dialects::{inspect_command, specs};
use cli_engine::Mode;
use sim_core::device::Dialect;
use sim_core::profile::{
    COMWARE_ROUTER_PROFILE, COMWARE_SWITCH_PROFILE, IOS_ROUTER_PROFILE, IOS_SWITCH_PROFILE,
    TMOS_LB_PROFILE, VRP_ROUTER_PROFILE, VRP_SWITCH_PROFILE,
};

#[derive(Debug)]
struct Row {
    topic: String,
    feature: String,
    vendor: &'static str,
    cmd: String,
    mode: String,
    role: &'static str,
}

fn field<'a>(block: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}: '");
    let start = block.find(&needle)? + needle.len();
    let rest = &block[start..];
    let end = rest.find('\'')?;
    Some(&rest[..end])
}

fn mode_of(s: &str) -> Mode {
    match s {
        "Exec" => Mode::Exec,
        "Priv" => Mode::Priv,
        "Config" => Mode::Config,
        "If" => Mode::If,
        "Pool" => Mode::Pool,
        "Acl" => Mode::Acl,
        "Line" => Mode::Line,
        "Aaa" => Mode::Aaa,
        "Mst" => Mode::Mst,
        "Ospf" => Mode::Ospf,
        "OspfArea" => Mode::OspfArea,
        "Bgp" => Mode::Bgp,
        "Isis" => Mode::Isis,
        "RoutePolicy" => Mode::RoutePolicy,
        "Zone" => Mode::Zone,
        "SecurityPolicy" => Mode::SecurityPolicy,
        "SecurityRule" => Mode::SecurityRule,
        "ClassMap" => Mode::ClassMap,
        "PolicyMap" => Mode::PolicyMap,
        "PolicyMapClass" => Mode::PolicyMapClass,
        "Ipsec" => Mode::Ipsec,
        "Ospfv3" => Mode::Ospfv3,
        other => panic!("未知命令模式 {other}"),
    }
}

fn profiles(vendor: &str) -> [(&'static str, &'static str); 2] {
    match vendor {
        "ios" => [
            ("router", IOS_ROUTER_PROFILE),
            ("switch", IOS_SWITCH_PROFILE),
        ],
        "vrp" => [
            ("router", VRP_ROUTER_PROFILE),
            ("switch", VRP_SWITCH_PROFILE),
        ],
        "comware" => [
            ("router", COMWARE_ROUTER_PROFILE),
            ("switch", COMWARE_SWITCH_PROFILE),
        ],
        // 阶段 5A：TMOS 只有 LoadBalancer 一种设备类型，不按 router/switch 拆分。
        "tmos" => [("router", TMOS_LB_PROFILE), ("switch", TMOS_LB_PROFILE)],
        other => panic!("未知厂商 {other}"),
    }
}

/// 用与 `cmdmap.ts::TOPIC_PROFILE_ROLE` 一致的角色表解析真实表格。
fn topic_role(topic: &str) -> &'static str {
    match topic {
        "vlan" | "stp" | "lag" | "stack" | "mlag" | "ha" | "vxlan" | "isolation" => "switch",
        "campus" => "switch-or-router", // campus 只有 Trunk 行显式覆盖，下面按 feature 区分
        _ => "router",
    }
}

/// 真实课程表的行块；`include_str!` 直接读 `cmdmap.ts`，不维护第二份表格。
fn table_blocks() -> Vec<&'static str> {
    include_str!("../../../web/src/content/cmdmap.ts")
        .split('}')
        .filter(|b| {
            field(b, "topic").is_some()
                && field(b, "feature").is_some()
                && field(b, "mode").is_some()
        })
        .collect()
}

fn rows() -> Vec<Row> {
    let mut out = Vec::new();
    for block in table_blocks() {
        let topic = field(block, "topic").expect("table_blocks 已过滤");
        let feature = field(block, "feature").expect("table_blocks 已过滤");
        let mode = field(block, "mode").expect("table_blocks 已过滤");
        let mut role = topic_role(topic);
        if topic == "campus" {
            role = if feature.contains("Trunk") {
                "switch"
            } else {
                "router"
            };
        }
        if field(block, "role").is_some() {
            role = "switch";
        }
        for (vendor, key, mode_key) in [
            ("ios", "ios", "modeIos"),
            ("vrp", "vrp", "modeVrp"),
            ("comware", "comware", "modeComware"),
            // 阶段 5A：第四列 tmos 也必须参与零拒绝硬断言，不能漏检。
            ("tmos", "tmos", "modeTmos"),
        ] {
            let Some(cmd) = field(block, key) else {
                continue;
            };
            // 平台专属行的空列 = 该平台没有逐字对应命令；不是过滤厂商，也不是接受错误状态。
            if cmd.is_empty() {
                continue;
            }
            let m = field(block, mode_key).unwrap_or(mode);
            out.push(Row {
                topic: topic.to_string(),
                feature: feature.to_string(),
                vendor,
                cmd: cmd.to_string(),
                mode: m.to_string(),
                role,
            });
        }
    }
    out
}

fn selected_profile(row: &Row) -> &'static str {
    profiles(row.vendor)
        .iter()
        .find(|(r, _)| *r == row.role)
        .map(|(_, p)| *p)
        .unwrap_or_else(|| panic!("{} 无 Profile", row.role))
}

#[test]
fn stage3a_cmdmap_is_parsed_from_the_real_table() {
    let rows = rows();
    assert!(
        rows.len() >= 300,
        "三厂商对照表应解析出至少 300 个单元格，实际 {}",
        rows.len()
    );
    // Stage 3B：security 段允许平台专属行（空列 = 该平台无逐字对应命令）。
    // 每个课程行仍必须至少产生一个真实单元格，三厂商整体都必须被覆盖；
    // 零拒绝门禁不因此放宽：所有非空单元格（含全部 IOS security 行）仍逐格检查。
    assert!(
        rows.len() >= table_blocks().len(),
        "每个课程行至少有一个真实单元格：rows={} blocks={}",
        rows.len(),
        table_blocks().len()
    );
    assert!(rows.iter().any(|r| r.vendor == "ios"));
    assert!(rows.iter().any(|r| r.vendor == "vrp"));
    assert!(rows.iter().any(|r| r.vendor == "comware"));
    // 代表课程的 L2 行必须显式选择 switch Profile。
    for r in rows.iter().filter(|r| r.topic == "vlan") {
        assert_eq!(r.role, "switch", "VLAN 行 {} 必须选 switch", r.feature);
    }
}

#[test]
fn stage3a_cmdmap_uses_structured_platform_status_not_boolean() {
    let rows = rows();
    let mut supports = std::collections::BTreeSet::new();
    let mut comware_full = 0usize;
    let mut comware_not_full = 0usize;
    for r in &rows {
        // 同时检查 router / switch 两个 Profile，暴露能力门禁差异，证明缓存 key 必须含 Profile。
        for (_, p) in profiles(r.vendor) {
            let i = inspect_command(p, mode_of(&r.mode), &r.cmd);
            supports.insert(i.support.label().to_string());
            assert!(!i.detail.contains("设备实例: "), "不得伪造设备实例");
            // 解析失败或未关联规格的迁移期分支没有平台级前缀，仍必须给出结构化 detail。
            assert!(!i.detail.is_empty(), "结构化 detail 不得为空");
            if i.support.label() == "full-support" || i.support.label() == "pending-verification" {
                assert!(
                    i.detail.contains("平台级结果") || i.detail.contains("规格元数据"),
                    "非错误路径必须可识别为平台级/迁移期结果: {}",
                    i.detail
                );
            }
            assert!(!i.evidence.is_empty(), "证据等级必须可见");
            if r.vendor != "comware" {
                continue;
            }
            // 阶段 4A 分层矩阵：不再对 Comware 全量降级，也不全量放行。
            if i.support.label() == "full-support" {
                comware_full += 1;
                // 完整支持必须来自「已实现 + 有积极证据」的 Comware 规格行，
                // 不允许靠 UI 一刀切或 parse-only 伪造。
                let allowed = specs::find(Dialect::Comware, &i.spec_id)
                    .map(|s| {
                        matches!(s.semantics, cli_engine::spec::SemanticSupport::Implemented)
                            && s.evidence.can_decide_capability()
                    })
                    .unwrap_or(false);
                assert!(
                    allowed,
                    "Comware 完整支持必须关联首批已实现规格: {} / {} / {}",
                    r.topic, r.cmd, i.spec_id
                );
                assert_ne!(
                    r.topic, "security",
                    "security 主题未收口，不得完整支持: {}",
                    r.cmd
                );
            } else {
                comware_not_full += 1;
            }
            // 正向层：VLAN 首批闭环行按结构化结果显示完整支持。
            if r.topic == "vlan" {
                assert_eq!(
                    i.support.label(),
                    "full-support",
                    "VLAN 首批闭环行应完整支持: {} / {} / {} / {}",
                    r.feature,
                    r.cmd,
                    p,
                    i.detail
                );
            }
            // 负向层：security 等未收口主题继续不得完整支持，也不得被拆成命令错误。
            if r.topic == "security" {
                assert_ne!(
                    i.support.label(),
                    "full-support",
                    "Comware security 负向门禁: {}",
                    r.cmd
                );
                assert_ne!(
                    i.support.label(),
                    "invalid-command",
                    "Comware security 原生写法不应变成命令错误: {}",
                    r.cmd
                );
            }
        }
    }
    // 分层矩阵必须同时包含「完整支持」与「非完整支持」，证明不是全量放行或全量降级。
    assert!(comware_full > 0, "首批闭环行必须出现完整支持");
    assert!(comware_not_full > 0, "未迁移/未模拟行必须仍非完整支持");
    // 结构化状态至少覆盖多种，而不是旧版单一 ✓/⚠。
    assert!(
        supports.len() >= 3,
        "对照表应呈现多种结构化状态，实际 {supports:?}"
    );
}

#[test]
fn stage3a_cmdmap_router_switch_profiles_change_capability_verdict() {
    // IOS L2SwitchPort：router 有积极证据不支持，switch 支持。
    let router = inspect_command(IOS_ROUTER_PROFILE, Mode::If, "switchport access vlan 10");
    let switch = inspect_command(IOS_SWITCH_PROFILE, Mode::If, "switchport access vlan 10");
    assert_eq!(router.support.label(), "unsupported-by-device");
    assert_ne!(switch.support.label(), "unsupported-by-device");
    // 同一命令、不同 Profile 的结构化结果不同 → 缓存 key 必须包含 Profile。
    assert_ne!(router.detail, switch.detail);
}

#[test]
fn stage3a_cmdmap_invalid_and_pending_are_distinct() {
    let bad = inspect_command(
        IOS_ROUTER_PROFILE,
        Mode::Config,
        "this is not a cisco command",
    );
    assert_eq!(bad.support.label(), "invalid-command");
    assert_eq!(bad.status.label(), "InvalidCommand");
    assert!(!bad.passed);

    let route = inspect_command(IOS_ROUTER_PROFILE, Mode::Exec, "show ip route");
    assert_eq!(route.status.label(), "Executed");
    // 待验证是证据元数据，不是第五种执行状态，也不得从 passed 推导完整支持。
    assert_eq!(route.support.label(), "pending-verification");
    assert!(!route.passed);
}

/// 探测辅助：打印「选定 Profile 下仍为命令错误」或「设备不支持」的行，
/// 用于按现有 Stage 2 能力和证据修正明确错误的 IOS/VRP 写法（--nocapture）。
#[test]
fn stage3a_cmdmap_probe_rejected_rows() {
    let mut rejected = 0usize;
    let all = rows();
    for r in &all {
        let p = selected_profile(r);
        let i = inspect_command(p, mode_of(&r.mode), &r.cmd);
        if matches!(
            i.support.label(),
            "invalid-command" | "unsupported-by-device"
        ) {
            rejected += 1;
            println!(
                "REJECTED {} | {} | {} | {} | {} | {} | {}",
                r.topic,
                r.feature,
                r.vendor,
                r.role,
                r.mode,
                r.cmd,
                i.support.label()
            );
        }
    }
    println!("TOTAL_ROWS={} REJECTED={}", rows().len(), rejected);
    // Stage 3B：零拒绝从「只打印」升级为真实回归断言。
    // 不通过过滤 IOS security 行、放宽状态判断或接受错误状态来制造 0；
    // 所有非空单元格（含全部真实 IOS ZBF 行与教学抽象行）都在选定 Profile 下逐格检查。
    assert_eq!(
        rejected, 0,
        "课程表零拒绝门禁：共 {rejected} 个单元格仍是命令错误或设备不支持"
    );
}
