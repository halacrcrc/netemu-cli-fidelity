//! 派生命令规格、稳定 ID、lint 以及通用执行/检查结果类型。
//!
//! 这里不定义任何厂商语法。`CNode` 仍是语法、模式、帮助和补全的唯一输入源；
//! 规格表只以稳定 ID 关联元数据，`CommandSpec` 是从命令树 + 元数据派生的逻辑视图。
//! 稳定 ID = 规范模式路径 + 规范字面量路径 + 参数槽序号（不含参数显示名）。

use crate::{node_segment, CNode, Mode, Tok};

pub use sim_core::profile::{CapabilityVerdict, EvidenceLevel};

/// 四态执行结果；"待验证"是证据元数据，不是第五态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandStatus {
    Executed,
    UnsupportedByDevice,
    NotSimulated,
    InvalidCommand,
}

impl CommandStatus {
    pub fn label(self) -> &'static str {
        match self {
            CommandStatus::Executed => "Executed",
            CommandStatus::UnsupportedByDevice => "UnsupportedByDevice",
            CommandStatus::NotSimulated => "NotSimulated",
            CommandStatus::InvalidCommand => "InvalidCommand",
        }
    }

    /// 所有非 `Executed` 路径都不得改变设备状态。
    pub fn allows_state_change(self) -> bool {
        self == CommandStatus::Executed
    }
}

/// 结构化命令结果。`device_output` 只放真实设备文本，模拟器限制/验证状态
/// 只放 `simulator_notices`；不含仿真事件流。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandResult {
    pub status: CommandStatus,
    pub device_output: String,
    pub simulator_notices: Vec<String>,
    pub state_changed: bool,
}

impl CommandResult {
    pub fn executed(device_output: String, state_changed: bool) -> Self {
        Self {
            status: CommandStatus::Executed,
            device_output,
            simulator_notices: vec![],
            state_changed,
        }
    }

    pub fn rejected(
        status: CommandStatus,
        device_output: String,
        simulator_notices: Vec<String>,
    ) -> Self {
        debug_assert!(!status.allows_state_change());
        Self {
            status,
            device_output,
            simulator_notices,
            state_changed: false,
        }
    }

    /// 迁移期文本契约：旧 `exec_command` / `exec_and_run` 只渲染设备文本，
    /// 模拟器提示不会混进 `device_output`。
    pub fn device_text(&self) -> &str {
        &self.device_output
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticSupport {
    Implemented,
    NotSimulated,
}

/// 已接线命令的效果分类；避免把 `exit`、查询等合法无状态命令误判为空实现。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    StateChange,
    Query,
    ModeSwitch,
    Session,
    Traffic,
    /// 已确认平台支持但承诺语义未实现。
    Unimplemented,
}

impl Effect {
    pub fn label(self) -> &'static str {
        match self {
            Effect::StateChange => "state-change",
            Effect::Query => "query",
            Effect::ModeSwitch => "mode-switch",
            Effect::Session => "session",
            Effect::Traffic => "traffic",
            Effect::Unimplemented => "unimplemented",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportLabel {
    FullSupport,
    RecognizedOnly,
    UnsupportedByDevice,
    PendingVerification,
    InvalidCommand,
}

impl SupportLabel {
    pub fn label(self) -> &'static str {
        match self {
            SupportLabel::FullSupport => "full-support",
            SupportLabel::RecognizedOnly => "recognized-only",
            SupportLabel::UnsupportedByDevice => "unsupported-by-device",
            SupportLabel::PendingVerification => "pending-verification",
            SupportLabel::InvalidCommand => "invalid-command",
        }
    }
}

/// 被动命令检查的结构化结果（Profile 级 / 设备会话级共用）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspection {
    /// 是否可标为"完整支持"；待验证与仅识别均为 false。
    pub passed: bool,
    pub detail: String,
    pub status: CommandStatus,
    pub support: SupportLabel,
    pub evidence: &'static str,
    pub spec_id: String,
}

/// 纯判断输入；由解析、能力、语义和证据检查共同填写，不含任何副作用入口。
#[derive(Clone, Copy, Debug)]
pub struct JudgeInput<'a> {
    pub spec_id: &'a str,
    pub parse_ok: bool,
    pub mode_ok: bool,
    pub capability: CapabilityVerdict,
    pub semantics: SemanticSupport,
    pub evidence: EvidenceLevel,
}

/// 主动实验判分与被动命令检查共享的**纯判断函数**。
/// 能力检查必须先于模拟语义检查；证据不足不得判为 `UnsupportedByDevice`。
pub fn judge(input: JudgeInput<'_>) -> Inspection {
    let JudgeInput {
        spec_id,
        parse_ok,
        mode_ok,
        capability,
        semantics,
        evidence,
    } = input;
    let ev = evidence.label();
    let id = spec_id.to_string();
    if !parse_ok || !mode_ok {
        return Inspection {
            passed: false,
            detail: "语法或当前模式不正确".into(),
            status: CommandStatus::InvalidCommand,
            support: SupportLabel::InvalidCommand,
            evidence: ev,
            spec_id: id,
        };
    }
    if capability == CapabilityVerdict::Unsupported {
        return Inspection {
            passed: false,
            detail: "当前设备 Profile 有积极证据表明不支持该能力".into(),
            status: CommandStatus::UnsupportedByDevice,
            support: SupportLabel::UnsupportedByDevice,
            evidence: ev,
            spec_id: id,
        };
    }
    if semantics == SemanticSupport::NotSimulated {
        let detail = if evidence.can_decide_capability() {
            "命令在所选平台有效，但模拟器尚未实现承诺语义；仅识别".to_string()
        } else {
            "模拟器尚未实现承诺语义，且平台行为证据不足；待验证，不宣称真机有效".to_string()
        };
        let support = if evidence.can_decide_capability() {
            SupportLabel::RecognizedOnly
        } else {
            SupportLabel::PendingVerification
        };
        return Inspection {
            passed: false,
            detail,
            status: CommandStatus::NotSimulated,
            support,
            evidence: ev,
            spec_id: id,
        };
    }
    if !evidence.can_decide_capability() {
        // 证据不足但语义已实现：仍执行并返回 Executed，只是不得进入"完整支持"。
        return Inspection {
            passed: false,
            detail: "语义已实现并执行；平台能力或输出证据不足，待验证".into(),
            status: CommandStatus::Executed,
            support: SupportLabel::PendingVerification,
            evidence: ev,
            spec_id: id,
        };
    }
    Inspection {
        passed: true,
        detail: "语法、模式、能力与模拟语义均满足".into(),
        status: CommandStatus::Executed,
        support: SupportLabel::FullSupport,
        evidence: ev,
        spec_id: id,
    }
}

// ---------- 稳定 ID 与规格 lint ----------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedCommand {
    pub id: String,
    /// 节点自身 + 参数槽的结构指纹，不含祖先命令路径，也不含参数显示名。
    pub shape: String,
    pub mode: &'static str,
}

/// 稳定 ID：模式 + 规范字面量路径 + 参数槽序号。
pub fn command_id(mode: &str, path: &[String]) -> String {
    if path.is_empty() {
        mode.to_string()
    } else {
        format!("{}/{}", mode, path.join("/"))
    }
}

/// 结构指纹：字面量/参数种类 + 直接子参数槽；参数显示名变化不算漂移。
pub fn shape_of(node: &CNode) -> String {
    let head = match &node.tok {
        Tok::Lit(w) => format!("lit:{w}"),
        Tok::Param { kind, .. } => format!("param:{}", kind.tag()),
    };
    let mut slots = Vec::new();
    let mut n = 0usize;
    for kid in &node.kids {
        if let Tok::Param { kind, .. } = &kid.tok {
            slots.push(format!("{n}.{}", kind.tag()));
            n += 1;
        }
    }
    format!("{head}|p[{}]", slots.join(","))
}

/// 从现有命令树派生所有可执行节点（`run.is_some()`）；不复制任何语法定义。
pub fn derive_commands(roots: &[(&'static str, &[CNode])]) -> Vec<DerivedCommand> {
    let mut out = Vec::new();
    for (mode, root) in roots {
        let mut pslot = 0usize;
        for node in *root {
            let slot = match &node.tok {
                Tok::Param { .. } => {
                    let s = pslot;
                    pslot += 1;
                    s
                }
                _ => 0,
            };
            let mut path = vec![node_segment(node, slot)];
            walk(node, mode, &mut path, &mut out);
        }
    }
    out
}

fn walk(node: &CNode, mode: &'static str, path: &mut Vec<String>, out: &mut Vec<DerivedCommand>) {
    if node.run.is_some() {
        out.push(DerivedCommand {
            id: command_id(mode, path),
            shape: shape_of(node),
            mode,
        });
    }
    let mut pslot = 0usize;
    for kid in &node.kids {
        let slot = match &kid.tok {
            Tok::Param { .. } => {
                let s = pslot;
                pslot += 1;
                s
            }
            _ => 0,
        };
        path.push(node_segment(kid, slot));
        walk(kid, mode, path, out);
        path.pop();
    }
}

/// lint 只需要规格表的关联键与结构指纹；具体元数据由各方言自行持有。
pub trait SpecKey {
    fn id(&self) -> &str;
    fn shape(&self) -> &str;
    fn strict(&self) -> bool;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LintReport {
    /// 重复的派生 ID 或重复的旁表 ID。
    pub duplicates: Vec<String>,
    /// 旁表项找不到对应可执行节点（悬空）。
    pub dangling: Vec<String>,
    /// ID 仍存在但节点结构指纹已变化（定义漂移）。
    pub drift: Vec<String>,
    /// 可执行节点尚未关联元数据，全部显式列出作为迁移清单，不隐藏豁免。
    pub unassociated: Vec<String>,
    /// 严格门禁（C1/H0）中缺失、悬空或漂移的 ID。
    pub strict_missing: Vec<String>,
    pub derived_total: usize,
    pub strict_total: usize,
    pub strict_covered: usize,
}

impl LintReport {
    /// 严格门禁是否通过；迁移清单不参与判定，但会完整暴露。
    pub fn strict_ok(&self) -> bool {
        self.duplicates.is_empty()
            && self.dangling.is_empty()
            && self.drift.is_empty()
            && self.strict_missing.is_empty()
    }
}

/// 检查覆盖/悬空/重复 ID/定义漂移。未关联节点全部返回，调用方必须显式列入迁移清单。
pub fn lint<K: SpecKey>(
    roots: &[(&'static str, &[CNode])],
    entries: &[K],
    strict_ids: &[&str],
) -> LintReport {
    let derived = derive_commands(roots);
    let mut report = LintReport {
        derived_total: derived.len(),
        ..Default::default()
    };

    let mut id_counts = std::collections::HashMap::<&str, usize>::new();
    for d in &derived {
        *id_counts.entry(d.id.as_str()).or_insert(0) += 1;
    }
    for (id, n) in &id_counts {
        if *n > 1 {
            report.duplicates.push((*id).to_string());
        }
    }

    let derived_shapes: std::collections::HashMap<&str, &str> = derived
        .iter()
        .map(|d| (d.id.as_str(), d.shape.as_str()))
        .collect();

    let mut entry_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for e in entries {
        if !entry_ids.insert(e.id()) {
            report.duplicates.push(e.id().to_string());
        }
        match derived_shapes.get(e.id()) {
            None => report.dangling.push(e.id().to_string()),
            Some(shape) if *shape != e.shape() => report.drift.push(e.id().to_string()),
            _ => {}
        }
    }

    for d in &derived {
        if !entry_ids.contains(d.id.as_str()) {
            report.unassociated.push(d.id.clone());
        }
    }

    report.strict_total = strict_ids.len();
    for id in strict_ids {
        let ok = entry_ids.contains(id)
            && derived_shapes.contains_key(id)
            && !report.dangling.iter().any(|x| x == id)
            && !report.drift.iter().any(|x| x == id);
        if ok {
            report.strict_covered += 1;
        } else {
            report.strict_missing.push((*id).to_string());
        }
    }

    for v in [
        &mut report.duplicates,
        &mut report.dangling,
        &mut report.drift,
        &mut report.unassociated,
        &mut report.strict_missing,
    ] {
        v.sort();
        v.dedup();
    }
    report
}

/// 模式稳定标签，供稳定 ID 派生复用。
pub fn mode_tag(mode: Mode) -> &'static str {
    mode.tag()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CNode, ParamKind};

    #[derive(Clone, Copy)]
    struct Row {
        id: &'static str,
        shape: &'static str,
        strict: bool,
    }
    impl SpecKey for Row {
        fn id(&self) -> &str {
            self.id
        }
        fn shape(&self) -> &str {
            self.shape
        }
        fn strict(&self) -> bool {
            self.strict
        }
    }

    fn tree() -> Vec<CNode> {
        vec![
            CNode::lit("show", "Show").kid(
                CNode::lit("ip", "IP").kid(CNode::lit("route", "Route").run(|_| Ok(String::new()))),
            ),
            CNode::lit("interface", "Interface")
                .run(|_| Ok(String::new()))
                .kid(CNode::param(ParamKind::IfName, "if", "Interface name")),
        ]
    }

    #[test]
    fn stable_id_excludes_param_display_name() {
        let a = CNode::lit("ip", "IP")
            .run(|_| Ok(String::new()))
            .kid(CNode::param(ParamKind::Ipv4, "addr", "Address"));
        let b = CNode::lit("ip", "IP")
            .run(|_| Ok(String::new()))
            .kid(CNode::param(ParamKind::Ipv4, "renamed", "Address"));
        assert_eq!(shape_of(&a), shape_of(&b));
        assert_eq!(
            derive_commands(&[("if", std::slice::from_ref(&a))])[0].id,
            derive_commands(&[("if", std::slice::from_ref(&b))])[0].id
        );
    }

    #[test]
    fn literal_mode_or_param_slot_change_is_drift() {
        let base = CNode::lit("route", "Route")
            .run(|_| Ok(String::new()))
            .kid(CNode::param(ParamKind::Ipv4, "dst", "Destination"));
        let renamed = CNode::lit("routes", "Route")
            .run(|_| Ok(String::new()))
            .kid(CNode::param(ParamKind::Ipv4, "dst", "Destination"));
        let kind_changed = CNode::lit("route", "Route")
            .run(|_| Ok(String::new()))
            .kid(CNode::param(ParamKind::PrefixLen, "dst", "Destination"));
        let extra_slot = CNode::lit("route", "Route")
            .run(|_| Ok(String::new()))
            .kid(CNode::param(ParamKind::Ipv4, "dst", "Destination"))
            .kid(CNode::param(ParamKind::Ipv4, "nh", "Next hop"));
        assert_ne!(
            derive_commands(&[("config", std::slice::from_ref(&base))])[0].id,
            derive_commands(&[("config", std::slice::from_ref(&renamed))])[0].id
        );
        assert_ne!(
            derive_commands(&[("config", std::slice::from_ref(&base))])[0].id,
            derive_commands(&[("priv", std::slice::from_ref(&base))])[0].id
        );
        assert_ne!(shape_of(&base), shape_of(&kind_changed));
        assert_ne!(shape_of(&base), shape_of(&extra_slot));
    }

    #[test]
    fn lint_detects_dangling_duplicate_drift_and_unassociated() {
        let root = tree();
        let good = Row {
            id: "exec/show/ip/route",
            shape: "lit:route|p[]",
            strict: true,
        };
        let roots = [("exec", root.as_slice())];
        let strict = ["exec/show/ip/route"];

        // 基线：一个已关联节点 + 一个显式报告的未关联节点。
        let ok = lint(&roots, &[good], &strict);
        assert!(ok.strict_ok(), "{ok:?}");
        assert_eq!(ok.unassociated, vec!["exec/interface".to_string()]);
        assert_eq!(ok.strict_covered, 1);

        // 故意制造悬空旁表项。
        let dangling = Row {
            id: "exec/show/ip/rout",
            shape: "lit:route|p[]",
            strict: false,
        };
        let r = lint(&roots, &[good, dangling], &strict);
        assert_eq!(r.dangling, vec!["exec/show/ip/rout".to_string()]);

        // 故意制造定义漂移：ID 不变但结构指纹被改写。
        let drifted = Row {
            id: "exec/show/ip/route",
            shape: "lit:route|p[0.ipv4]",
            strict: true,
        };
        let r = lint(&roots, &[drifted], &strict);
        assert_eq!(r.drift, vec!["exec/show/ip/route".to_string()]);
        assert_eq!(r.strict_missing, vec!["exec/show/ip/route".to_string()]);
        assert!(!r.strict_ok());

        // 故意制造重复 ID。
        let r = lint(&roots, &[good, good], &strict);
        assert_eq!(r.duplicates, vec!["exec/show/ip/route".to_string()]);

        // 严格门禁中的命令若缺少元数据必须报出。
        let r = lint(&roots, &[] as &[Row], &strict);
        assert_eq!(r.strict_missing, vec!["exec/show/ip/route".to_string()]);
        assert_eq!(r.unassociated.len(), 2);
    }

    #[test]
    fn duplicate_derived_ids_are_reported() {
        let root = vec![
            CNode::lit("show", "Show").run(|_| Ok(String::new())),
            CNode::lit("show", "Show again").run(|_| Ok(String::new())),
        ];
        let r = lint(&[("exec", root.as_slice())], &[] as &[Row], &[]);
        assert_eq!(r.duplicates, vec!["exec/show".to_string()]);
    }

    #[test]
    fn capability_check_precedes_semantics_check_and_pending_evidence_still_executes() {
        let base = JudgeInput {
            spec_id: "x",
            parse_ok: true,
            mode_ok: true,
            capability: CapabilityVerdict::Supported,
            semantics: SemanticSupport::Implemented,
            evidence: EvidenceLevel::DocumentationOnly,
        };
        assert_eq!(judge(base).status, CommandStatus::Executed);
        assert_eq!(judge(base).support, SupportLabel::FullSupport);
        assert!(judge(base).passed);

        let both = JudgeInput {
            capability: CapabilityVerdict::Unsupported,
            semantics: SemanticSupport::NotSimulated,
            ..base
        };
        assert_eq!(judge(both).status, CommandStatus::UnsupportedByDevice);

        let pending = JudgeInput {
            evidence: EvidenceLevel::Unverified,
            ..base
        };
        assert_eq!(judge(pending).status, CommandStatus::Executed);
        assert_eq!(judge(pending).support, SupportLabel::PendingVerification);
        assert!(!judge(pending).passed);

        let unknown_cap = JudgeInput {
            capability: CapabilityVerdict::Unknown,
            ..base
        };
        assert_eq!(judge(unknown_cap).status, CommandStatus::Executed);

        let bad = JudgeInput {
            mode_ok: false,
            ..base
        };
        assert_eq!(judge(bad).status, CommandStatus::InvalidCommand);
    }
}
