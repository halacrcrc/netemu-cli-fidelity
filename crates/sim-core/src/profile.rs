//! 阶段 1：设备 Profile、显式平台策略与能力证据。
//!
//! Profile 是静态平台描述：它不保存接口、路由或协议状态，也不持有第二份配置。
//! 配置、协议、拓扑和数据面状态仍只由 `device`/`engine`/`net`/`summary` 持有。
//! 平台默认值通过 `PlatformPolicy` 显式注入，共享 handler 不按方言隐式分支。

use crate::device::{DeviceKind, Dialect};

/// 存档 schema 版本。0 = 无版本字段的阶段 0 旧存档。
pub const SCHEMA_VERSION: u32 = 1;

/// 旧存档缺少 Profile 时使用的默认映射提示。
pub const LEGACY_PROFILE_NOTICE: &str =
    "旧存档未提供设备 Profile，已按 dialect + kind 映射到默认基线；能力门禁可能改变部分命令的支持状态";

/// 加载工作区将清空所有终端会话，UI 必须明示（阶段 1 存档兼容）。
pub const LOAD_RESET_NOTICE: &str = "加载将重置所有终端会话";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceLevel {
    /// 官方文档或官方示例；不宣称已设备验证。
    DocumentationOnly,
    /// 已在指定 eNSP 设备镜像验证（须记录 VRP 版本）。
    EnsPVerified,
    /// 已在真实设备验证（须记录平台与 OS 版本）。
    DeviceVerified,
    /// 公开社区样本，仅作辅助。
    CommunitySample,
    /// 证据不足，不能判定支持或不支持。
    Unverified,
}

impl EvidenceLevel {
    pub fn label(self) -> &'static str {
        match self {
            EvidenceLevel::DocumentationOnly => "DocumentationOnly",
            EvidenceLevel::EnsPVerified => "EnsPVerified",
            EvidenceLevel::DeviceVerified => "DeviceVerified",
            EvidenceLevel::CommunitySample => "CommunitySample",
            EvidenceLevel::Unverified => "Unverified",
        }
    }

    /// 只有积极证据才允许据此判定平台能力；证据不足不得当成“不支持”。
    pub fn can_decide_capability(self) -> bool {
        !matches!(
            self,
            EvidenceLevel::Unverified | EvidenceLevel::CommunitySample
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    L3InterfaceAddressing,
    StaticRouting,
    RouteTableView,
    ConfigView,
    IcmpEcho,
    /// 二层交换端口（`switchport` / `port link-type` 一类命令）。
    L2SwitchPort,
    /// 配置持久化到闪存（`save` / `write memory`）。
    ConfigPersistence,
    /// 区域化防火墙（IOS XE ZBF / VRP 安全区域策略）。
    ZoneFirewall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityVerdict {
    Supported,
    /// 有积极证据表明当前平台不支持。
    Unsupported,
    /// 证据不足：运行时不得据此返回 `UnsupportedByDevice`。
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityEvidence {
    pub capability: Capability,
    pub verdict: CapabilityVerdict,
    pub level: EvidenceLevel,
    pub source: &'static str,
    pub note: &'static str,
}

/// Cisco IOS XE 17.6 官方文档（Catalyst 9300 接口与硬件组件配置指南）：
/// routed port 是三层口，`switchport` 用于把交换机物理口切到二层模式。
/// 该资料证明 `switchport` 属于 LAN 交换端口能力；本项目 IOS Router Profile
/// 不含交换硬件，因此判定为该能力不支持。
/// 正文已于阶段 2A 通过同族官方 HTML 章节复核（见 CISCO_L2_PORT_HTML_REF）：
/// “A routed port is a Layer 3 interface only and does not support Layer 2
/// protocols, such as DTP and STP.”；逐字设备报错文本仍未验证。
pub const CISCO_L2_PORT_REF: &str =
    "https://www.cisco.com/c/en/us/td/docs/switches/lan/catalyst9300/software/release/17-6/configuration_guide/int_hw/b_176_int_and_hw_9300_cg.pdf";

/// E5 正文复核来源：同一《Interface and Hardware Components Configuration Guide》
/// 的 Catalyst 9300 / IOS XE 17.13 HTML 章节（17.6 PDF 在本网络被 Akamai 拒绝）。
/// 仅用于佐证书正文，不改变 DocumentationOnly 等级，也不作为逐字输出黄金依据。
pub const CISCO_L2_PORT_HTML_REF: &str =
    "https://www.cisco.com/c/en/us/td/docs/switches/lan/catalyst9300/software/release/17-13/configuration_guide/int_hw/b_1713_int_and_hw_9300_cg/configuring_interface_characteristics.html";

/// 进入 trunk 模式时的初始放行范围。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrunkInitial {
    /// 思科：进入 trunk 默认放行全部 VLAN（状态层 `trunk_allowed = None`）。
    AllVlans,
    /// 华为/华三：进入 trunk 默认仅放行 VLAN1，待 allow-pass 扩充。
    Vlan1Only,
}

/// 未划分安全区域的接口流量如何处理。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnzonedTraffic {
    /// 安全策略不参与判定（沿用既有教学行为；本包无官方正文证据，标注待验证）。
    PassThrough,
    /// 按缺省策略处理。
    ApplyDefault,
    /// IOS XE ZBF：两个未划区域接口之间不经过防火墙；任一侧已划入区域则丢弃
    /// （官方「Traffic from a zone interface to a nonzone interface ... is always
    /// dropped；unless default zones are enabled」）。
    ZoneBoundaryDrop,
}

/// 防火墙策略来源模型（阶段 6B）。
///
/// 共享数据面只按此显式平台对象选择策略来源，不写 `if dialect` 分支：
/// - VRP 基线 = 扁平「区域对 + 顺序规则」；
/// - IOS XE ZBF = zone-pair → policy-map → class-map 的原生对象图。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirewallPolicyModel {
    /// 该设备类型不做防火墙转发判定。
    None,
    /// 扁平顺序规则（`security_policies`）。
    FlatRules,
    /// IOS XE ZBF 对象图（`zbf_zone_pairs` / `zbf_policy_maps` / `zbf_class_maps`）。
    ZonePairs,
}

/// 安全策略数据面平台策略对象（阶段 6A，参照 Trunk 平台策略先例显式注入）。
///
/// 共享的 `engine` 数据面只消费这里的值，不按方言写 `if dialect` 分支。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirewallPolicy {
    /// 策略来源模型。
    pub model: FirewallPolicyModel,
    /// 域间流量未命中任何启用规则时的动作（华为缺省策略 any/any → deny）。
    pub interzone_default_permit: bool,
    /// 同区域流量未命中任何启用规则时的动作。
    pub intra_zone_default_permit: bool,
    /// 未划分区域接口的处置。
    pub unzoned: UnzonedTraffic,
    /// 会话表项空闲老化时间（TCP）。
    pub tcp_idle_ns: u64,
    /// 会话表项空闲老化时间（ICMP）。
    pub icmp_idle_ns: u64,
}

impl FirewallPolicy {
    /// 命中会话表后是否仍复查安全策略。阶段 6A 基线 = 不复查。
    pub const RECHECK_POLICY_ON_SESSION_HIT: bool = false;
}

/// 平台默认策略对象：共享状态操作只消费这里的值，不内嵌 `if dialect`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformPolicy {
    /// 路由器（及防火墙等三层设备）物理口默认管理状态。
    pub router_interface_admin_up: bool,
    /// 交换机端口与主机网卡默认管理状态。
    pub switch_interface_admin_up: bool,
    /// 进入 trunk 模式的初始放行范围。
    pub trunk_initial: TrunkInitial,
    /// 显式 allow-pass 是否强制保留 VLAN1（华为/华三 true，思科 false）。
    pub trunk_allow_keeps_vlan1: bool,
    /// allow-pass 是否要求端口已处于 trunk 模式（华为/华三 true，思科 false）。
    pub trunk_allow_requires_trunk_mode: bool,
    /// allow-pass 是否在现有放行列表上累加（华为/华三 true），还是替换（思科 false）。
    pub trunk_allow_additive: bool,
    /// 安全策略数据面（阶段 6A）。
    pub firewall: FirewallPolicy,
}

impl PlatformPolicy {
    pub fn default_admin_up(self, kind: DeviceKind) -> bool {
        if matches!(
            kind,
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            self.router_interface_admin_up
        } else {
            self.switch_interface_admin_up
        }
    }

    /// 进入 trunk 模式时写入状态层的初始放行列表；空列表表示全部放行（None）。
    pub fn trunk_initial_list(self) -> Vec<u16> {
        match self.trunk_initial {
            TrunkInitial::AllVlans => Vec::new(),
            TrunkInitial::Vlan1Only => vec![1],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vendor {
    Cisco,
    Huawei,
    H3C,
    /// F5 Networks BIG-IP（阶段 5A / TMOS 17.x）
    F5,
    Generic,
}

/// 静态平台描述。首阶段只覆盖 C1/H0 所需维度，不提前建设 AAA、板卡或硬件模型。
#[derive(Clone, Debug)]
pub struct DeviceProfile {
    pub id: &'static str,
    pub vendor: Vendor,
    pub dialect: Dialect,
    pub kind: DeviceKind,
    pub os_baseline: &'static str,
    pub policy: PlatformPolicy,
    pub evidence: EvidenceLevel,
    pub capabilities: Vec<CapabilityEvidence>,
}

impl DeviceProfile {
    pub fn capability(&self, cap: Capability) -> CapabilityVerdict {
        self.capabilities
            .iter()
            .find(|e| e.capability == cap)
            .map(|e| {
                if e.level.can_decide_capability() {
                    e.verdict
                } else {
                    CapabilityVerdict::Unknown
                }
            })
            .unwrap_or(CapabilityVerdict::Unknown)
    }

    pub fn capability_evidence(&self, cap: Capability) -> Option<&CapabilityEvidence> {
        self.capabilities.iter().find(|e| e.capability == cap)
    }
}

/// H3C Comware V7 官方文档基线（阶段 4A）。逐字输出与真机校准仍待验证。
/// 证据与访问日期见 `docs/cli-fidelity/stage4a-implementation-record.md` §2（E1–E10）。
pub const COMWARE_V7_BASELINE: &str = "H3C Comware V7 Documentation Baseline (E1-E10)";
pub const COMWARE_L2_REF: &str =
    "H3C《09-VLAN configuration commands》https://wwwsg.h3c.com/en/d_201906/1198196_294551_0.htm";
pub const COMWARE_SWITCH_REF: &str =
    "H3C《04-二层技术-以太网交换配置指导》https://www.h3c.com/cn/d_202212/1745029_30005_0.htm";
pub const COMWARE_ROUTING_REF: &str =
    "H3C《05-三层技术-IP路由配置指导/07-OSPF配置》https://www.h3c.com/cn/d_202504/2394061_30005_0.htm";

/// 三平台独立基线标识（Comware 不再沿用阶段 0「通用模型（待验证）」）。
pub const IOS_ROUTER_PROFILE: &str = "ios-xe-17-router";
pub const IOS_SWITCH_PROFILE: &str = "ios-xe-17-l3-switch";
pub const VRP_ROUTER_PROFILE: &str = "vrp-generic-router";
pub const VRP_SWITCH_PROFILE: &str = "vrp-generic-l3-switch";
pub const COMWARE_ROUTER_PROFILE: &str = "comware-generic-router";
pub const COMWARE_SWITCH_PROFILE: &str = "comware-generic-l3-switch";

// ---------- 阶段 6A：防火墙基线 ----------
//
// 基线决策：**华为 VRP 安全策略（USG/FW 文档基线）**。评估矩阵与理由见
// `docs/cli-fidelity/stage6a-implementation-record.md` §2。
// 落选血统 Cisco ASA（`nameif` / `security-level` / 安全级别回退）已从命令树与
// 数据面移除，仅保留存档字段，不作为独立 Profile 存在。
/// 华为 VRP 防火墙安全策略官方文档基线（V1–V9）。
pub const VRP_FIREWALL_BASELINE: &str =
    "Huawei VRP Firewall Security Policy Documentation Baseline (V1-V9)";
/// 华为 VRP 防火墙 Profile（`DeviceKind::Firewall` 的基线 Profile）。
pub const VRP_FIREWALL_PROFILE: &str = "vrp-generic-firewall";
/// Cisco IOS XE 17.x Zone-Based Policy Firewall Profile（阶段 6B：完整 ZBF 对象图）。
pub const IOS_FIREWALL_PROFILE: &str = "ios-xe-17-zbf-firewall";
/// Comware V7 防火墙：沿用 Stage 4A 边界（区域/策略可解析、可显示），
/// **安全数据面不在本包**。
pub const COMWARE_FIREWALL_PROFILE: &str = "comware-generic-firewall";

/// 阶段 6B：Cisco IOS XE 17.x ZBF 官方正文（全文核验，2026-11-16）。
/// 覆盖 class-map / policy-map / zone-pair / service-policy、inspect/pass/drop、
/// 缺省丢弃、同区域缺省放行与子模式提示符。
pub const CISCO_ZBF_REF: &str = "Cisco《Security and VPN Configuration Guide, Cisco IOS XE 17.x》Zone-Based Policy Firewalls, https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/sec-vpn/b-security-vpn/m_sec-zone-pol-fw-xe.html";

/// V1：华为《安全策略的默认设置》—— 缺省策略 local→any permit，其他 any/deny；
/// 控制同一安全区域内流量的缺省安全策略缺省禁用。
pub const HW_SECPOLICY_DEFAULTS: &str = "Huawei《安全策略的默认设置》(Default Settings for Security Policies), https://support.huawei.cn/hedex/api/pages/EDOC1100435583/AEP0109R/06/resources/security_secpolicy_cfg_0009.html";
/// V2：华为《什么是安全策略？》—— 首包匹配策略并建会话表；后续包查会话表，
/// **不再匹配安全策略**；只需在访问发起方向配置策略。
pub const HW_SECPOLICY_ENCYCLOPEDIA: &str = "Huawei《什么是安全策略？》(IP 知识百科, 更新 2026-07-13), https://info.support.huawei.com/info-finder/encyclopedia/zh/安全策略.html";
/// V3：华为《清除会话表信息》—— 会话持续有流量则不老化；策略改「允许→禁止」后
/// 需清除会话表才立即生效，清除后连接被强行中断。
pub const HW_SESSION_CLEAR: &str = "Huawei《清除会话表信息》(Clearing a Session Table), https://support.huawei.cn/hedex/api/pages/EDOC1100386427/AZN0712J/09/resources/security_session_management_cfg_0009.html";
/// V4：华为《检查安全策略》—— 缺省情况下 FW 系列防火墙域内（如 trust→trust）
/// 安全策略都是禁止的，需手工打开；`display security-policy` 输出
/// RULE ID / RULE NAME / STATE / ACTION / HITTED。
pub const HW_SECPOLICY_CHECK: &str = "Huawei《检查安全策略》(Troubleshooting, V5R1), https://info.support.huawei.com/network/ptmngsys/Web/tsrev_security/cn/content/security/28_edesk_nat_failed_V5R1/edesk_nat_failed_edesk003.html";
/// V5：华为《安全区域配置命令》—— `firewall zone <name>` / `add interface` /
/// `undo add interface` / `display zone`；视图 `[HUAWEI-zone-trust]`。
pub const HW_ZONE_CMDS: &str = "Huawei《安全区域配置命令》(CLI Reference), https://support.huawei.cn/enterprise/zh/doc/EDOC1100460859/58b84bde";
/// V6：华为《检查防火墙基本功能的配置结果》—— `display firewall zone
/// [ zone-name ] [ interface | priority ]`、`display firewall interzone`。
pub const HW_FIREWALL_CHECK: &str = "Huawei《检查防火墙基本功能的配置结果》, https://support.huawei.com/enterprise/zh/doc/EDOC1100317283/cd376375";
/// V7：华为《调整安全策略规则》/`enable (安全策略规则视图)` —— `rule move
/// rule-name1 { after | before } rule-name2`；规则视图 `enable` / `disable`，
/// 缺省为启用。
pub const HW_RULE_ADJUST: &str = "Huawei《调整安全策略规则》https://support.huawei.com/enterprise/zh/doc/EDOC1100412543/284718e7 ; 《enable(安全策略规则视图)》https://support.huawei.cn/enterprise/zh/doc/EDOC1100412535/2480e39b (EN-US_CLIREF_0000002377839328)";
/// V8：华为《配置安全策略的会话老化时间》—— `security-policy` → `rule name` →
/// `session aging-time`；被策略阻断会话缺省保留 120s。
pub const HW_SESSION_AGING: &str = "Huawei《配置安全策略的会话老化时间》(Configuring the Session Aging Time of a Security Policy), https://support.huawei.cn/hedex/api/pages/EDOC1100435583/AEP0109R/06/resources/security_secpolicy_cfg_0018.html";
/// V9：华为《安全策略配置命令》—— `display security-policy rule …` 输出
/// RULE ID / RULE NAME / STATE / ACTION / HITS；`default action { permit | deny }`。
pub const HW_SECPOLICY_CMDS: &str = "Huawei《Security Policy Configuration Commands》, https://support.huawei.com/enterprise/en/doc/EDOC1100262560/2480e39b/EN-US_CLIREF_0000001536916906";
/// F5 BIG-IP TMOS 17.x 官方文档基线（阶段 5A）。
/// 证据 F1–F6 与访问日期见 `docs/cli-fidelity/stage5a-implementation-record.md` §2。
/// TMOS 只有 LoadBalancer 这一种设备类型，故 Profile 不按 router/switch 拆分。
pub const TMOS_V17_BASELINE: &str = "F5 BIG-IP TMOS 17.x Documentation Baseline (F1-F6)";
pub const TMOS_LB_PROFILE: &str = "f5-bigip-tmos-17-loadbalancer";
pub const GENERIC_PROFILE: &str = "generic-unverified";
/// 官方 F5 TMSH Reference v17 的 tmsh / ltm pool / ltm virtual 页面。
pub const TMOS_TMSH_REF: &str =
    "F5《TMSH / tmsh(1)》https://clouddocs.f5.com/cli/tmsh-reference/latest/general/tmsh.html";
pub const TMOS_LTM_POOL_REF: &str =
    "F5《ltm pool(1)》https://clouddocs.f5.com/cli/tmsh-reference/latest/modules/ltm/ltm_pool.html";
pub const TMOS_LTM_VIRTUAL_REF: &str =
    "F5《ltm virtual(1)》https://clouddocs.f5.com/cli/tmsh-reference/latest/modules/ltm/ltm_virtual.html";

fn common_capabilities(kind: DeviceKind, dialect: Dialect) -> Vec<CapabilityEvidence> {
    let doc: &'static str = match dialect {
        Dialect::Ios => "Cisco IOS XE 17.x 配置指南（文档基线）",
        Dialect::Vrp => "Huawei 通用 VRP 文档基线（未经 eNSP 校准）",
        Dialect::Comware => COMWARE_V7_BASELINE,
        Dialect::Tmos => TMOS_V17_BASELINE,
    };
    // 三平台均为官方文档基线；逐字输出证据缺口由规格行的 PENDING_OUTPUT 单独标注。
    let level = EvidenceLevel::DocumentationOnly;
    let l2_verdict = match dialect {
        // 官方资料把 switchport 定义为 LAN 交换端口能力；三层路由器口不具备该能力。
        Dialect::Ios if matches!(kind, DeviceKind::Switch | DeviceKind::AccessPoint) => {
            CapabilityVerdict::Supported
        }
        Dialect::Ios => CapabilityVerdict::Unsupported,
        // E1/E2：Comware V7 的 port link-type / port access vlan / port trunk permit vlan
        // 属于二层以太网接口能力，交换设备有正向官方证据。
        Dialect::Comware if matches!(kind, DeviceKind::Switch | DeviceKind::AccessPoint) => {
            CapabilityVerdict::Supported
        }
        // VRP 与 Comware 非交换设备无正向“不支持”证据，按待验证处理。
        _ => CapabilityVerdict::Unknown,
    };
    let l2_level = if l2_verdict == CapabilityVerdict::Unknown {
        EvidenceLevel::Unverified
    } else {
        EvidenceLevel::DocumentationOnly
    };
    // 阶段 6B：区域化防火墙能力按平台与设备类型显式给值。
    // - IOS XE 路由器/防火墙：ZBF 官方正文支持（Router 的 ZBF 是官方主线场景）；
    // - VRP 防火墙：安全区域策略基线；VRP 路由器无正/负证据 → Unknown；
    // - Comware/Tmos：安全数据面未模拟，不宣称支持也不宣称不支持。
    let (zbf_verdict, zbf_level, zbf_source, zbf_note) = match (dialect, kind) {
        (Dialect::Ios, DeviceKind::Router | DeviceKind::Firewall) => (
            CapabilityVerdict::Supported,
            EvidenceLevel::DocumentationOnly,
            CISCO_ZBF_REF,
            "Zone-Based Policy Firewall：zone-pair + class/policy-map + inspect/pass/drop",
        ),
        (Dialect::Vrp, DeviceKind::Firewall) => (
            CapabilityVerdict::Supported,
            EvidenceLevel::DocumentationOnly,
            VRP_FIREWALL_BASELINE,
            "安全区域 + 顺序安全策略 + 状态化会话",
        ),
        _ => (
            CapabilityVerdict::Unknown,
            EvidenceLevel::Unverified,
            doc,
            "区域化防火墙能力未在本模拟器中承诺",
        ),
    };
    vec![
        CapabilityEvidence {
            capability: Capability::L3InterfaceAddressing,
            verdict: CapabilityVerdict::Supported,
            level,
            source: doc,
            note: "接口编址与掩码/前缀长度形式",
        },
        CapabilityEvidence {
            capability: Capability::StaticRouting,
            verdict: CapabilityVerdict::Supported,
            level,
            source: doc,
            note: "静态路由写入与撤销",
        },
        CapabilityEvidence {
            capability: Capability::RouteTableView,
            verdict: CapabilityVerdict::Supported,
            level,
            source: doc,
            note: "路由表查询视图；逐字输出证据待验证",
        },
        CapabilityEvidence {
            capability: Capability::ConfigView,
            verdict: CapabilityVerdict::Supported,
            level,
            source: doc,
            note: "配置视图从 DevSummary 实时渲染（ADR D12）",
        },
        CapabilityEvidence {
            capability: Capability::IcmpEcho,
            verdict: CapabilityVerdict::Supported,
            level,
            source: doc,
            note: "ICMP echo 属于已建模的 L4 场景",
        },
        CapabilityEvidence {
            capability: Capability::L2SwitchPort,
            verdict: l2_verdict,
            level: l2_level,
            source: CISCO_L2_PORT_REF,
            note: "routed port 为三层口；switchport 属二层交换端口能力",
        },
        CapabilityEvidence {
            capability: Capability::ConfigPersistence,
            verdict: CapabilityVerdict::Supported,
            level: EvidenceLevel::DocumentationOnly,
            source: doc,
            note: "平台支持保存配置；模拟器尚未实现持久化语义",
        },
        CapabilityEvidence {
            capability: Capability::ZoneFirewall,
            verdict: zbf_verdict,
            level: zbf_level,
            source: zbf_source,
            note: zbf_note,
        },
    ]
}

/// 由方言与设备类型派生 Profile；旧存档缺少 Profile 时也走同一映射，避免静默差异。
pub fn profile_id_for(dialect: Dialect, kind: DeviceKind) -> &'static str {
    match (dialect, kind) {
        (Dialect::Ios, DeviceKind::Switch | DeviceKind::AccessPoint) => IOS_SWITCH_PROFILE,
        // 阶段 6A：防火墙按基线拆出独立 Profile（F5/Comware 先例）。
        (Dialect::Ios, DeviceKind::Firewall) => IOS_FIREWALL_PROFILE,
        (Dialect::Ios, _) => IOS_ROUTER_PROFILE,
        (Dialect::Vrp, DeviceKind::Switch | DeviceKind::AccessPoint) => VRP_SWITCH_PROFILE,
        (Dialect::Vrp, DeviceKind::Firewall) => VRP_FIREWALL_PROFILE,
        (Dialect::Vrp, _) => VRP_ROUTER_PROFILE,
        (Dialect::Comware, DeviceKind::Switch | DeviceKind::AccessPoint) => COMWARE_SWITCH_PROFILE,
        (Dialect::Comware, DeviceKind::Firewall) => COMWARE_FIREWALL_PROFILE,
        (Dialect::Comware, _) => COMWARE_ROUTER_PROFILE,
        // 阶段 5A：TMOS 只有 LoadBalancer 一种设备类型，稳定 ID 唯一。
        (Dialect::Tmos, _) => TMOS_LB_PROFILE,
    }
}

pub fn profile_for(dialect: Dialect, kind: DeviceKind) -> DeviceProfile {
    let (vendor, os_baseline, router_up, evidence) = match dialect {
        Dialect::Ios => (
            Vendor::Cisco,
            "Cisco IOS XE 17.x",
            false,
            EvidenceLevel::DocumentationOnly,
        ),
        // VRP 文档基线：路由器接口默认已启用，不应要求 undo shutdown 补救错误默认。
        Dialect::Vrp => (
            Vendor::Huawei,
            "Huawei Generic VRP Documentation Baseline",
            true,
            EvidenceLevel::DocumentationOnly,
        ),
        // 阶段 4A：Comware 改为明确的 V7 官方文档基线（E1–E10），不再是「阶段 0 通用模型」。
        // 路由器物理口默认管理状态在本包取得的官方正文中无直接结论，保持项目既有
        // 保守值 false 并在实施记录标注待验证；这里没有机械继承 VRP 的 true。
        Dialect::Comware => (
            Vendor::H3C,
            COMWARE_V7_BASELINE,
            false,
            EvidenceLevel::DocumentationOnly,
        ),
        // 阶段 5A：F5 官方 TMOS 17.x 文档基线（F1–F6）。TMOS 物理口默认管理状态
        // 在 F1–F6 正文中无直接结论，沿用项目既有 LoadBalancer 的默认 up 策略，
        // 并在实施记录标注「平台策略待验证」；不机械继承 VRP/Comware 的值。
        Dialect::Tmos => (
            Vendor::F5,
            TMOS_V17_BASELINE,
            true,
            EvidenceLevel::DocumentationOnly,
        ),
    };
    // Trunk 平台默认值逐平台显式注入，共享 handler 不内嵌方言分支。
    // 思科：进入 trunk 放行全部、allowed vlan 替换列表。
    // 华为 VRP：文档基线仅放行 VLAN1、allow-pass 需先 link-type trunk 且累加保留 VLAN1。
    // 华三 Comware V7：由 E1/E2/E3/E10 独立核定——
    //   E1「A trunk port allows only packets from VLAN 1 to pass through」；
    //   E1「If you use the port trunk permit vlan command multiple times, the trunk
    //        port carries the set of VLANs specified by vlan-list in each execution」→ 累加；
    //   E3 配置文件 `port trunk permit vlan 1 100 200` → 保留 VLAN1；
    //   E1/E2 官方示例一律先 `port link-type trunk` → 需要 Trunk 模式。
    // 结论值与 VRP 相同但证据独立，不是从 VRP 机械继承。
    let (
        trunk_initial,
        trunk_allow_keeps_vlan1,
        trunk_allow_requires_trunk_mode,
        trunk_allow_additive,
    ) = match dialect {
        Dialect::Ios => (TrunkInitial::AllVlans, false, false, false),
        Dialect::Vrp => (TrunkInitial::Vlan1Only, true, true, true),
        Dialect::Comware => (TrunkInitial::Vlan1Only, true, true, true),
        // TMOS 无「Trunk 放行列表」概念；命令树里也没有二层 VLAN 命令。
        // 这里给出显式中性策略，避免共享 handler 隐式按方言分支。
        Dialect::Tmos => (TrunkInitial::AllVlans, false, false, false),
    };
    // 阶段 6A：安全策略数据面平台策略显式注入。
    //
    // 取值依据（全部为文档正文级证据，见 `HW_*` 与 §2 实施记录）：
    // - 域间未命中缺省拒绝：V1「其他策略：匹配条件 any，动作 deny」。
    // - 同区域未命中缺省拒绝：V4「缺省情况下，FW 系列防火墙域内的安全策略
    //   （如 trust 到 trust）都是禁止的，需要手动打开域内访问策略」。
    //   注意 V1 中「控制同一安全区域内流量的缺省安全策略」缺省禁用属**版本差异**，
    //   实施记录 §4 已登记该分歧并按「不得无条件放行」取更严格的一端；
    //   eNSP 校准后如确认所选版本为放行，只需改这一个策略值，不动共享逻辑。
    // - 未划分区域接口：本包未取得官方正文结论，显式标注待验证，保持既有教学行为。
    // - 老化时间：模拟器参数（参照公开 USG `display firewall session aging-time`
    //   观测值 tcp 1200s / icmp 20s，仅社区样本级），不作为保真承诺；`reset` 命令
    //   提供「立即生效」的可测路径（V3）。
    let firewall = match (dialect, kind) {
        // 基线：VRP 防火墙（Firewall 设备类型）。
        (Dialect::Vrp, DeviceKind::Firewall) => FirewallPolicy {
            model: FirewallPolicyModel::FlatRules,
            interzone_default_permit: false,
            intra_zone_default_permit: false,
            unzoned: UnzonedTraffic::PassThrough,
            tcp_idle_ns: 1_200_000_000_000,
            icmp_idle_ns: 20_000_000_000,
        },
        // 阶段 6B：IOS XE 路由器/防火墙执行真实 ZBF。
        // 官方正文：区域间未配置策略 → drop；同区域缺省放行；一个区域口与一个
        // 未划区域口之间始终丢弃；self zone 是缺省拒绝的唯一例外（本模拟器的
        // 设备自发流量不经过转发路径，因此天然满足该例外）。
        (Dialect::Ios, DeviceKind::Router | DeviceKind::Firewall) => FirewallPolicy {
            model: FirewallPolicyModel::ZonePairs,
            interzone_default_permit: false,
            intra_zone_default_permit: true,
            unzoned: UnzonedTraffic::ZoneBoundaryDrop,
            tcp_idle_ns: 3_600_000_000_000,
            icmp_idle_ns: 2_000_000_000,
        },
        // Comware 安全数据面不在本包（Stage 4A 边界），
        // 其余设备类型不做防火墙数据面判定；不承诺任何行为。
        _ => FirewallPolicy {
            model: FirewallPolicyModel::None,
            interzone_default_permit: false,
            intra_zone_default_permit: false,
            unzoned: UnzonedTraffic::PassThrough,
            tcp_idle_ns: 1_200_000_000_000,
            icmp_idle_ns: 20_000_000_000,
        },
    };
    // 防火墙设备使用独立基线标识，不用路由器基线冒充。
    let os_baseline = if kind == DeviceKind::Firewall {
        match dialect {
            Dialect::Vrp => VRP_FIREWALL_BASELINE,
            Dialect::Comware => COMWARE_V7_BASELINE,
            Dialect::Ios => "Cisco IOS XE 17.x Zone-Based Policy Firewall",
            Dialect::Tmos => TMOS_V17_BASELINE,
        }
    } else {
        os_baseline
    };
    DeviceProfile {
        id: profile_id_for(dialect, kind),
        vendor,
        dialect,
        kind,
        os_baseline,
        policy: PlatformPolicy {
            router_interface_admin_up: router_up,
            switch_interface_admin_up: true,
            trunk_initial,
            trunk_allow_keeps_vlan1,
            trunk_allow_requires_trunk_mode,
            trunk_allow_additive,
            firewall,
        },
        evidence,
        capabilities: common_capabilities(kind, dialect),
    }
}

pub fn profile_by_id(id: &str) -> Option<DeviceProfile> {
    const ALL: &[(Dialect, DeviceKind)] = &[
        (Dialect::Ios, DeviceKind::Router),
        (Dialect::Ios, DeviceKind::Switch),
        (Dialect::Ios, DeviceKind::Firewall),
        (Dialect::Vrp, DeviceKind::Router),
        (Dialect::Vrp, DeviceKind::Switch),
        (Dialect::Vrp, DeviceKind::Firewall),
        (Dialect::Comware, DeviceKind::Router),
        (Dialect::Comware, DeviceKind::Switch),
        (Dialect::Comware, DeviceKind::Firewall),
        (Dialect::Tmos, DeviceKind::LoadBalancer),
    ];
    ALL.iter()
        .find(|(d, k)| profile_id_for(*d, *k) == id)
        .map(|(d, k)| profile_for(*d, *k))
}

pub fn platform_policy(dialect: Dialect) -> PlatformPolicy {
    profile_for(dialect, DeviceKind::Router).policy
}

/// 按「方言 + 设备类型」取平台策略；防火墙数据面必须用这个入口，
/// 因为同一方言的 Router 与 Firewall 策略来源模型不同。
pub fn platform_policy_for(dialect: Dialect, kind: DeviceKind) -> PlatformPolicy {
    profile_for(dialect, kind).policy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ios_router_down_vrp_router_up_are_explicit_policies() {
        let ios = platform_policy(Dialect::Ios);
        let vrp = platform_policy(Dialect::Vrp);
        assert!(!ios.router_interface_admin_up);
        assert!(vrp.router_interface_admin_up);
        assert!(!ios.default_admin_up(DeviceKind::Router));
        assert!(vrp.default_admin_up(DeviceKind::Router));
        assert!(ios.default_admin_up(DeviceKind::Switch));
        assert!(vrp.default_admin_up(DeviceKind::Host));
    }

    #[test]
    fn trunk_defaults_are_explicit_per_platform() {
        let ios = platform_policy(Dialect::Ios);
        assert_eq!(ios.trunk_initial, TrunkInitial::AllVlans);
        assert!(ios.trunk_initial_list().is_empty());
        assert!(!ios.trunk_allow_keeps_vlan1);
        assert!(!ios.trunk_allow_requires_trunk_mode);
        assert!(!ios.trunk_allow_additive);

        for d in [Dialect::Vrp, Dialect::Comware] {
            let p = platform_policy(d);
            assert_eq!(p.trunk_initial, TrunkInitial::Vlan1Only);
            assert_eq!(p.trunk_initial_list(), vec![1]);
            assert!(p.trunk_allow_keeps_vlan1);
            assert!(p.trunk_allow_requires_trunk_mode);
            assert!(p.trunk_allow_additive);
        }
    }

    #[test]
    fn profiles_are_independent_and_round_trip_by_id() {
        let ios = profile_for(Dialect::Ios, DeviceKind::Router);
        let vrp = profile_for(Dialect::Vrp, DeviceKind::Router);
        assert_ne!(ios.id, vrp.id);
        assert_eq!(ios.dialect, Dialect::Ios);
        assert_eq!(vrp.dialect, Dialect::Vrp);
        assert_eq!(profile_by_id(vrp.id).unwrap().id, vrp.id);
        assert!(profile_by_id("unknown").is_none());
    }

    #[test]
    fn unsupported_requires_positive_evidence() {
        let ios_router = profile_for(Dialect::Ios, DeviceKind::Router);
        let l2 = ios_router
            .capability_evidence(Capability::L2SwitchPort)
            .unwrap();
        assert_eq!(l2.verdict, CapabilityVerdict::Unsupported);
        assert!(l2.level.can_decide_capability());
        assert_eq!(
            ios_router.capability(Capability::L2SwitchPort),
            CapabilityVerdict::Unsupported
        );

        let ios_switch = profile_for(Dialect::Ios, DeviceKind::Switch);
        assert_eq!(
            ios_switch.capability(Capability::L2SwitchPort),
            CapabilityVerdict::Supported
        );

        // 证据不足不得判成“设备不支持”。
        let vrp_router = profile_for(Dialect::Vrp, DeviceKind::Router);
        assert_eq!(
            vrp_router.capability(Capability::L2SwitchPort),
            CapabilityVerdict::Unknown
        );
    }
}
