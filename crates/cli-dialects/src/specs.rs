//! C1/C2/C6a/H0/H1 及 Stage 2B 九条流程的命令规格旁表。
//!
//! 编号更正（Stage 2B 前置）：主方案 §8 中 C3 = IOS DHCP Pool / network /
//! default-router；C6a = IOS 本地用户、VTY、login local、认证与传输配置。
//! 本文件原「C3（管理与登录）」段实为 C6a 子集，现按 canonical 编号标注；
//! 原段行内容保持不变，仅更正标签。真正 C3 DHCP 由本文件新增段补做。
//!
//! 本文件**不定义任何语法**：没有字面量树、参数链、帮助文本或 handler。
//! 每条规格只以稳定 ID（由 `cli-engine::spec::derive_commands` 从 `CNode` 派生）
//! 关联能力、语义、证据和承诺契约。`CommandSpec` 是运行时派生视图。
//! 尚未关联的可执行节点由 `lint_report` 全部列出，作为显式迁移清单，不设隐藏豁免。

use cli_engine::spec::{self, DerivedCommand, Effect, LintReport, SemanticSupport, SpecKey};
use sim_core::device::Dialect;
use sim_core::profile::{
    Capability, EvidenceLevel, HW_FIREWALL_CHECK, HW_RULE_ADJUST, HW_SECPOLICY_CHECK,
    HW_SECPOLICY_CMDS, HW_SECPOLICY_DEFAULTS, HW_SECPOLICY_ENCYCLOPEDIA, HW_SESSION_AGING,
    HW_SESSION_CLEAR, HW_ZONE_CMDS,
};

/// 旁表行。`shape` 是节点结构指纹（不含祖先路径与参数显示名），由 lint 与命令树比对。
#[derive(Clone, Copy, Debug)]
pub struct SpecRow {
    pub dialect: Dialect,
    pub id: &'static str,
    pub shape: &'static str,
    pub effect: Effect,
    pub capability: Option<Capability>,
    pub semantics: SemanticSupport,
    pub evidence: EvidenceLevel,
    pub contract: &'static str,
    pub references: &'static [&'static str],
    /// 是否属于已承诺流程（C1/C2/C3/C4/C5/C6a/C7/C8/H0/H1/H2/H3/H6）的严格门禁。
    pub strict: bool,
}

impl SpecKey for SpecRow {
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

const CISCO_CFG: &str = "Cisco IOS XE 17.x 配置指南（文档基线，逐字输出待验证）";
const VRP_CFG: &str = "Huawei 通用 VRP 文档基线（DocumentationOnly，未经 eNSP 校准）";
const ADR_D12: &str = "docs/adr/0001-core-decisions.md D12 配置视图同源渲染";

/// 已确认 IOS/VRP 命令语义，但厂商逐字输出缺少官方正文/设备样本 → 待验证。
const PENDING_OUTPUT: EvidenceLevel = EvidenceLevel::Unverified;
/// 平台能力有官方文档支撑，输出采用结构/状态断言而非逐字黄金。
const DOC: EvidenceLevel = EvidenceLevel::DocumentationOnly;

// Stage 3B：Cisco IOS XE 17.x Zone-Based Policy Firewall 官方全文证据（2026-11-12 访问）。
// 均为官方章节正文核验，不是搜索摘要；无带版本设备/eNSP 样本 → DocumentationOnly，
// 只建立语法/模式/结构化状态测试，不建立逐字输出黄金。
const ZBF_CFG: &str = "Cisco《Security and VPN Configuration Guide, Cisco IOS XE 17.x》Zone-Based Policy Firewalls, Updated 2021-01-11, https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/sec-vpn/b-security-vpn/m_sec-zone-pol-fw-xe.html";
const ZBF_ALG_CFG: &str = "Cisco《Security and VPN Configuration Guide, Cisco IOS XE 17.x》Enabling ALGs and AICs in Zone-Based Policy Firewalls, Updated 2021-01-11, https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/sec-vpn/b-security-vpn/m_zbf_enable_alg_aic.html";
const ZBF_SCCP_CFG: &str = "Cisco《Security and VPN Configuration Guide, Cisco IOS XE 17.x》Firewall Support of Skinny Client Control Protocol, Updated 2021-01-11, https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/sec-vpn/b-security-vpn/m_sec-data-sccp.html";
const ZBF_SHOW_CFG: &str = "Cisco《IP Addressing Configuration Guide, Cisco IOS XE 17.x》vTCP for ALG Support, Updated 2022-11-15, https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/ip-addressing/b-ip-addressing/m_sec-vtcp-alg.html";
/// O-1：官方 ZBF 设计指南给出 `show policy-map type inspect zone-pair [zone-pair-name] [sessions]`，
/// 证明区域对名与 sessions 是**可组合**的可选参数（2026-11-16 访问正文）。
const ZBF_DESIGN_REF: &str = "Cisco《Understand the Zone-Based Policy Firewall Design》show policy-map type inspect zone-pair [zone-pair-name] [sessions], https://www.cisco.com/c/en/us/support/docs/security/ios-firewall/98628-zone-design-guide.html";

// 阶段 4A：H3C Comware V7 官方正文证据（访问日期 2026-11-13，DocumentationOnly）。
// 标题 / URL / 版本 / 全文边界见 docs/cli-fidelity/stage4a-implementation-record.md §2。
const CW_L2: &str = "H3C《09-VLAN configuration commands》Comware V7 R2607P26-6W101, 2019-06, https://wwwsg.h3c.com/en/d_201906/1198196_294551_0.htm";
const CW_SW: &str = "H3C《04-二层技术-以太网交换配置指导/07-VLAN映射配置》Comware V7 R7748Pxx-6W100, 2022-12, https://www.h3c.com/cn/d_202212/1745029_30005_0.htm";
const CW_EX: &str = "H3C《04-Comware 7 CLI-based configuration examples / 019-Remote Portal Authentication》Comware 7 6W102, 2024-08, https://www.h3c.com/en/d_202408/2257333_294551_0.htm";
const CW_CLI: &str = "H3C《01-基础配置指导》1.1.3 返回上一级视图 / 1.1.4 返回用户视图, Comware V7, https://www.h3c.com/cn/d_202009/1339568_30005_0.htm";
const CW_ROUTE: &str = "H3C《07-三层技术-IP路由命令参考》ip route-static / undo ip route-static, Comware V7, https://www.h3c.com/cn/d_201803/1071543_30005_0.htm";
const CW_OSPF: &str = "H3C《05-三层技术-IP路由配置指导/07-OSPF配置》1.4 配置OSPF基本功能, Comware V7 R7752-6W101, 2025-04, https://www.h3c.com/cn/d_202504/2394061_30005_0.htm";
const CW_CFG_VIEW: &str = "H3C《01-基础配置命令参考》display current-configuration, Comware V7, https://www.h3c.com/cn/d_201706/1001779_30005_0.htm";
const CW_PF: &str = "H3C《16-Security Command Reference/07-Packet filter commands》Comware V7 R1330Pxx-6W100, 2025-06, https://www.h3c.com/en/d_202506/2515724_294551_0.htm";

// 阶段 5A：F5 官方 F5 TMSH Reference v17 正文证据（访问日期 2026-11-13）。
// 标题 / URL / 版本 / 全文边界见 docs/cli-fidelity/stage5a-implementation-record.md §2。
const F5_TMSH: &str = "F5《TMSH / tmsh(1)》F5 TMSH Reference v17, https://clouddocs.f5.com/cli/tmsh-reference/latest/general/tmsh.html";
const F5_POOL: &str = "F5《ltm pool(1)》F5 TMSH Reference v17, https://clouddocs.f5.com/cli/tmsh-reference/latest/modules/ltm/ltm_pool.html";
const F5_VIRTUAL: &str = "F5《ltm virtual(1)》F5 TMSH Reference v17, https://clouddocs.f5.com/cli/tmsh-reference/latest/modules/ltm/ltm_virtual.html";
const F5_EXIT: &str = "F5《exit(1)》F5 TMSH Reference v17, https://clouddocs.f5.com/cli/tmsh-reference/latest/commands/exit.html";
const F5_QUIT: &str = "F5《quit(1)》F5 TMSH Reference v17, https://clouddocs.f5.com/cli/tmsh-reference/latest/commands/quit.html";
const F5_CD: &str = "F5《cd(1)》F5 TMSH Reference v17, https://clouddocs.f5.com/cli/tmsh-reference/latest/commands/cd.html";

macro_rules! row {
    ($d:expr, $id:expr, $shape:expr, $effect:expr, $cap:expr, $sem:expr, $ev:expr, $contract:expr, $refs:expr, $strict:expr) => {
        SpecRow {
            dialect: $d,
            id: $id,
            shape: $shape,
            effect: $effect,
            capability: $cap,
            semantics: $sem,
            evidence: $ev,
            contract: $contract,
            references: $refs,
            strict: $strict,
        }
    };
}

/// 阶段 5A：Tmos 规格行简写（能力门禁一律 None，证据等级恒为 DocumentationOnly）。
macro_rules! tmos {
    ($id:expr, $shape:expr, $effect:expr, $sem:expr, $contract:expr, $refs:expr, $strict:expr) => {
        row!(
            Dialect::Tmos,
            $id,
            $shape,
            $effect,
            None,
            $sem,
            DOC,
            $contract,
            $refs,
            $strict
        )
    };
}

/// 已接线命令的元数据；其余节点全部进入 `lint_report().unassociated` 迁移清单。
static ROWS: &[SpecRow] = &[
    // ---------- IOS / C1 ----------
    row!(
        Dialect::Ios,
        "exec/enable",
        "lit:enable|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "进入特权模式；只改会话模式",
        &["IOS CLI 模式"],
        true
    ),
    row!(
        Dialect::Ios,
        "priv/configure/terminal",
        "lit:terminal|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "进入全局配置模式",
        &["IOS CLI 模式"],
        true
    ),
    row!(
        Dialect::Ios,
        "config/hostname",
        "lit:hostname|p[0.word]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "写入设备名并反映到提示符/配置视图",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/interface",
        "lit:interface|p[0.if-name]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "选中接口并进入接口配置模式",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/ip/route",
        "lit:route|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        DOC,
        "完整预检后写入静态路由；路由表与配置视图可见",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/no/ip/route",
        "lit:route|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        DOC,
        "撤销静态路由；路由表与配置视图同步",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/exit",
        "lit:exit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "退出配置模式",
        &["IOS CLI 模式"],
        true
    ),
    row!(
        Dialect::Ios,
        "config/end",
        "lit:end|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "回到特权模式",
        &["IOS CLI 模式"],
        true
    ),
    row!(
        Dialect::Ios,
        "if/ip/address",
        "lit:address|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "完整预检后写入接口 IP/掩码；配置视图可见",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "if/no/ip/address",
        "lit:address|p[]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "撤销接口 IP；配置视图同步",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "if/shutdown",
        "lit:shutdown|p[]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "设置接口管理 down；状态摘要与配置视图同步",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "if/no/shutdown",
        "lit:shutdown|p[]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "取消 shutdown；状态摘要与配置视图同步",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "if/exit",
        "lit:exit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "退出接口配置模式",
        &["IOS CLI 模式"],
        true
    ),
    row!(
        Dialect::Ios,
        "if/end",
        "lit:end|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "回到特权模式",
        &["IOS CLI 模式"],
        true
    ),
    row!(
        Dialect::Ios,
        "exec/ping",
        "lit:ping|p[0.ipv4]",
        Effect::Traffic,
        Some(Capability::IcmpEcho),
        SemanticSupport::Implemented,
        DOC,
        "启动 ICMP echo；数据面结果由引擎推进",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "priv/ping",
        "lit:ping|p[0.ipv4]",
        Effect::Traffic,
        Some(Capability::IcmpEcho),
        SemanticSupport::Implemented,
        DOC,
        "启动 ICMP echo；数据面结果由引擎推进",
        &[CISCO_CFG],
        true
    ),
    // 逐字路由表输出缺少官方正文样本：语义已实现仍执行，但 inspect 不得标完整支持。
    row!(
        Dialect::Ios,
        "exec/show/ip/route",
        "lit:route|p[]",
        Effect::Query,
        Some(Capability::RouteTableView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从 sim-core 路由状态渲染路由表",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "priv/show/ip/route",
        "lit:route|p[]",
        Effect::Query,
        Some(Capability::RouteTableView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从 sim-core 路由状态渲染路由表",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "exec/show/running-config",
        "lit:running-config|p[]",
        Effect::Query,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从 DevSummary 实时渲染接口与静态路由",
        &[ADR_D12],
        true
    ),
    row!(
        Dialect::Ios,
        "priv/show/running-config",
        "lit:running-config|p[]",
        Effect::Query,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从 DevSummary 实时渲染接口与静态路由",
        &[ADR_D12],
        true
    ),
    // 有积极证据表明 IOS 三层路由器口不具备二层交换端口能力。
    row!(
        Dialect::Ios,
        "if/switchport/mode/access",
        "lit:access|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "仅 L2 交换端口支持；三层路由器口应被能力门禁拒绝",
        &[sim_core::profile::CISCO_L2_PORT_REF],
        true
    ),
    // ---------- VRP / H0 ----------
    row!(
        Dialect::Vrp,
        "exec/system-view",
        "lit:system-view|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "进入系统视图",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/interface",
        "lit:interface|p[0.if-name]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "进入接口视图",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/ip/route-static",
        "lit:route-static|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        DOC,
        "完整预检后写入静态路由；路由表与配置视图可见",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/undo/ip/route-static",
        "lit:route-static|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        DOC,
        "撤销静态路由；路由表与配置视图同步",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/undo/ip/route-static",
        "lit:route-static|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        DOC,
        "撤销静态路由（共享 undo 子树）",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/ip/address",
        "lit:address|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "完整预检后写入接口 IP/前缀长度",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/undo/ip/address",
        "lit:address|p[]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "撤销接口 IP",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/undo/ip/address",
        "lit:address|p[]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "撤销接口 IP",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/shutdown",
        "lit:shutdown|p[]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "设置接口管理 down",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/undo/shutdown",
        "lit:shutdown|p[]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "undo shutdown；状态摘要与配置视图同步",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/undo/shutdown",
        "lit:shutdown|p[]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "undo shutdown；状态摘要与配置视图同步",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "退出系统视图",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/return",
        "lit:return|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "回到用户视图",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "退出接口视图",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/return",
        "lit:return|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "回到用户视图",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/ping",
        "lit:ping|p[0.ipv4]",
        Effect::Traffic,
        Some(Capability::IcmpEcho),
        SemanticSupport::Implemented,
        DOC,
        "启动 ICMP echo；数据面结果由引擎推进",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/ping",
        "lit:ping|p[0.ipv4]",
        Effect::Traffic,
        Some(Capability::IcmpEcho),
        SemanticSupport::Implemented,
        DOC,
        "启动 ICMP echo；数据面结果由引擎推进",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/ip/routing-table",
        "lit:routing-table|p[]",
        Effect::Query,
        Some(Capability::RouteTableView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从 sim-core 路由状态渲染路由表",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/display/ip/routing-table",
        "lit:routing-table|p[]",
        Effect::Query,
        Some(Capability::RouteTableView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从 sim-core 路由状态渲染路由表",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/display/ip/routing-table",
        "lit:routing-table|p[]",
        Effect::Query,
        Some(Capability::RouteTableView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从 sim-core 路由状态渲染路由表",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/current-configuration",
        "lit:current-configuration|p[]",
        Effect::Query,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从 DevSummary 实时渲染接口与静态路由",
        &[ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/display/current-configuration",
        "lit:current-configuration|p[]",
        Effect::Query,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从 DevSummary 实时渲染接口与静态路由",
        &[ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/display/current-configuration",
        "lit:current-configuration|p[]",
        Effect::Query,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从 DevSummary 实时渲染接口与静态路由",
        &[ADR_D12],
        true
    ),
    // 平台已确认支持 save，但配置持久化语义未模拟 → NotSimulated，状态完全不变。
    row!(
        Dialect::Vrp,
        "exec/save",
        "lit:save|p[]",
        Effect::Unimplemented,
        Some(Capability::ConfigPersistence),
        SemanticSupport::NotSimulated,
        DOC,
        "把运行配置持久化到闪存；模拟器未实现",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/save",
        "lit:save|p[]",
        Effect::Unimplemented,
        Some(Capability::ConfigPersistence),
        SemanticSupport::NotSimulated,
        DOC,
        "把运行配置持久化到闪存；模拟器未实现",
        &[VRP_CFG],
        true
    ),
    // ---------- IOS / C2（VLAN/Access/Trunk/SVI） ----------
    row!(
        Dialect::Ios,
        "config/vlan",
        "lit:vlan|p[0.number]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "创建 VLAN；仅 L2 交换能力支持，三层路由器口被能力门禁拒绝",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/interface/vlan/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "选中 SVI 并进入接口配置模式",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "if/switchport/mode/trunk",
        "lit:trunk|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "进入 trunk 模式；初始放行范围由平台策略注入（IOS 全部 VLAN）",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "if/switchport/access/vlan/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "设置 access VLAN（pvid）并登记 VLAN 表",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "if/switchport/trunk/allowed/vlan/@0vlan-list",
        "param:vlan-list|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "设置 trunk 放行列表（IOS 替换列表，不强制 VLAN1）",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "if/no/switchport/access/vlan",
        "lit:vlan|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "否定 access VLAN，恢复默认 VLAN 1",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "exec/show/vlan/brief",
        "lit:brief|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从交换状态渲染 VLAN 表",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "priv/show/vlan/brief",
        "lit:brief|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从交换状态渲染 VLAN 表",
        &[CISCO_CFG],
        true
    ),
    // ---------- IOS / C6a（管理与登录；Stage 2A 曾误标为 C3） ----------
    row!(
        Dialect::Ios,
        "config/username/@0word/privilege/@0number/secret/@0word",
        "param:word|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "创建本地用户并设 privilege/secret；privilege>15 完整预检拒绝",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/username/@0word/secret/@0word",
        "param:word|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "创建本地用户（默认 privilege 1）",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/enable/secret/@0word",
        "param:word|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "设置 enable 口令",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/line/vty/@0number/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "进入 VTY 线路视图",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "line/login",
        "lit:login|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "启用登录认证",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "line/login/local",
        "lit:local|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "启用本地用户认证",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "line/transport/input/@0word-list",
        "param:word-list|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "设置 VTY 接入协议（telnet/ssh/all/none）",
        &[CISCO_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "line/exit",
        "lit:exit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "退出线路视图",
        &[CISCO_CFG],
        true
    ),
    // ---------- VRP / H1（VLAN/Trunk/SVI） ----------
    // VRP/Comware 的 L2 交换端口能力证据不足（E6，Unknown）：语义已实现仍执行，
    // 但 evidence 取 Unverified → inspect 待验证，不宣称完整支持。
    row!(
        Dialect::Vrp,
        "config/vlan",
        "lit:vlan|p[0.number]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "创建 VLAN",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/vlan/batch",
        "lit:batch|p[0.vlan-list]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "批量创建 VLAN",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/interface/vlanif/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "选中 Vlanif 并进入接口视图",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/port/link-type/access",
        "lit:access|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "设置端口为 access",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/port/link-type/trunk",
        "lit:trunk|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "进入 trunk；默认仅放行 VLAN1（平台策略注入）",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/port/default/vlan/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "设置 access 端口默认 VLAN（pvid）",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/port/trunk/allow-pass/vlan/@0vlan-list",
        "param:vlan-list|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "trunk 放行 VLAN；要求先 link-type trunk，累加并保留 VLAN1",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/undo/port/default/vlan",
        "lit:vlan|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "否定 access 端口默认 VLAN，恢复 VLAN 1",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/vlan",
        "lit:vlan|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从交换状态渲染 VLAN 表",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/display/vlan",
        "lit:vlan|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从交换状态渲染 VLAN 表",
        &[VRP_CFG],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/display/vlan",
        "lit:vlan|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "从交换状态渲染 VLAN 表",
        &[VRP_CFG],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "config/ip/dhcp/pool",
        "lit:pool|p[0.word]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "pool/network",
        "lit:network|p[0.ipv4]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "pool/network/@0ipv4/@0dotted-mask",
        "param:dotted-mask|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "pool/default-router/@0ipv4",
        "param:ipv4|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "pool/dns-server/@0ipv4",
        "param:ipv4|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "pool/exit",
        "lit:exit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "config/ip/dhcp/excluded-address/@0ipv4/@0ipv4",
        "param:ipv4|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "config/no/ip/dhcp/pool/@0word",
        "param:word|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "config/no/ip/dhcp/excluded-address/@0ipv4/@0ipv4",
        "param:ipv4|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "pool/no/network",
        "lit:network|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "pool/no/default-router",
        "lit:default-router|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "pool/no/dns-server",
        "lit:dns-server|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "config/ip/dhcp/snooping",
        "lit:snooping|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "if/ip/dhcp/snooping/trust",
        "lit:trust|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "exec/show/ip/dhcp/binding",
        "lit:binding|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "priv/show/ip/dhcp/binding",
        "lit:binding|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "exec/show/ip/dhcp/snooping/binding",
        "lit:binding|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Ios,
        "priv/show/ip/dhcp/snooping/binding",
        "lit:binding|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Vrp,
        "if/dhcp/select/global",
        "lit:global|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Vrp,
        "pool/network/@0ipv4/mask/@0dotted-mask",
        "param:dotted-mask|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Vrp,
        "pool/gateway-list/@0ipv4",
        "param:ipv4|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Vrp,
        "pool/dns-list/@0ipv4",
        "param:ipv4|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Vrp,
        "pool/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Vrp,
        "exec/display/dhcp/snooping/user-binding",
        "lit:user-binding|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Vrp,
        "config/display/dhcp/snooping/user-binding",
        "lit:user-binding|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C3 ----------
    row!(
        Dialect::Vrp,
        "if/display/dhcp/snooping/user-binding",
        "lit:user-binding|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C3：DHCP 池/网络/网关/DNS/排除地址与否定形式完整预检后提交；无匹配池或网关为 0 不分配",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C6a ----------
    row!(
        Dialect::Ios,
        "line/password/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C6a：裸 login=线路口令 / login local=本地用户；越界 privilege 拒绝；口令不泄漏",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C6a ----------
    row!(
        Dialect::Ios,
        "line/no/login",
        "lit:login|p[]",
        Effect::StateChange,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C6a：裸 login=线路口令 / login local=本地用户；越界 privilege 拒绝；口令不泄漏",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C6a ----------
    row!(
        Dialect::Ios,
        "config/no/enable/secret",
        "lit:secret|p[]",
        Effect::StateChange,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C6a：裸 login=线路口令 / login local=本地用户；越界 privilege 拒绝；口令不泄漏",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C6a ----------
    row!(
        Dialect::Ios,
        "config/no/username/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C6a：裸 login=线路口令 / login local=本地用户；越界 privilege 拒绝；口令不泄漏",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "config/access-list/@0number/@0word/@0ipv4/@0wildcard",
        "param:wildcard|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "config/access-list/@0number/@0word/@0ipv4/@0wildcard/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "config/access-list/@0number/@0word/icmp/@0ipv4/@0wildcard",
        "param:wildcard|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "config/access-list/@0number/@0word/icmp/@0ipv4/@0wildcard/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "config/access-list/@0number/@0word/ip/@0ipv4/@0wildcard",
        "param:wildcard|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "config/access-list/@0number/@0word/ip/@0ipv4/@0wildcard/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "config/access-list/@0number/@0word/tcp/@0ipv4/@0wildcard",
        "param:wildcard|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "config/access-list/@0number/@0word/tcp/@0ipv4/@0wildcard/eq/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "config/access-list/@0number/@0word/tcp/@0ipv4/@0wildcard/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "config/access-list/@0number/@0word/tcp/@0ipv4/@0wildcard/@0ipv4/@0wildcard/eq/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "if/ip/access-group/@0number/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "config/no/access-list/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "if/no/ip/access-group/@0number/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "exec/show/access-lists",
        "lit:access-lists|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C5 ----------
    row!(
        Dialect::Ios,
        "priv/show/access-lists",
        "lit:access-lists|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C5：首条命中/隐式拒绝，通配符与 TCP 端口参与真实数据面；删除与解绑状态不变路径",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4 ----------
    row!(
        Dialect::Ios,
        "config/ip/nat/inside",
        "lit:inside|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：inside/outside、静态映射与 EasyIP 预检后提交；no 形式同步清空转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4 ----------
    row!(
        Dialect::Ios,
        "config/ip/nat/outside",
        "lit:outside|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：inside/outside、静态映射与 EasyIP 预检后提交；no 形式同步清空转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4 ----------
    row!(
        Dialect::Ios,
        "if/ip/nat/inside",
        "lit:inside|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：inside/outside、静态映射与 EasyIP 预检后提交；no 形式同步清空转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4 ----------
    row!(
        Dialect::Ios,
        "if/ip/nat/outside",
        "lit:outside|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：inside/outside、静态映射与 EasyIP 预检后提交；no 形式同步清空转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4 ----------
    row!(
        Dialect::Ios,
        "config/ip/nat/inside/source/static/@0ipv4/@0ipv4",
        "param:ipv4|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：inside/outside、静态映射与 EasyIP 预检后提交；no 形式同步清空转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4 ----------
    row!(
        Dialect::Ios,
        "config/ip/nat/inside/source/list/@0number/interface/@0if-name",
        "param:if-name|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：inside/outside、静态映射与 EasyIP 预检后提交；no 形式同步清空转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4 ----------
    row!(
        Dialect::Ios,
        "config/no/ip/nat/inside/source/static/@0ipv4/@0ipv4",
        "param:ipv4|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：inside/outside、静态映射与 EasyIP 预检后提交；no 形式同步清空转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4 ----------
    row!(
        Dialect::Ios,
        "if/no/ip/nat/inside",
        "lit:inside|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：inside/outside、静态映射与 EasyIP 预检后提交；no 形式同步清空转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4 ----------
    row!(
        Dialect::Ios,
        "if/no/ip/nat/outside",
        "lit:outside|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：inside/outside、静态映射与 EasyIP 预检后提交；no 形式同步清空转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4 ----------
    row!(
        Dialect::Ios,
        "exec/show/ip/nat/translations",
        "lit:translations|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：inside/outside、静态映射与 EasyIP 预检后提交；no 形式同步清空转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4 ----------
    row!(
        Dialect::Ios,
        "priv/show/ip/nat/translations",
        "lit:translations|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：inside/outside、静态映射与 EasyIP 预检后提交；no 形式同步清空转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C7 ----------
    row!(
        Dialect::Ios,
        "config/router/ospf/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C7：真实进程号/区域、区域不匹配不建邻；no 撤销配置/邻居/派生路由",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C7 ----------
    row!(
        Dialect::Ios,
        "ospf/network/@0ipv4/@0wildcard/area/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C7：真实进程号/区域、区域不匹配不建邻；no 撤销配置/邻居/派生路由",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C7 ----------
    row!(
        Dialect::Ios,
        "config/no/router/ospf/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C7：真实进程号/区域、区域不匹配不建邻；no 撤销配置/邻居/派生路由",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C7 ----------
    row!(
        Dialect::Ios,
        "ospf/exit",
        "lit:exit|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C7：真实进程号/区域、区域不匹配不建邻；no 撤销配置/邻居/派生路由",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C7 ----------
    row!(
        Dialect::Ios,
        "ospf/end",
        "lit:end|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C7：真实进程号/区域、区域不匹配不建邻；no 撤销配置/邻居/派生路由",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C7 ----------
    row!(
        Dialect::Ios,
        "exec/show/ip/ospf/neighbor",
        "lit:neighbor|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C7：真实进程号/区域、区域不匹配不建邻；no 撤销配置/邻居/派生路由",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C7 ----------
    row!(
        Dialect::Ios,
        "priv/show/ip/ospf/neighbor",
        "lit:neighbor|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C7：真实进程号/区域、区域不匹配不建邻；no 撤销配置/邻居/派生路由",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C7 ----------
    row!(
        Dialect::Ios,
        "exec/show/ip/ospf/database",
        "lit:database|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C7：真实进程号/区域、区域不匹配不建邻；no 撤销配置/邻居/派生路由",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C7 ----------
    row!(
        Dialect::Ios,
        "priv/show/ip/ospf/database",
        "lit:database|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C7：真实进程号/区域、区域不匹配不建邻；no 撤销配置/邻居/派生路由",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C8 ----------
    row!(
        Dialect::Ios,
        "if/vrrp/@0number/ip/@0ipv4",
        "param:ipv4|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C8：按优先级与接口 IP 选举 Master/Backup；抢占可关闭；no 删除组",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C8 ----------
    row!(
        Dialect::Ios,
        "if/vrrp/@0number/priority/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C8：按优先级与接口 IP 选举 Master/Backup；抢占可关闭；no 删除组",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C8 ----------
    row!(
        Dialect::Ios,
        "if/vrrp/@0number/preempt",
        "lit:preempt|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C8：按优先级与接口 IP 选举 Master/Backup；抢占可关闭；no 删除组",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C8 ----------
    row!(
        Dialect::Ios,
        "if/no/vrrp/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C8：按优先级与接口 IP 选举 Master/Backup；抢占可关闭；no 删除组",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C8 ----------
    row!(
        Dialect::Ios,
        "if/no/vrrp/@0number/preempt",
        "lit:preempt|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C8：按优先级与接口 IP 选举 Master/Backup；抢占可关闭；no 删除组",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C8 ----------
    row!(
        Dialect::Ios,
        "exec/show/vrrp",
        "lit:vrrp|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C8：按优先级与接口 IP 选举 Master/Backup；抢占可关闭；no 删除组",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C8 ----------
    row!(
        Dialect::Ios,
        "priv/show/vrrp",
        "lit:vrrp|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C8：按优先级与接口 IP 选举 Master/Backup；抢占可关闭；no 删除组",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2 ----------
    row!(
        Dialect::Vrp,
        "config/stp",
        "lit:stp|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：STP 优先级/根桥/端口角色由真实状态重算；undo 恢复默认并重新收敛",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2 ----------
    row!(
        Dialect::Vrp,
        "config/stp/priority/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：STP 优先级/根桥/端口角色由真实状态重算；undo 恢复默认并重新收敛",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2 ----------
    row!(
        Dialect::Vrp,
        "config/stp/root/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：STP 优先级/根桥/端口角色由真实状态重算；undo 恢复默认并重新收敛",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2 ----------
    row!(
        Dialect::Vrp,
        "config/stp/disable",
        "lit:disable|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：STP 优先级/根桥/端口角色由真实状态重算；undo 恢复默认并重新收敛",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2 ----------
    row!(
        Dialect::Vrp,
        "config/undo/stp/priority",
        "lit:priority|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：STP 优先级/根桥/端口角色由真实状态重算；undo 恢复默认并重新收敛",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2 ----------
    row!(
        Dialect::Vrp,
        "exec/display/stp",
        "lit:stp|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：STP 优先级/根桥/端口角色由真实状态重算；undo 恢复默认并重新收敛",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2 ----------
    row!(
        Dialect::Vrp,
        "exec/display/stp/brief",
        "lit:brief|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：STP 优先级/根桥/端口角色由真实状态重算；undo 恢复默认并重新收敛",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2 ----------
    row!(
        Dialect::Vrp,
        "config/display/stp",
        "lit:stp|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：STP 优先级/根桥/端口角色由真实状态重算；undo 恢复默认并重新收敛",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2 ----------
    row!(
        Dialect::Vrp,
        "config/display/stp/brief",
        "lit:brief|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：STP 优先级/根桥/端口角色由真实状态重算；undo 恢复默认并重新收敛",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2 ----------
    row!(
        Dialect::Vrp,
        "if/display/stp",
        "lit:stp|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：STP 优先级/根桥/端口角色由真实状态重算；undo 恢复默认并重新收敛",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2 ----------
    row!(
        Dialect::Vrp,
        "if/display/stp/brief",
        "lit:brief|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：STP 优先级/根桥/端口角色由真实状态重算；undo 恢复默认并重新收敛",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "config/interface/eth-trunk/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "if/eth-trunk/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "if/mode/lacp-static",
        "lit:lacp-static|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "if/mode/manual/load-balance",
        "lit:load-balance|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "if/undo/eth-trunk",
        "lit:eth-trunk|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "config/undo/eth-trunk",
        "lit:eth-trunk|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "exec/display/eth-trunk",
        "lit:eth-trunk|p[0.number]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "exec/display/eth-trunk/@0number",
        "param:number|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "config/display/eth-trunk",
        "lit:eth-trunk|p[0.number]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "config/display/eth-trunk/@0number",
        "param:number|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "if/display/eth-trunk",
        "lit:eth-trunk|p[0.number]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H3 ----------
    row!(
        Dialect::Vrp,
        "if/display/eth-trunk/@0number",
        "param:number|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H3：成员加入/移除与 LACP Selected 真实协商；前置条件失败拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "config/ospf/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf/area/@0word",
        "param:word|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf_area/network/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf/undo/area/@0word",
        "param:word|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf/undo/network/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf_area/undo/network/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "config/undo/ospf/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf/return",
        "lit:return|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf_area/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf_area/return",
        "lit:return|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "exec/display/ospf/peer",
        "lit:peer|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "exec/display/ospf/lsdb",
        "lit:lsdb|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "exec/display/ospfv3/peer",
        "lit:peer|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "exec/display/ospfv3/lsdb",
        "lit:lsdb|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "config/display/ospf/peer",
        "lit:peer|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "config/display/ospf/lsdb",
        "lit:lsdb|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "config/display/ospfv3/peer",
        "lit:peer|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "config/display/ospfv3/lsdb",
        "lit:lsdb|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "if/display/ospf/peer",
        "lit:peer|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "if/display/ospf/lsdb",
        "lit:lsdb|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "if/display/ospfv3/peer",
        "lit:peer|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "if/display/ospfv3/lsdb",
        "lit:lsdb|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf/display/ospf/peer",
        "lit:peer|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf/display/ospf/lsdb",
        "lit:lsdb|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf/display/ospfv3/peer",
        "lit:peer|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf/display/ospfv3/lsdb",
        "lit:lsdb|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf_area/display/ospf/peer",
        "lit:peer|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf_area/display/ospf/lsdb",
        "lit:lsdb|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf_area/display/ospfv3/peer",
        "lit:peer|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H6 ----------
    row!(
        Dialect::Vrp,
        "ospf_area/display/ospfv3/lsdb",
        "lit:lsdb|p[]",
        Effect::Query,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H6：双层视图层级、真实进程/区域/network；undo 撤销后协议状态同步",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C4：EasyIP 撤销（§5.4 适用 no 形式） ----------
    row!(
        Dialect::Ios,
        "config/no/ip/nat/inside/source/list/@0number/interface/@0if-name",
        "param:if-name|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：EasyIP 撤销须完整预检 ACL/接口与当前绑定一致，拒绝则状态不变；提交后清空派生转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Ios,
        "config/no/ip/nat/inside/source/list/@0number/interface/@0if-name/overload",
        "lit:overload|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C4：EasyIP 撤销须完整预检 ACL/接口与当前绑定一致，拒绝则状态不变；提交后清空派生转换表",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / C7：单条 network 撤销（§5.5 适用 no 形式） ----------
    row!(
        Dialect::Ios,
        "ospf/no/network/@0ipv4/@0wildcard/area/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "C7：no network 校验区域与配置一致，撤销后配置/邻居/路由/数据面同步恢复",
        &[CISCO_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2B / H2：undo stp root（§5.7 适用 undo） ----------
    row!(
        Dialect::Vrp,
        "config/undo/stp/root",
        "lit:root|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：undo stp root 恢复默认优先级并重新选举根桥，端口角色随协议状态重算",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "if/undo/stp/root",
        "lit:root|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H2：undo stp root 恢复默认优先级并重新选举根桥，端口角色随协议状态重算",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2C / H4：VRP BGP 邻居、宣告、路由与撤销 ----------
    row!(
        Dialect::Vrp,
        "config/bgp/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：进入 BGP 视图并写入真实本端 AS；非法 AS 拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "bgp/peer/@0ipv4/as-number/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：配置 eBGP 邻居 AS/地址，会话与跨 AS 路由真实派生；错误 AS/无邻居不建邻",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "bgp/network/@0ipv4/mask/@0dotted-mask",
        "param:dotted-mask|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：network 宣告真实前缀（mask 关键字形态）；未宣告前缀不出现在对端路由表",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "bgp/network/@0ipv4/@0dotted-mask",
        "param:dotted-mask|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：network 宣告真实前缀（点分掩码形态）；未宣告前缀不出现在对端路由表",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "bgp/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：退出 BGP 视图，只改会话模式",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "bgp/return",
        "lit:return|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：返回用户视图，只改会话模式",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/bgp/peer",
        "lit:peer|p[]",
        Effect::Query,
        Some(Capability::RouteTableView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：display bgp peer 取真实会话状态（邻居/对端 AS/Established），纯读",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/bgp/routing-table",
        "lit:routing-table|p[]",
        Effect::Query,
        Some(Capability::RouteTableView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：display bgp routing-table 取真实学习路由（前缀/AS_PATH/下一跳/LocPrf），纯读",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/undo/bgp/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：undo bgp 校验 AS 一致后撤销进程，清空会话/路由；不匹配拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "bgp/undo/peer/@0ipv4",
        "param:ipv4|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：undo peer 删除邻居及其策略绑定，会话/路由同步撤销；邻居不存在拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "bgp/undo/network/@0ipv4/mask/@0dotted-mask",
        "param:dotted-mask|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：undo network 撤销宣告（mask 关键字形态），对端经 Update 全表刷新撤销前缀",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "bgp/undo/network/@0ipv4/@0dotted-mask",
        "param:dotted-mask|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H4：undo network 撤销宣告（点分掩码形态），对端经 Update 全表刷新撤销前缀",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2C / H5：VRP ACL 作为 route-policy 匹配条件 ----------
    row!(
        Dialect::Vrp,
        "config/acl/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：进入 ACL 视图（编号形态），非法编号拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/acl/@1word/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：进入 ACL 视图（basic/advanced 形态），非法编号拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "acl/rule/@0word/source/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：VRP ACL 规则首条命中/隐式拒绝，通配符参与路由前缀匹配",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "acl/rule/@0word/source/@0ipv4/@0wildcard/destination/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：VRP ACL 规则（含 destination）首条命中/隐式拒绝",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "acl/rule/@0word/icmp/source/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：VRP ACL ICMP 规则首条命中/隐式拒绝",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "acl/rule/@0word/icmp/source/@0ipv4/@0wildcard/destination/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：VRP ACL ICMP 规则（含 destination）首条命中/隐式拒绝",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "acl/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：退出 ACL 视图，只改会话模式",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/acl",
        "lit:acl|p[]",
        Effect::Query,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：display acl 取真实 ACL 规则与命中计数，纯读",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/acl/all",
        "lit:all|p[]",
        Effect::Query,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：display acl all 取真实全部 ACL 规则，纯读",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/undo/acl/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：undo acl 删除整条 ACL 并解除接口绑定、重算策略；ACL 不存在拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2C / H5：VRP Route-Policy 节点与 apply ----------
    row!(
        Dialect::Vrp,
        "config/route-policy/@0word/@0word/node/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：进入 route-policy 节点视图，seq 升序首命中；非法序号/动作拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "route_policy/if-match/acl/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：if-match acl 引用真实 ACL 作为匹配条件；引用不存在 ACL 拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "route_policy/apply/local-preference/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：apply local-preference 真实影响 BGP 选路（非仅渲染）",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "route_policy/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：退出 route-policy 视图，只改会话模式",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "route_policy/return",
        "lit:return|p[]",
        Effect::ModeSwitch,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：返回用户视图，只改会话模式",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/route-policy",
        "lit:route-policy|p[]",
        Effect::Query,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：display route-policy 取真实节点/匹配/apply 属性，纯读",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/undo/route-policy/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：undo route-policy 删除整条策略并重算路由；策略不存在拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/undo/route-policy/@0word/node/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：undo route-policy node 删除单节点（保留策略名）；节点不存在拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "route_policy/undo/if-match",
        "lit:if-match|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：undo if-match 清除当前节点匹配条件并重算路由",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "route_policy/undo/if-match/acl",
        "lit:acl|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：undo if-match acl 清除当前节点 ACL 匹配并重算路由",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "route_policy/undo/apply",
        "lit:apply|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：undo apply 清除当前节点 apply 属性并重算选路",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "route_policy/undo/apply/local-preference",
        "lit:local-preference|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：undo apply local-preference 清除本地优先级并重算选路",
        &[VRP_CFG, ADR_D12],
        true
    ),
    // ---------- Stage 2C / H5：VRP BGP 邻居策略绑定与解绑 ----------
    row!(
        Dialect::Vrp,
        "bgp/peer/@0ipv4/route-policy/@0word/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：peer route-policy import/export 真实过滤前缀；引用不存在策略/邻居拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),
    row!(
        Dialect::Vrp,
        "bgp/undo/peer/@0ipv4/route-policy/@0word/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        PENDING_OUTPUT,
        "H5：undo peer route-policy 解除绑定后按协议重算路由；绑定不存在拒绝且状态不变",
        &[VRP_CFG, ADR_D12],
        true
    ),

    // ---------- 阶段 6B：Cisco IOS XE 17.x ZBF（完整数据面） ----------
    // 官方全文核验（ZBF_CFG，访问 2026-11-16）：
    //   Device(config)# class-map type inspect {match-any|match-all} <name> -> config-cmap
    //   Device(config-cmap)# match protocol <name> / match access-group <n>
    //   Device(config)# policy-map type inspect <name>                    -> config-pmap
    //   Device(config-pmap)# class type inspect <cm> | class class-default -> config-pmap-c
    //   Device(config-pmap-c)# inspect [pmap] | pass | drop [log]
    //   Device(config)# zone-pair security <name> source <src> destination <dst>
    //                                                                     -> config-sec-zone-pair
    //   Device(config-sec-zone-pair)# service-policy type inspect <policy>
    // 判定统一走 `firewall_allow` / `firewall_tcp_allow` 与既有会话表：
    // inspect 建会话、pass 无状态、drop 丢弃、未配置策略即 drop。
    row!(
        Dialect::Ios,
        "config/class-map/type/inspect/match-any/@0word",
        "param:word|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 创建/进入 inspect class-map（match-any）(config-cmap)#；match-any 表示任一 match 命中",
        &[ZBF_CFG, ZBF_ALG_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/class-map/type/inspect/match-all/@0word",
        "param:word|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 创建/进入 inspect class-map（match-all）(config-cmap)#；match-all 要求全部 match 命中",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "class_map/match/protocol/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: match protocol <name>；数据面实现 ICMP/TCP 与常见 TCP 服务端口，L7/ALG 显式未模拟",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "class_map/no/match/protocol/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 否定 match protocol；仅移除该条件",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "class_map/match/access-group/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: match access-group <number>，复用 ACL 5 元组首命中判定",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "class_map/no/match/access-group/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 否定 match access-group",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "class_map/exit",
        "lit:exit|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: (config-cmap)# exit → (config)#",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "class_map/end",
        "lit:end|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: (config-cmap)# end → 特权模式",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/policy-map/type/inspect/@0word",
        "param:word|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 创建/进入 inspect policy-map (config-pmap)#",
        &[ZBF_CFG, ZBF_ALG_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map/class/type/inspect/@0word",
        "param:word|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: class type inspect <cmap> 进入 (config-pmap-c)#；引用不存在的 class-map 拒绝",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map/class/class-default",
        "lit:class-default|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 系统类 class-default（始终最后）；未配置动作时缺省 drop",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map/no/class/type/inspect/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 从 policy-map 删除一个 class",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map/no/class/class-default",
        "lit:class-default|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 删除 class-default",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map/exit",
        "lit:exit|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: (config-pmap)# exit → (config)#",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map/end",
        "lit:end|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: (config-pmap)# end → 特权模式",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map_class/inspect",
        "lit:inspect|p[0.word]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: inspect [parameter-map] 状态化检查；首包建会话、回包走会话表",
        &[ZBF_CFG, ZBF_ALG_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map_class/pass",
        "lit:pass|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: pass 无状态放行，不建立会话；反向流量需镜像策略",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map_class/drop",
        "lit:drop|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: drop 丢弃命中流量",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map_class/drop/log",
        "lit:log|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: drop log（仅保存日志意图；模拟器不产生日志）",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map_class/no/inspect",
        "lit:inspect|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 否定 inspect 动作",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map_class/no/pass",
        "lit:pass|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 否定 pass 动作",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map_class/no/drop",
        "lit:drop|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 否定 drop 动作",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map_class/no/drop/log",
        "lit:log|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: no drop log（官方排障示例），只关闭日志意图",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map_class/exit",
        "lit:exit|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: (config-pmap-c)# exit → (config-pmap)#",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "policy_map_class/end",
        "lit:end|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: (config-pmap-c)# end → 特权模式",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/zone-pair/security/@0word/source/@0word/destination/@0word",
        "param:word|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: zone-pair security <name> source <src> destination <dst> → (config-sec-zone-pair)#；区域必须存在，self 可用",
        &[ZBF_CFG, ZBF_SCCP_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "security_rule/service-policy/type/inspect/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 在 config-sec-zone-pair 挂接 policy-map；策略必须存在",
        &[ZBF_CFG, ZBF_SCCP_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "security_rule/no/service-policy/type/inspect/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 解除 zone-pair 的策略挂接（之后按官方 drop）",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "security_rule/exit",
        "lit:exit|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: (config-sec-zone-pair)# exit → (config)#",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "security_rule/end",
        "lit:end|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: (config-sec-zone-pair)# end → 特权模式",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/no/zone/security/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: no zone security <name> 删除区域并清除接口成员关系",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/no/zone-pair/security/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: no zone-pair security <name> 删除区域对",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/no/class-map/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: no class-map <name>；仍被 policy-map 引用时拒绝删除",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/no/class-map/type/inspect/match-any/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: no class-map type inspect match-any <name>",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/no/class-map/type/inspect/match-all/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: no class-map type inspect match-all <name>",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/no/policy-map/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: no policy-map <name>；仍挂在 zone-pair 上时拒绝删除",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/no/policy-map/type/inspect/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: no policy-map type inspect <name>",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "config/zone/security/@0word",
        "param:word|p[]",
        Effect::ModeSwitch,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: zone security <name> 创建/进入区域视图 (config-sec-zone)#",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "if/zone-member/security/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 接口 zone-member security <zone> 写入唯一成员关系 Port::zone",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "if/no/zone-member/security/@0word",
        "param:word|p[]",
        Effect::StateChange,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: 接口移出安全区域",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "exec/show/zone/security",
        "lit:security|p[]",
        Effect::Query,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: show zone security 读取唯一区域/成员状态",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "exec/show/zone-pair/security",
        "lit:security|p[]",
        Effect::Query,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: show zone-pair security 列出方向与挂接的 inspect policy",
        &[ZBF_CFG, ZBF_SHOW_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "exec/show/policy-map/type/inspect/zone-pair",
        "lit:zone-pair|p[0.word]",
        Effect::Query,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: show policy-map type inspect zone-pair [name] 结构化输出与命中计数",
        &[ZBF_SHOW_CFG, ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "exec/show/policy-map/type/inspect/zone-pair/session",
        "lit:session|p[]",
        Effect::Query,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: show policy-map type inspect zone-pair session 追加会话计数（E1 步骤语法）",
        &[ZBF_CFG],
        true
    ),
    row!(
        Dialect::Ios,
        "exec/show/policy-map/type/inspect/zone-pair/sessions",
        "lit:sessions|p[]",
        Effect::Query,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: sessions 复数别名（Cisco ZBF 设计指南语法）",
        &[ZBF_CFG, ZBF_DESIGN_REF],
        true
    ),
    row!(
        Dialect::Ios,
        "exec/show/policy-map/type/inspect/zone-pair/@0word/session",
        "lit:session|p[]",
        Effect::Query,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: [zone-pair-name] [sessions] 组合形式受支持；按区域对名过滤并追加会话计数",
        &[ZBF_CFG, ZBF_DESIGN_REF],
        true
    ),
    row!(
        Dialect::Ios,
        "exec/show/policy-map/type/inspect/zone-pair/@0word/sessions",
        "lit:sessions|p[]",
        Effect::Query,
        Some(Capability::ZoneFirewall),
        SemanticSupport::Implemented,
        DOC,
        "ZBF: [zone-pair-name] [sessions] 组合形式（复数别名）",
        &[ZBF_CFG, ZBF_DESIGN_REF],
        true
    ),

    // ---------- 阶段 6B：ZBF 邻接但未模拟（显式 NotSimulated） ----------
    row!(
        Dialect::Ios,
        "config/parameter-map/type/inspect/@0word",
        "param:word|p[]",
        Effect::Unimplemented,
        None,
        SemanticSupport::NotSimulated,
        DOC,
        "ZBF: parameter-map type inspect 的参数（tcp idle-time 等）未模拟；inspect [pmap] 只保存引用",
        &[ZBF_CFG],
        false
    ),
    row!(
        Dialect::Ios,
        "class_map/match/class-map/@0word",
        "param:word|p[]",
        Effect::Unimplemented,
        None,
        SemanticSupport::NotSimulated,
        DOC,
        "ZBF: 嵌套 match class-map 未模拟",
        &[ZBF_CFG],
        false
    ),
    row!(
        Dialect::Ios,
        "security_rule/description/@0word-list",
        "param:word-list|p[]",
        Effect::Unimplemented,
        None,
        SemanticSupport::NotSimulated,
        DOC,
        "ZBF: zone-pair description 未模拟（不参与判定）",
        &[ZBF_CFG],
        false
    ),
    row!(
        Dialect::Ios,
        "security_rule/platform/inspect/match-statistics/per-filter",
        "lit:per-filter|p[]",
        Effect::Unimplemented,
        None,
        SemanticSupport::NotSimulated,
        DOC,
        "ZBF: platform inspect match-statistics per-filter 未模拟",
        &[ZBF_CFG],
        false
    ),
    row!(
        Dialect::Ios,
        "policy_map_class/service-policy/type/inspect/@0word",
        "param:word|p[]",
        Effect::Unimplemented,
        None,
        SemanticSupport::NotSimulated,
        DOC,
        "ZBF: 层次化 policy-map 未模拟",
        &[ZBF_CFG],
        false
    ),

    // ---------- 阶段 6A：VRP 防火墙安全策略基线首批闭环 ----------
    // 基线决策见 docs/cli-fidelity/stage6a-implementation-record.md §2：
    // 选择华为 VRP 安全策略（V1–V9 官方正文证据），Cisco ASA 血统
    // （nameif / security-level / 安全级别回退）降级为教学抽象并移除 CLI 入口。
    // 全部首批命令 `Implemented + DocumentationOnly + strict`；无带版本设备样本，
    // 因此只承诺配置落库、派生状态与查询/配置视图，不承诺厂商逐字文本。
    //
    // 设备级能力由 handler 的 `require_firewall` 门禁（仅 DeviceKind::Firewall 可执行）；
    // 本表是**平台级**（VRP 方言）结论，与 cmdmap 的平台对照语义一致。
    row!(
        Dialect::Vrp,
        "config/firewall/zone/@0word",
        "param:word|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V5: firewall zone <name> 创建/进入安全区域视图 [HUAWEI-zone-<name>]；预定义区域优先级 local 100 / trust 85 / dmz 50 / untrust 5",
        &[HW_ZONE_CMDS, HW_FIREWALL_CHECK],
        true
    ),
    row!(
        Dialect::Vrp,
        "zone/add/interface/@0if-name",
        "param:if-name|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V5: add interface 把三层接口加入当前安全区域；一个接口只属于一个区域，配置视图与 display firewall zone 同步",
        &[HW_ZONE_CMDS, HW_FIREWALL_CHECK],
        true
    ),
    row!(
        Dialect::Vrp,
        "zone/undo/add/interface/@0if-name",
        "param:if-name|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V5: undo add interface 把接口移出当前安全区域；接口不属于该区域时拒绝且状态不变",
        &[HW_ZONE_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "zone/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V5: 安全区域视图 quit 退回系统视图",
        &[HW_ZONE_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "zone/return",
        "lit:return|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V5: 安全区域视图 return 直达用户视图",
        &[HW_ZONE_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/security-policy",
        "lit:security-policy|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: security-policy 进入安全策略视图 [HUAWEI-policy-security]",
        &[HW_SECPOLICY_CMDS, HW_SECPOLICY_DEFAULTS],
        true
    ),
    row!(
        Dialect::Vrp,
        "config/security-policy/ip",
        "lit:ip|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: 安全策略视图的 ip 关键字（VRP 与 Comware 均接受）进入同一安全策略视图",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_policy/rule/name/@0word",
        "param:word|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V7/V9: rule name <n> 创建或进入安全策略规则视图；新规则缺省启用、动作为 deny、条件为空",
        &[HW_SECPOLICY_CMDS, HW_RULE_ADJUST],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_policy/rule/move/@0word/@0word/@0word",
        "param:word|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V7: rule move <name1> { before | after } <name2> 调整规则顺序；规则按列表顺序首条命中",
        &[HW_RULE_ADJUST],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_policy/undo/rule/name/@0word",
        "param:word|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: undo rule name <n> 删除规则；规则不存在时拒绝且状态不变",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_policy/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: 安全策略视图 quit 退回系统视图",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_policy/return",
        "lit:return|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: 安全策略视图 return 直达用户视图",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_rule/source-zone/@0word",
        "param:word|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: 规则视图 source-zone <zone> 设置源安全区域匹配条件",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_rule/destination-zone/@0word",
        "param:word|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: 规则视图 destination-zone <zone> 设置目的安全区域匹配条件",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_rule/action/@0word",
        "param:word|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: 规则视图 action { permit | deny } 设置动作",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_rule/enable",
        "lit:enable|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V7: 规则视图 enable 启用本规则（缺省启用）",
        &[HW_RULE_ADJUST],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_rule/disable",
        "lit:disable|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V7: 规则视图 disable 禁用本规则；禁用规则不参与匹配，display 的 STATE 列显示 disable",
        &[HW_RULE_ADJUST],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_rule/undo/source-zone",
        "lit:source-zone|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: undo source-zone 清空源区域条件（规则保留但不再命中）",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_rule/undo/destination-zone",
        "lit:destination-zone|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: undo destination-zone 清空目的区域条件",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_rule/undo/action",
        "lit:action|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: undo action 复位规则动作",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_rule/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: 规则视图 quit 退回安全策略视图（不直达系统视图）",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "security_rule/return",
        "lit:return|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: 规则视图 return 直达用户视图",
        &[HW_SECPOLICY_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/firewall/zone",
        "lit:zone|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V6: display firewall zone 输出区域优先级与成员接口，数据来自唯一状态源；无逐字输出黄金",
        &[HW_FIREWALL_CHECK, HW_ZONE_CMDS],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/firewall/session/table",
        "lit:table|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V3/V9: display firewall session table 输出 ICMP 会话（源/目的 IP + echo id）与 TCP 会话（双向五元组）；只做结构断言",
        &[HW_SESSION_CLEAR, HW_SECPOLICY_ENCYCLOPEDIA, HW_SESSION_AGING],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/reset/firewall/session/table",
        "lit:table|p[]",
        Effect::Session,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V3: reset firewall session table 清除会话表，使策略变更对既有流立即生效（否则既有流继续转发到老化）",
        &[HW_SESSION_CLEAR],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/security-policy",
        "lit:security-policy|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9/V4: display security-policy 按 RULE ID / RULE NAME / STATE / ACTION / HITS 列输出，并显示平台缺省策略 default（any/any → deny）",
        &[HW_SECPOLICY_CMDS, HW_SECPOLICY_CHECK, HW_SECPOLICY_DEFAULTS],
        true
    ),
    row!(
        Dialect::Vrp,
        "exec/display/security-policy/rule/name/@0word",
        "param:word|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        DOC,
        "V9: display security-policy rule name <n> 输出 `(<hits> times matched) rule name <n>` 与 source-zone / destination-zone / action 明细",
        &[HW_SECPOLICY_CMDS],
        true
    ),

    // ---------- Comware V7 / 阶段 4A 首批原生闭环 ----------
    // 语法与模式由 comware::build() 的 CNode 派生；这里只挂能力、语义、证据与承诺契约。
    // 证据均为 H3C 官方正文（DocumentationOnly），无带版本设备样本 → 逐字输出仍待验证，
    // 故契约只承诺配置落库、派生状态与查询/配置视图，不承诺厂商逐字文本。
    row!(
        Dialect::Comware,
        "config/vlan",
        "lit:vlan|p[0.number]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E1/E11: vlan <id> 创建 VLAN（1-4094）并进入 VLAN 视图 [Sysname-vlan<id>]；quit 退回系统视图",
        &[CW_L2, CW_EX],
        true
    ),
    row!(
        Dialect::Comware,
        "vlan/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "E6/E11: VLAN 视图 quit 退回上一级系统视图（不直达用户视图）",
        &[CW_EX],
        true
    ),
    row!(
        Dialect::Comware,
        "vlan/return",
        "lit:return|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "E6/E11: VLAN 视图 return 从任意非用户视图直达用户视图",
        &[CW_EX],
        true
    ),
    row!(
        Dialect::Comware,
        "config/undo/vlan/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E1: undo vlan <id> 删除 VLAN；VLAN1 不可删除，同步清理 SVI/端口/表项",
        &[CW_L2],
        true
    ),
    row!(
        Dialect::Comware,
        "config/interface",
        "lit:interface|p[0.if-name]",
        Effect::ModeSwitch,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E3: interface GigabitEthernet1/0/x 进入接口视图；只改会话模式",
        &[CW_EX],
        true
    ),
    row!(
        Dialect::Comware,
        "config/interface/vlan-interface/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E1/E3: interface Vlan-interface <id> 进入 SVI 视图；须先创建 VLAN",
        &[CW_L2, CW_EX],
        true
    ),
    row!(
        Dialect::Comware,
        "if/port/link-type/access",
        "lit:access|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E1: port link-type access；端口默认即为 Access",
        &[CW_L2],
        true
    ),
    row!(
        Dialect::Comware,
        "if/port/link-type/trunk",
        "lit:trunk|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E1/E2: port link-type trunk；初始放行列表仅 VLAN1（平台策略显式注入）",
        &[CW_L2, CW_SW],
        true
    ),
    row!(
        Dialect::Comware,
        "if/port/access/vlan/@0number",
        "param:number|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E1/E3: port access vlan <id> 设置 Access 口与 PVID；display vlan / 配置视图可见",
        &[CW_L2, CW_EX],
        true
    ),
    row!(
        Dialect::Comware,
        "if/undo/port/access/vlan",
        "lit:vlan|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E1: undo port access vlan 恢复默认 VLAN1；与 port access vlan 共用状态层",
        &[CW_L2],
        true
    ),
    row!(
        Dialect::Comware,
        "if/port/trunk/permit/vlan/@0vlan-list",
        "param:vlan-list|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E1/E2/E10: port trunk permit vlan <list> 累加放行且保留 VLAN1；须先 link-type trunk；负向流量被丢弃",
        &[CW_L2, CW_SW, CW_EX],
        true
    ),
    row!(
        Dialect::Comware,
        "if/undo/port/trunk/permit/vlan/@0vlan-list",
        "param:vlan-list|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E1/E10: undo port trunk permit vlan <list> 移除指定放行 VLAN 并按策略保留 VLAN1",
        &[CW_L2],
        true
    ),
    row!(
        Dialect::Comware,
        "if/undo/port/trunk/permit/vlan/all",
        "lit:all|p[]",
        Effect::StateChange,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E1/E10: undo port trunk permit vlan all 恢复平台默认放行列表（仅 VLAN1）",
        &[CW_L2],
        true
    ),
    row!(
        Dialect::Comware,
        "if/ip/address",
        "lit:address|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "E1/E3: ip address <ip> <mask-length> 写入 SVI/三层口；配置视图与路由表可见",
        &[CW_L2, CW_EX],
        true
    ),
    row!(
        Dialect::Comware,
        "if/shutdown",
        "lit:shutdown|p[]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "E1: shutdown 关闭 VLAN 接口/物理口，写入管理状态",
        &[CW_L2],
        true
    ),
    row!(
        Dialect::Comware,
        "if/undo/shutdown",
        "lit:shutdown|p[]",
        Effect::StateChange,
        Some(Capability::L3InterfaceAddressing),
        SemanticSupport::Implemented,
        DOC,
        "E1: undo shutdown 恢复管理状态；SVI 默认未被手工关闭",
        &[CW_L2],
        true
    ),
    row!(
        Dialect::Comware,
        "config/ip/route-static",
        "lit:route-static|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        DOC,
        "E3/E7: ip route-static 完整预检后写入静态路由；display ip routing-table 与正/负 Ping 可验证",
        &[CW_EX, CW_ROUTE],
        true
    ),
    row!(
        Dialect::Comware,
        "config/undo/ip/route-static",
        "lit:route-static|p[0.ipv4]",
        Effect::StateChange,
        Some(Capability::StaticRouting),
        SemanticSupport::Implemented,
        DOC,
        "E7: undo ip route-static 按同一匹配键撤销静态路由，路由表与 Ping 反映变化",
        &[CW_ROUTE],
        true
    ),
    row!(
        Dialect::Comware,
        "exec/display/current-configuration",
        "lit:current-configuration|p[]",
        Effect::Query,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        DOC,
        "E5/ADR D12: 从 DevSummary 实时渲染 Comware 原生配置，输出可重执行；逐字输出待验证",
        &[CW_CFG_VIEW, ADR_D12],
        true
    ),
    row!(
        Dialect::Comware,
        "exec/display/vlan",
        "lit:vlan|p[]",
        Effect::Query,
        Some(Capability::L2SwitchPort),
        SemanticSupport::Implemented,
        DOC,
        "E1: display vlan 读取 VLAN/端口状态；逐字输出待验证，采用结构断言",
        &[CW_L2],
        true
    ),
    row!(
        Dialect::Comware,
        "exec/display/ip/routing-table",
        "lit:routing-table|p[]",
        Effect::Query,
        Some(Capability::RouteTableView),
        SemanticSupport::Implemented,
        DOC,
        "E7: display ip routing-table 读取真实静态/直连路由；逐字输出待验证",
        &[CW_ROUTE],
        true
    ),
    row!(
        Dialect::Comware,
        "exec/display/ip/interface/brief",
        "lit:brief|p[]",
        Effect::Query,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        DOC,
        "迁移期兼容查询：从 DevSummary 读取接口摘要；Comware 官方形式为 display interface brief",
        &[CW_CFG_VIEW],
        false
    ),
    row!(
        Dialect::Comware,
        "exec/display/interface/brief",
        "lit:brief|p[]",
        Effect::Query,
        Some(Capability::ConfigView),
        SemanticSupport::Implemented,
        DOC,
        "E1/E3: display interface brief 读取接口摘要；逐字输出待验证",
        &[CW_L2],
        false
    ),
    row!(
        Dialect::Comware,
        "config/ospf/@0number",
        "param:number|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "E8: ospf <pid> 进入 OSPF 进程视图并保留进程号",
        &[CW_OSPF],
        true
    ),
    row!(
        Dialect::Comware,
        "ospf/area/@0word",
        "param:word|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "E8: area <area-id> 进入 OSPF 区域视图；network 仅在该视图有效",
        &[CW_OSPF],
        true
    ),
    row!(
        Dialect::Comware,
        "ospf_area/network/@0ipv4/@0wildcard",
        "param:wildcard|p[]",
        Effect::StateChange,
        None,
        SemanticSupport::Implemented,
        DOC,
        "E8: network <ip> <wildcard> 写入真实 OSPF 网段；配置视图与邻居/路由能力不回归（不扩展数据面）",
        &[CW_OSPF],
        true
    ),
    row!(
        Dialect::Comware,
        "ospf_area/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "E6: 区域视图 quit 返回同一 OSPF 进程视图（进程号不丢失）",
        &[CW_CLI],
        true
    ),
    row!(
        Dialect::Comware,
        "ospf/quit",
        "lit:quit|p[]",
        Effect::ModeSwitch,
        None,
        SemanticSupport::Implemented,
        DOC,
        "E6: 进程视图 quit 返回系统视图；与区域视图 quit 组成两级退出",
        &[CW_CLI],
        true
    ),
    row!(
        Dialect::Comware,
        "exec/display/ospf/peer",
        "lit:peer|p[]",
        Effect::Query,
        None,
        SemanticSupport::Implemented,
        DOC,
        "E8: display ospf peer 读取真实邻居状态；逐字输出待验证，不扩展数据面",
        &[CW_OSPF],
        false
    ),
    row!(
        Dialect::Comware,
        "if/packet-filter/@0number/@0word",
        "param:word|p[]",
        Effect::Unimplemented,
        None,
        SemanticSupport::NotSimulated,
        DOC,
        "E9: packet-filter <acl> {inbound|outbound} 是真实 Comware 语法，但本包不模拟 ACL 数据面；仅识别、不改状态、不显示完整支持",
        &[CW_PF],
        false
    ),

    // ---------- 阶段 5A：F5 BIG-IP TMOS 17.x / tmsh 首批闭环 ----------
    // 只给有官方证据且已实现的节点标 Implemented；其余节点显式留在 lint 迁移清单，
    // 不伪造完整支持。非 /Common/ 分区与 modify 属性组标 NotSimulated。
    tmos!("exec/ltm", "lit:ltm|p[]", Effect::ModeSwitch, SemanticSupport::Implemented,
        "F1: (tmos)# ltm 进入 ltm 模块，提示符 (tmos.ltm)#", &[F5_TMSH], true),
    tmos!("exec/sys", "lit:sys|p[]", Effect::ModeSwitch, SemanticSupport::Implemented,
        "F1: (tmos)# sys 进入 sys 模块，提示符 (tmos.sys)#", &[F5_TMSH], true),
    tmos!("exec/create/ltm/pool/@0word", "param:word|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F2/F1: create ltm pool <name> 创建 /Common/ 下的负载均衡池", &[F5_POOL, F5_TMSH], true),
    tmos!("exec/create/ltm/pool/@0word/members/add/{/@0word/}", "lit:}|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F2: create pool <name> members add { <ip>:<port> } 建池并加入成员；地址端口错、重复成员被拒", &[F5_POOL], true),
    tmos!("exec/create/ltm/virtual/@0word/destination/@0word/pool/@0word", "param:word|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F3/F1: create ltm virtual <name> destination <ipv4:port> pool <name>；引用不存在的 pool 被拒", &[F5_VIRTUAL, F5_TMSH], true),
    tmos!("exec/modify/ltm/pool/@0word/members/add/{/@0word/}", "lit:}|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F2: modify ltm pool <name> members add { <ip>:<port> } 向已有池添加成员", &[F5_POOL], true),
    tmos!("exec/delete/ltm/pool/@0word", "param:word|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F2: delete ltm pool <name>；被虚拟服务器引用时拒绝", &[F5_POOL], true),
    tmos!("exec/delete/ltm/virtual/@0word", "param:word|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F3: delete ltm virtual <name> 删除虚拟服务器并清理会话", &[F5_VIRTUAL], true),
    tmos!("exec/list/ltm/pool", "lit:pool|p[0.word]", Effect::Query, SemanticSupport::Implemented,
        "F1/F2: list ltm pool 显示池与成员配置；输出只做结构断言", &[F5_TMSH, F5_POOL], true),
    tmos!("exec/list/ltm/virtual", "lit:virtual|p[0.word]", Effect::Query, SemanticSupport::Implemented,
        "F1/F3: list ltm virtual 显示虚拟服务器配置", &[F5_TMSH, F5_VIRTUAL], true),
    tmos!("exec/list/ltm/pool/@0word", "param:word|p[]", Effect::Query, SemanticSupport::Implemented,
        "F2: list ltm pool /Common/<name> 按对象全路径显示指定池", &[F5_POOL, F5_CD], true),
    tmos!("exec/list/ltm/virtual/@0word", "param:word|p[]", Effect::Query, SemanticSupport::Implemented,
        "F3: list ltm virtual /Common/<name> 按对象全路径显示指定虚拟服务器", &[F5_VIRTUAL, F5_CD], true),
    tmos!("exec/show/ltm/pool/@0word", "param:word|p[]", Effect::Query, SemanticSupport::Implemented,
        "F1/F2: show ltm pool /Common/<name> 显示指定池状态与会话分配", &[F5_TMSH, F5_POOL], true),
    tmos!("exec/show/ltm/virtual/@0word", "param:word|p[]", Effect::Query, SemanticSupport::Implemented,
        "F1/F3: show ltm virtual /Common/<name>", &[F5_TMSH, F5_VIRTUAL], true),
    tmos!("exec/show/ltm/pool", "lit:pool|p[0.word]", Effect::Query, SemanticSupport::Implemented,
        "F1: show 显示统计与运行状态；show ltm pool 输出当前会话分配", &[F5_TMSH, F5_POOL], true),
    tmos!("exec/show/ltm/virtual", "lit:virtual|p[0.word]", Effect::Query, SemanticSupport::Implemented,
        "F1/F3: show ltm virtual 显示虚拟服务器状态", &[F5_TMSH, F5_VIRTUAL], true),
    tmos!("exec/show/sys/version", "lit:version|p[]", Effect::Query, SemanticSupport::Implemented,
        "F1: sys 模块 show version；无设备样本，不伪造版本号", &[F5_TMSH], true),
    tmos!("exec/cd//Common", "lit:/Common|p[]", Effect::ModeSwitch, SemanticSupport::Implemented,
        "F6: cd /Common 切换默认工作文件夹", &[F5_CD], true),
    tmos!("exec/cd/@0word", "param:word|p[]", Effect::Unimplemented, SemanticSupport::NotSimulated,
        "F6/§13(9): 其他 Partition / Route Domain 未模拟，返回 NotSimulated 且不改状态", &[F5_CD], false),
    tmos!("exec/pwd", "lit:pwd|p[]", Effect::Query, SemanticSupport::Implemented,
        "F6: pwd 显示当前工作文件夹 /Common", &[F5_CD], true),
    tmos!("exec/help", "lit:help|p[]", Effect::Query, SemanticSupport::Implemented,
        "F1: help 显示 tmsh 模块层级", &[F5_TMSH], true),
    tmos!("exec/exit", "lit:exit|p[]", Effect::ModeSwitch, SemanticSupport::Implemented,
        "F4: exit 退出当前模块/组件返回上一级", &[F5_EXIT], true),
    tmos!("exec/quit", "lit:quit|p[]", Effect::ModeSwitch, SemanticSupport::Implemented,
        "F5: quit 退出 tmsh；模拟器无独立系统 shell 层，统一回到 (tmos)#", &[F5_QUIT], true),

    tmos!("tmos_ltm/create/pool/@0word", "param:word|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F1/F2: (tmos.ltm)# create pool <name> 模块内相对写法", &[F5_TMSH, F5_POOL], true),
    tmos!("tmos_ltm/create/pool/@0word/members/add/{/@0word/}", "lit:}|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F2: 官方示例 create pool my_pool members add { <ip>:<port> }", &[F5_POOL], true),
    tmos!("tmos_ltm/create/virtual/@0word/destination/@0word/pool/@0word", "param:word|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F3/F1: (tmos.ltm)# create virtual <name> destination <ipv4:port> pool <name>", &[F5_VIRTUAL, F5_TMSH], true),
    tmos!("tmos_ltm/modify/pool/@0word/members/add/{/@0word/}", "lit:}|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F2: (tmos.ltm)# modify pool <name> members add { <ip>:<port> }；池不存在被拒", &[F5_POOL], true),
    tmos!("tmos_ltm/delete/pool/@0word", "param:word|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F2: delete pool <name>；被引用时拒绝", &[F5_POOL], true),
    tmos!("tmos_ltm/delete/virtual/@0word", "param:word|p[]", Effect::StateChange, SemanticSupport::Implemented,
        "F3: delete virtual <name>", &[F5_VIRTUAL], true),
    tmos!("tmos_ltm/list/pool", "lit:pool|p[0.word]", Effect::Query, SemanticSupport::Implemented,
        "F1/F2: (tmos.ltm)# list pool 显示池配置", &[F5_TMSH, F5_POOL], true),
    tmos!("tmos_ltm/list/virtual", "lit:virtual|p[0.word]", Effect::Query, SemanticSupport::Implemented,
        "F1/F3: (tmos.ltm)# list virtual 显示虚拟服务器配置", &[F5_TMSH, F5_VIRTUAL], true),
    tmos!("tmos_ltm/show/pool", "lit:pool|p[0.word]", Effect::Query, SemanticSupport::Implemented,
        "F1: (tmos.ltm)# show pool 显示状态与当前会话分配", &[F5_TMSH, F5_POOL], true),
    tmos!("tmos_ltm/show/virtual", "lit:virtual|p[0.word]", Effect::Query, SemanticSupport::Implemented,
        "F1/F3: (tmos.ltm)# show virtual 显示虚拟服务器状态", &[F5_TMSH, F5_VIRTUAL], true),
    tmos!("tmos_ltm/list/pool/@0word", "param:word|p[]", Effect::Query, SemanticSupport::Implemented,
        "F2: (tmos.ltm)# list pool /Common/<name>", &[F5_POOL, F5_CD], true),
    tmos!("tmos_ltm/list/virtual/@0word", "param:word|p[]", Effect::Query, SemanticSupport::Implemented,
        "F3: (tmos.ltm)# list virtual /Common/<name>", &[F5_VIRTUAL, F5_CD], true),
    tmos!("tmos_ltm/show/pool/@0word", "param:word|p[]", Effect::Query, SemanticSupport::Implemented,
        "F1/F2: (tmos.ltm)# show pool /Common/<name>", &[F5_TMSH, F5_POOL], true),
    tmos!("tmos_ltm/show/virtual/@0word", "param:word|p[]", Effect::Query, SemanticSupport::Implemented,
        "F1/F3: (tmos.ltm)# show virtual /Common/<name>", &[F5_TMSH, F5_VIRTUAL], true),
    tmos!("tmos_ltm/cd//Common", "lit:/Common|p[]", Effect::ModeSwitch, SemanticSupport::Implemented,
        "F6: 模块内 cd /Common", &[F5_CD], true),
    tmos!("tmos_ltm/cd/@0word", "param:word|p[]", Effect::Unimplemented, SemanticSupport::NotSimulated,
        "F6/§13(9): 其他 Partition 未模拟", &[F5_CD], false),
    tmos!("tmos_ltm/pwd", "lit:pwd|p[]", Effect::Query, SemanticSupport::Implemented,
        "F6: 模块内 pwd", &[F5_CD], true),
    tmos!("tmos_ltm/help", "lit:help|p[]", Effect::Query, SemanticSupport::Implemented,
        "F1: 模块内 help", &[F5_TMSH], true),
    tmos!("tmos_ltm/exit", "lit:exit|p[]", Effect::ModeSwitch, SemanticSupport::Implemented,
        "F4: (tmos.ltm)# exit → (tmos)#", &[F5_EXIT], true),
    tmos!("tmos_ltm/quit", "lit:quit|p[]", Effect::ModeSwitch, SemanticSupport::Implemented,
        "F5: 模块内 quit 退出 tmsh", &[F5_QUIT], true),

    tmos!("tmos_sys/list/version", "lit:version|p[]", Effect::Query, SemanticSupport::Implemented,
        "F1: (tmos.sys)# list version（无逐字输出黄金）", &[F5_TMSH], true),
    tmos!("tmos_sys/show/version", "lit:version|p[]", Effect::Query, SemanticSupport::Implemented,
        "F1: (tmos.sys)# show version", &[F5_TMSH], true),
    tmos!("tmos_sys/exit", "lit:exit|p[]", Effect::ModeSwitch, SemanticSupport::Implemented,
        "F4: (tmos.sys)# exit → (tmos)#", &[F5_EXIT], true),
    tmos!("tmos_sys/quit", "lit:quit|p[]", Effect::ModeSwitch, SemanticSupport::Implemented,
        "F5: sys 模块内 quit", &[F5_QUIT], true),
];

/// 派生逻辑视图；不携带语法、帮助或 handler。
#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub id: String,
    pub dialect: Dialect,
    pub mode: &'static str,
    pub shape: &'static str,
    pub effect: Effect,
    pub capability: Option<Capability>,
    pub semantics: SemanticSupport,
    pub evidence: EvidenceLevel,
    pub contract: &'static str,
    pub references: &'static [&'static str],
    pub strict: bool,
}

/// 由稳定 ID 查派生规格；无规格的节点仍按迁移期行为执行，并由 lint 显式列出。
pub fn find(dialect: Dialect, id: &str) -> Option<CommandSpec> {
    ROWS.iter()
        .find(|r| r.dialect == dialect && r.id == id)
        .map(|r| CommandSpec {
            id: r.id.to_string(),
            dialect: r.dialect,
            mode: r.id.split('/').next().unwrap_or(""),
            shape: r.shape,
            effect: r.effect,
            capability: r.capability,
            semantics: r.semantics,
            evidence: r.evidence,
            contract: r.contract,
            references: r.references,
            strict: r.strict,
        })
}

/// 严格门禁 ID 从旁表派生，不维护第二份清单。
pub fn strict_ids(dialect: Dialect) -> Vec<&'static str> {
    ROWS.iter()
        .filter(|r| r.dialect == dialect && r.strict)
        .map(|r| r.id)
        .collect()
}

pub fn rows(dialect: Dialect) -> Vec<SpecRow> {
    ROWS.iter()
        .filter(|r| r.dialect == dialect)
        .copied()
        .collect()
}

/// 规格 lint：覆盖、悬空、重复 ID、定义漂移和显式迁移清单。
pub fn lint_report(dialect: Dialect) -> LintReport {
    let roots = crate::roots_for(dialect);
    let strict = strict_ids(dialect);
    spec::lint(&roots, &rows(dialect), &strict)
}

/// 从命令树派生全部可执行节点，供测试与迁移清单核对。
pub fn derived(dialect: Dialect) -> Vec<DerivedCommand> {
    spec::derive_commands(&crate::roots_for(dialect))
}

/// 已接线 handler 的效果分类统计（阶段 1 静态扫描结果）。
pub fn effect_counts(dialect: Dialect) -> Vec<(&'static str, usize)> {
    let rs = rows(dialect);
    let mut out: Vec<(&'static str, usize)> = [
        Effect::StateChange,
        Effect::Query,
        Effect::ModeSwitch,
        Effect::Session,
        Effect::Traffic,
        Effect::Unimplemented,
    ]
    .iter()
    .map(|e| (e.label(), rs.iter().filter(|r| r.effect == *e).count()))
    .collect();
    out.retain(|(_, n)| *n > 0);
    out
}
