//! 三方言 CLI：IOS（思科）、VRP（华为）、Comware（新华三）。
//!
//! 共享的处理器在 `cmd` 模块；方言差异体现在：
//! 1. 命令树（`ios` / `huawei` / `comware` 各自独立 `build()`；阶段 4A 已拆树，
//!    `trees(Dialect::Comware)` 不再返回 `huawei::build()`）
//! 2. 接口名规则（见 ifname）
//! 3. 报错风格与 show 输出格式（见 show）
//! 4. 模式体系（IOS 多一层特权模式）

pub mod comware;
pub mod config_render;
pub mod huawei;
pub mod ifname;
pub mod ios;
pub mod pc;
pub mod show;
pub mod specs;
pub mod tmos;

use std::collections::HashMap;
use std::sync::OnceLock;

use cli_engine::spec::{
    judge, CapabilityVerdict, CommandResult, CommandStatus, Inspection, JudgeInput,
};
use cli_engine::{CliError, Ctx, HelpList, LoginFlow, LoginStep, Mode, RemoteSession, Session};
use sim_core::device::{DevId, DeviceKind, DeviceState, Dialect, MgmtProto, VtyTransport};
use sim_core::engine::Engine;
use sim_core::profile::{self as profile_mod, EvidenceLevel};

/// 供 pc.rs 复用的 IPv4 工具重导出。
pub use sim_packets::{ipv4_from_str, ipv4_str};

pub struct Trees {
    pub exec: Vec<cli_engine::CNode>,
    pub priv_: Vec<cli_engine::CNode>,
    pub config: Vec<cli_engine::CNode>,
    pub if_: Vec<cli_engine::CNode>,
    pub pool: Vec<cli_engine::CNode>,
    pub acl: Vec<cli_engine::CNode>,
    pub line: Vec<cli_engine::CNode>,
    pub aaa: Vec<cli_engine::CNode>,
    pub mst: Vec<cli_engine::CNode>,
    pub ospf: Vec<cli_engine::CNode>,
    pub ospf_area: Vec<cli_engine::CNode>,
    pub bgp: Vec<cli_engine::CNode>,
    pub isis: Vec<cli_engine::CNode>,
    pub route_policy: Vec<cli_engine::CNode>,
    pub zone: Vec<cli_engine::CNode>,
    pub security_policy: Vec<cli_engine::CNode>,
    pub security_rule: Vec<cli_engine::CNode>,
    pub ipsec: Vec<cli_engine::CNode>,
    pub ospfv3: Vec<cli_engine::CNode>,
    /// Comware `vlan <id>` 进入的 VLAN 视图（其余方言保留最小退出集合）。
    pub vlan: Vec<cli_engine::CNode>,
    /// F5 tmsh `ltm` 模块（官方 `(tmos.ltm)#`）。
    pub tmos_ltm: Vec<cli_engine::CNode>,
    /// F5 tmsh `sys` 模块（官方 `(tmos.sys)#`）。
    pub tmos_sys: Vec<cli_engine::CNode>,
    /// 阶段 6B：IOS ZBF `(config-cmap)#`。
    pub class_map: Vec<cli_engine::CNode>,
    /// 阶段 6B：IOS ZBF `(config-pmap)#`。
    pub policy_map: Vec<cli_engine::CNode>,
    /// 阶段 6B：IOS ZBF `(config-pmap-c)#`。
    pub policy_map_class: Vec<cli_engine::CNode>,
}

impl Trees {
    pub fn root_for(&self, mode: Mode) -> &[cli_engine::CNode] {
        match mode {
            Mode::Exec => &self.exec,
            Mode::Priv => &self.priv_,
            Mode::Config => &self.config,
            Mode::If => &self.if_,
            Mode::Pool => &self.pool,
            Mode::Acl => &self.acl,
            Mode::Line => &self.line,
            Mode::Aaa => &self.aaa,
            Mode::Mst => &self.mst,
            Mode::Ospf => &self.ospf,
            Mode::OspfArea => &self.ospf_area,
            Mode::Bgp => &self.bgp,
            Mode::Isis => &self.isis,
            Mode::RoutePolicy => &self.route_policy,
            Mode::Zone => &self.zone,
            Mode::SecurityPolicy => &self.security_policy,
            Mode::SecurityRule => &self.security_rule,
            Mode::Ipsec => &self.ipsec,
            Mode::Ospfv3 => &self.ospfv3,
            Mode::Vlan => &self.vlan,
            Mode::TmosLtm => &self.tmos_ltm,
            Mode::TmosSys => &self.tmos_sys,
            Mode::ClassMap => &self.class_map,
            Mode::PolicyMap => &self.policy_map,
            Mode::PolicyMapClass => &self.policy_map_class,
        }
    }
}

pub fn trees(d: Dialect) -> &'static Trees {
    static MAP: OnceLock<HashMap<Dialect, Trees>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(Dialect::Ios, ios::build());
        m.insert(Dialect::Vrp, huawei::build());
        // 阶段 4A：Comware 使用独立命令树，不再与 VRP 共用 huawei::build()。
        m.insert(Dialect::Comware, comware::build());
        // 阶段 5A：F5 TMOS 17.x / tmsh 使用独立命令树；三棵厂商树不再承载抽象 ltm 命令。
        m.insert(Dialect::Tmos, tmos::build());
        m
    })
    .get(&d)
    .unwrap()
}

/// 把 `Trees` 的每模式根列表交给 `cli-engine::spec` 派生稳定 ID。
/// 不复制语法：只传引用，命令树仍是唯一定义源。
pub fn roots_for(d: Dialect) -> Vec<(&'static str, &'static [cli_engine::CNode])> {
    let t = trees(d);
    vec![
        ("exec", t.exec.as_slice()),
        ("priv", t.priv_.as_slice()),
        ("config", t.config.as_slice()),
        ("if", t.if_.as_slice()),
        ("pool", t.pool.as_slice()),
        ("acl", t.acl.as_slice()),
        ("line", t.line.as_slice()),
        ("aaa", t.aaa.as_slice()),
        ("mst", t.mst.as_slice()),
        ("ospf", t.ospf.as_slice()),
        ("ospf_area", t.ospf_area.as_slice()),
        ("bgp", t.bgp.as_slice()),
        ("isis", t.isis.as_slice()),
        ("route_policy", t.route_policy.as_slice()),
        ("zone", t.zone.as_slice()),
        ("security_policy", t.security_policy.as_slice()),
        ("security_rule", t.security_rule.as_slice()),
        ("ipsec", t.ipsec.as_slice()),
        ("ospfv3", t.ospfv3.as_slice()),
        ("vlan", t.vlan.as_slice()),
        ("tmos_ltm", t.tmos_ltm.as_slice()),
        ("tmos_sys", t.tmos_sys.as_slice()),
        ("class_map", t.class_map.as_slice()),
        ("policy_map", t.policy_map.as_slice()),
        ("policy_map_class", t.policy_map_class.as_slice()),
    ]
}

// ---------- 共享命令处理器 ----------

/// 各方言通用的命令行为；个别方言差异通过参数控制。
pub mod cmd {
    use super::*;
    use crate::show;
    use cli_engine::IfCtx;
    use sim_core::device::{
        Acl, AclProto, AclRule, BgpPeerPolicy, IpsecTunnel, LtmMember, LtmPool, LtmVirtual,
        MlagConfig, RoutePolicy, RoutePolicyNode, SecurityPolicy, SecurityZone, StackConfig,
        WlanProfile, WlanSecurity, ZbfAction, ZbfClassMap, ZbfMatch, ZbfMatchType,
        ZbfPolicyClass, ZbfPolicyMap, ZbfZonePair,
    };
    use sim_packets::mask_from_prefix;

    pub fn hostname(c: &mut Ctx) -> Result<String, CliError> {
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("主机名不能为空".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.name = name;
        }
        Ok(String::new())
    }

    pub fn interface(c: &mut Ctx) -> Result<String, CliError> {
        let raw = c.word("if");
        let kind = c.device_kind();
        match ifname::resolve_if(c.dialect, kind, &raw) {
            Some(ifref) => {
                c.sess.mode = Mode::If;
                c.sess.cur_if = Some(match ifref {
                    ifname::IfRef::Physical(idx) => IfCtx::Physical(idx),
                    ifname::IfRef::Svi(v) => IfCtx::Svi(v),
                    ifname::IfRef::Lag(id) => IfCtx::Lag(id),
                });
                Ok(String::new())
            }
            None => Err(CliError::Custom(match c.dialect {
                Dialect::Ios => "Invalid interface type and number".into(),
                _ => "Wrong parameter found at '^' position.".into(),
            })),
        }
    }

    /// `interface vlan 10`（IOS）/ `interface vlanif 20`（VRP）/ `interface vlan-interface 20`（Comware）
    /// 两段式 SVI 接口名。
    pub fn interface_svi(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Switch | DeviceKind::AccessPoint
        ) {
            return Err(CliError::Custom(match c.dialect {
                Dialect::Ios => "Invalid interface type and number".into(),
                _ => "Wrong parameter found at '^' position.".into(),
            }));
        }
        let v = c
            .number("vlan")
            .ok_or_else(|| CliError::Custom("Invalid VLAN ID".into()))?;
        let vlan = (1..=4094)
            .contains(&v)
            .then_some(v as u16)
            .ok_or_else(|| CliError::Custom("Invalid VLAN ID".into()))?;
        c.sess.mode = Mode::If;
        c.sess.cur_if = Some(IfCtx::Svi(vlan));
        Ok(String::new())
    }

    /// `interface Port-channel1` / `interface Eth-Trunk 1` / `interface Bridge-Aggregation 1`
    pub fn interface_lag(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let id = c
            .number("lag")
            .filter(|n| (1..=128).contains(n))
            .ok_or_else(|| CliError::Custom("Invalid aggregation group number".into()))?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                sw.ensure_lag(id as u16);
            }
        }
        c.sess.mode = Mode::If;
        c.sess.cur_if = Some(IfCtx::Lag(id as u16));
        Ok(String::new())
    }

    fn bind_lag(
        c: &mut Ctx,
        gid: u32,
        mode: sim_core::device::LagMode,
    ) -> Result<String, CliError> {
        require_switch(c)?;
        if !(1..=128).contains(&gid) {
            return Err(CliError::Custom("Invalid aggregation group number".into()));
        }
        let Some(IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom(
                "Aggregation can only be configured on physical interfaces".into(),
            ));
        };
        if c.eng.net.lag_bind(c.dev, port, gid as u16, mode) {
            c.eng.lacp_config_changed(c.dev);
        }
        Ok(String::new())
    }

    /// IOS：`channel-group <n> mode active|passive|on`（省略 mode 等价 on）。
    pub fn lag_bind_ios(c: &mut Ctx) -> Result<String, CliError> {
        let gid = c.number("group").unwrap_or(0);
        let mode = match c.word("mode").to_lowercase().as_str() {
            "active" => sim_core::device::LagMode::LacpActive,
            "passive" => sim_core::device::LagMode::LacpPassive,
            "on" | "" => sim_core::device::LagMode::Static,
            _ => return Err(CliError::Custom("Invalid channel-group mode".into())),
        };
        bind_lag(c, gid, mode)
    }

    /// VRP：成员口 `eth-trunk <n>` / Comware：`port link-aggregation group <n>`（动态聚合）。
    pub fn lag_bind_dynamic(c: &mut Ctx) -> Result<String, CliError> {
        let gid = c.number("group").unwrap_or(0);
        bind_lag(c, gid, sim_core::device::LagMode::LacpActive)
    }

    /// 聚合口视图：切换工作模式（LACP 主动 / 静态）。
    pub fn lag_mode_lacp(c: &mut Ctx) -> Result<String, CliError> {
        lag_set_mode(c, sim_core::device::LagMode::LacpActive)
    }

    pub fn lag_mode_static(c: &mut Ctx) -> Result<String, CliError> {
        lag_set_mode(c, sim_core::device::LagMode::Static)
    }

    fn lag_set_mode(c: &mut Ctx, mode: sim_core::device::LagMode) -> Result<String, CliError> {
        require_switch(c)?;
        let Some(IfCtx::Lag(gid)) = c.sess.cur_if else {
            return Err(CliError::Custom(
                "This command is only valid in an aggregation interface view".into(),
            ));
        };
        if c.eng.net.set_lag_mode(c.dev, gid, mode) {
            c.eng.lacp_config_changed(c.dev);
        }
        Ok(String::new())
    }

    /// 成员口退出聚合组（`no channel-group` / `undo eth-trunk` / `undo port link-aggregation group`）。
    pub fn lag_unbind(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let Some(IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom(
                "This command is only valid on physical interfaces".into(),
            ));
        };
        if c.eng.net.lag_unbind(c.dev, port).is_some() {
            c.eng.lacp_config_changed(c.dev);
        }
        Ok(String::new())
    }

    pub fn show_lag(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(show::lag_table(c.eng, c.dev, c.dialect))
    }

    fn require_if(c: &Ctx) -> Result<IfCtx, CliError> {
        c.sess
            .cur_if
            .ok_or_else(|| CliError::Custom("no interface selected".into()))
    }

    pub fn ip_address(c: &mut Ctx) -> Result<String, CliError> {
        let ifc = require_if(c)?;
        let ip = c.ipv4("ip");
        let len = c.mask_len("mask");
        if ip == 0 {
            return Err(CliError::Custom("Invalid IP address".into()));
        }
        match ifc {
            IfCtx::Physical(port) => {
                if matches!(
                    c.device_kind(),
                    DeviceKind::Switch | DeviceKind::AccessPoint
                ) {
                    return Err(CliError::Custom(
                        "This command is not supported on Layer 2 interfaces".into(),
                    ));
                }
                c.eng.net.set_port_ip(c.dev, port, Some(ip), len);
            }
            IfCtx::Svi(vlan) => {
                if !c.eng.net.set_svi(c.dev, vlan, Some(ip), len) {
                    return Err(CliError::Custom(
                        "Command is only available on switches".into(),
                    ));
                }
            }
            IfCtx::Lag(_) => {
                return Err(CliError::Custom(
                    "This command is not supported on Layer 2 interfaces".into(),
                ));
            }
        }
        Ok(String::new())
    }

    pub fn no_ip_address(c: &mut Ctx) -> Result<String, CliError> {
        let ifc = require_if(c)?;
        match ifc {
            IfCtx::Physical(port) => c.eng.net.set_port_ip(c.dev, port, None, 24),
            IfCtx::Svi(vlan) => {
                c.eng.net.set_svi(c.dev, vlan, None, 24);
            }
            IfCtx::Lag(_) => {}
        }
        Ok(String::new())
    }

    pub fn shutdown(c: &mut Ctx, up: bool) -> Result<String, CliError> {
        let ifc = require_if(c)?;
        match ifc {
            IfCtx::Physical(port) => {
                c.eng.net.set_port_admin(c.dev, port, up);
                c.eng.stp_topology_changed();
            }
            IfCtx::Svi(vlan) => {
                c.eng.net.set_svi_admin(c.dev, vlan, up);
            }
            IfCtx::Lag(gid) => {
                let members = c
                    .eng
                    .net
                    .device(c.dev)
                    .and_then(|d| match &d.state {
                        DeviceState::Switch(sw) => sw.lags.get(&gid).map(|g| g.members.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                for m in members {
                    c.eng.net.set_port_admin(c.dev, m, up);
                }
                c.eng.lacp_config_changed(c.dev);
            }
        }
        Ok(String::new())
    }

    // ---------- VLAN（交换机） ----------

    fn require_switch(c: &Ctx) -> Result<(), CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Switch | DeviceKind::AccessPoint
        ) {
            return Err(CliError::Custom(match c.dialect {
                Dialect::Ios => "Invalid input detected".into(),
                _ => "Command is only available on switches".into(),
            }));
        }
        Ok(())
    }

    fn require_vlan_id(v: u32) -> Result<u16, CliError> {
        if (1..=4094).contains(&v) {
            Ok(v as u16)
        } else {
            Err(CliError::Custom("Invalid VLAN ID".into()))
        }
    }

    /// 创建 VLAN：IOS `vlan 10`、Comware `vlan 10`。
    pub fn create_vlans(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let v = c
            .number("vlan")
            .ok_or_else(|| CliError::Custom("Invalid VLAN ID".into()))?;
        let vlan = require_vlan_id(v)?;
        c.eng.net.ensure_vlan(c.dev, vlan);
        Ok(String::new())
    }

    /// 批量创建 VLAN：VRP `vlan batch 10 20` / `vlan batch 10-12`。
    pub fn create_vlans_list(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let list = c
            .vlan_list("vlans")
            .ok_or_else(|| CliError::Custom("Invalid VLAN list".into()))?;
        for vlan in list {
            c.eng.net.ensure_vlan(c.dev, vlan);
        }
        Ok(String::new())
    }

    /// VLAN 命名（可选；M1 名字仅接受不落库）。
    pub fn vlan_name(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(String::new())
    }

    /// `switchport mode access`
    pub fn sw_mode_access(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let cli_engine::IfCtx::Physical(port) = require_if(c)? else {
            return Err(CliError::Custom(
                "This command is not supported on VLAN interfaces".into(),
            ));
        };
        c.eng
            .net
            .set_port_mode(c.dev, port, sim_core::device::PortMode::Access);
        Ok(String::new())
    }

    /// 进入 trunk 模式：IOS `switchport mode trunk` / 华为华三 `port link-type trunk`。
    /// 初始放行范围由平台策略显式注入（思科全部 VLAN，华为/华三仅 VLAN1）。
    pub fn enter_trunk(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let cli_engine::IfCtx::Physical(port) = require_if(c)? else {
            return Err(CliError::Custom(
                "This command is not supported on VLAN interfaces".into(),
            ));
        };
        let initial = c.platform_policy().trunk_initial_list();
        c.eng.net.set_port_trunk_allowed(c.dev, port, &initial);
        Ok(String::new())
    }

    /// 华为/华三 `port link-type access`
    pub fn lt_access(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let cli_engine::IfCtx::Physical(port) = require_if(c)? else {
            return Err(CliError::Custom(
                "This command is not supported on VLAN interfaces".into(),
            ));
        };
        c.eng
            .net
            .set_port_mode(c.dev, port, sim_core::device::PortMode::Access);
        Ok(String::new())
    }

    pub fn access_vlan(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let cli_engine::IfCtx::Physical(port) = require_if(c)? else {
            return Err(CliError::Custom(
                "This command is not supported on VLAN interfaces".into(),
            ));
        };
        let v = c
            .number("vlan")
            .ok_or_else(|| CliError::Custom("Invalid VLAN ID".into()))?;
        let vlan = require_vlan_id(v)?;
        c.eng.net.set_port_access_vlan(c.dev, port, vlan);
        Ok(String::new())
    }

    /// 否定 access VLAN：IOS `no switchport access vlan` / 华为华三 `undo port default vlan`。
    /// 恢复默认 VLAN 1（各平台一致），复用同一状态操作，不内嵌方言分支。
    pub fn no_access_vlan(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let cli_engine::IfCtx::Physical(port) = require_if(c)? else {
            return Err(CliError::Custom(
                "This command is not supported on VLAN interfaces".into(),
            ));
        };
        c.eng.net.set_port_access_vlan(c.dev, port, 1);
        Ok(String::new())
    }

    /// Trunk 放行列表（"10,20" / "10 20" / "10-12" 混合）。
    /// 前置条件、是否保留 VLAN1、累加还是替换均由平台策略显式注入。
    pub fn trunk_allow(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let cli_engine::IfCtx::Physical(port) = require_if(c)? else {
            return Err(CliError::Custom(
                "This command is not supported on VLAN interfaces".into(),
            ));
        };
        let policy = c.platform_policy();
        // 华为/华三要求先 `port link-type trunk`；未进入 trunk 时拒绝且不改状态。
        if policy.trunk_allow_requires_trunk_mode {
            let mode = c
                .eng
                .net
                .device(c.dev)
                .and_then(|d| d.ports.get(port))
                .map(|p| p.mode);
            if mode != Some(sim_core::device::PortMode::Trunk) {
                return Err(CliError::Custom("The port is not a trunk port".into()));
            }
        }
        let vlans = match c.val("vlans") {
            Some(cli_engine::Val::VlanList(v)) => v.clone(),
            Some(cli_engine::Val::Word(w)) | Some(cli_engine::Val::Raw(w)) => {
                cli_engine::parse_vlan_list(w)
                    .ok_or_else(|| CliError::Custom("Invalid VLAN list".into()))?
            }
            _ => return Err(CliError::Custom("Invalid VLAN list".into())),
        };
        let mut merged = if policy.trunk_allow_additive {
            let existing = c
                .eng
                .net
                .device(c.dev)
                .and_then(|d| d.ports.get(port))
                .and_then(|p| p.trunk_allowed.clone())
                .unwrap_or_default();
            let mut v = existing;
            v.extend(vlans);
            v
        } else {
            vlans
        };
        if policy.trunk_allow_keeps_vlan1 && !merged.contains(&1) {
            merged.push(1);
        }
        c.eng.net.set_port_trunk_allowed(c.dev, port, &merged);
        Ok(String::new())
    }

    /// 删除 VLAN：Comware `undo vlan <id>`（官方 E1）。VLAN 1 为默认 VLAN 不可删除；
    /// 非法 ID 与不存在的 VLAN 直接拒绝，不写状态。状态只由 `sim-core` 持有。
    pub fn no_vlan(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let v = c
            .number("vlan")
            .ok_or_else(|| CliError::Custom("Invalid VLAN ID".into()))?;
        let vlan = require_vlan_id(v)?;
        if vlan == sim_core::device::DEFAULT_VLAN {
            return Err(CliError::Custom(
                "The default VLAN cannot be removed".into(),
            ));
        }
        if !c.eng.net.remove_vlan(c.dev, vlan) {
            return Err(CliError::Custom("VLAN does not exist".into()));
        }
        Ok(String::new())
    }

    /// Comware `undo port trunk permit vlan { vlan-list | all }`（官方 E1/E10）。
    /// 只在已处于 Trunk 模式的端口生效；`all` / 移除后回到平台默认放行列表，
    /// 且按平台策略保留 VLAN 1。与 `trunk_allow` 共用同一状态层与策略对象。
    pub fn no_trunk_permit(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let cli_engine::IfCtx::Physical(port) = require_if(c)? else {
            return Err(CliError::Custom(
                "This command is not supported on VLAN interfaces".into(),
            ));
        };
        let policy = c.platform_policy();
        let mode = c
            .eng
            .net
            .device(c.dev)
            .and_then(|d| d.ports.get(port))
            .map(|p| p.mode);
        if mode != Some(sim_core::device::PortMode::Trunk) {
            return Err(CliError::Custom("The port is not a trunk port".into()));
        }
        let remove = c.vlan_list("vlans").unwrap_or_default();
        if remove.is_empty() {
            c.eng
                .net
                .set_port_trunk_allowed(c.dev, port, &policy.trunk_initial_list());
            return Ok(String::new());
        }
        let existing = c
            .eng
            .net
            .device(c.dev)
            .and_then(|d| d.ports.get(port))
            .and_then(|p| p.trunk_allowed.clone())
            .unwrap_or_else(|| policy.trunk_initial_list());
        let mut next: Vec<u16> = existing
            .into_iter()
            .filter(|v| !remove.contains(v))
            .collect();
        if policy.trunk_allow_keeps_vlan1 && !next.contains(&1) {
            next.push(1);
        }
        c.eng.net.set_port_trunk_allowed(c.dev, port, &next);
        Ok(String::new())
    }

    /// 「真实平台语法、模拟器尚未实现承诺语义」的防御性 handler（阶段 4A）。
    /// 只写 `simulator_notices`，不改 `sim-core` 状态、不返回厂商成功文本。
    /// 规格门禁为 `NotSimulated` 时 `exec_line_result` 在门禁处即拦截，正常路径不会调用它。
    pub fn recognized_only(c: &mut Ctx) -> Result<String, CliError> {
        c.notices.push(
            "[netemu] 该命令是所选平台的真实语法，但模拟器尚未实现承诺语义；命令未执行，设备状态未改变"
                .into(),
        );
        Ok(String::new())
    }

    // ---------- STP ----------

    fn stp_of<'a>(c: &'a mut Ctx<'_>) -> Result<&'a mut sim_core::device::StpInstance, CliError> {
        if c.device_kind() != DeviceKind::Switch {
            return Err(CliError::Custom(match c.dialect {
                Dialect::Ios => "Invalid input detected".into(),
                _ => "Command is only available on switches".into(),
            }));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        match &mut d.state {
            sim_core::device::DeviceState::Switch(sw) => Ok(&mut sw.stp),
            _ => Err(CliError::Custom("not a switch".into())),
        }
    }

    /// IOS `spanning-tree vlan <n> priority <p>` / VRP、Comware `stp priority <p>`
    pub fn stp_priority(c: &mut Ctx) -> Result<String, CliError> {
        let p = c.number("priority").unwrap_or(32768);
        if p > 61440 || !p.is_multiple_of(4096) {
            return Err(CliError::Custom(
                "Priority must be a multiple of 4096, 0-61440".into(),
            ));
        }
        stp_of(c)?.priority = p as u16;
        c.eng.stp_topology_changed();
        Ok(String::new())
    }

    /// Stage 2B / H2：`undo stp priority` 恢复默认 32768 并重新计算根桥。
    pub fn stp_no_priority(c: &mut Ctx) -> Result<String, CliError> {
        stp_of(c)?.priority = 32768;
        c.eng.stp_topology_changed();
        Ok(String::new())
    }

    /// Stage 2B / H2：`stp root primary|secondary` 按平台语义设置优先级。
    pub fn stp_root(c: &mut Ctx) -> Result<String, CliError> {
        let which = c.word("which");
        let p = match which.to_lowercase().as_str() {
            "primary" => 0,
            "secondary" => 4096,
            _ => return Err(CliError::Custom("Invalid STP root role".into())),
        };
        stp_of(c)?.priority = p;
        c.eng.stp_topology_changed();
        Ok(String::new())
    }

    /// IOS `spanning-tree vlan <n>` / VRP、Comware `stp enable`
    pub fn stp_enable(c: &mut Ctx, up: bool) -> Result<String, CliError> {
        stp_of(c)?.enabled = up;
        c.eng.stp_topology_changed();
        Ok(String::new())
    }

    fn stp_set_mode(c: &mut Ctx, mode: sim_core::device::StpMode) -> Result<String, CliError> {
        stp_of(c)?.mode = mode;
        c.eng.stp_topology_changed();
        Ok(String::new())
    }

    /// IOS `spanning-tree mode rapid-pvst`
    pub fn stp_mode_rstp(c: &mut Ctx) -> Result<String, CliError> {
        stp_set_mode(c, sim_core::device::StpMode::Rstp)
    }

    /// IOS `spanning-tree mode pvst` / VRP `stp mode stp`
    pub fn stp_mode_stp(c: &mut Ctx) -> Result<String, CliError> {
        stp_set_mode(c, sim_core::device::StpMode::Dot1d)
    }

    /// IOS `spanning-tree mode mst` / VRP `stp mode mstp`
    pub fn stp_mode_mst(c: &mut Ctx) -> Result<String, CliError> {
        stp_set_mode(c, sim_core::device::StpMode::Mstp)
    }

    /// 进入 MSTP 域配置视图（`spanning-tree mst configuration` / `stp region-configuration`）
    pub fn stp_enter_mst(c: &mut Ctx) -> Result<String, CliError> {
        stp_of(c)?;
        c.sess.mode = Mode::Mst;
        c.sess.cur_if = None;
        Ok(String::new())
    }

    /// 实例优先级：`spanning-tree mst 1 priority 4096` / `stp instance 1 priority 4096`
    pub fn stp_mst_priority(c: &mut Ctx) -> Result<String, CliError> {
        let id = c.number("id").unwrap_or(0);
        if !(1..=4094).contains(&id) {
            return Err(CliError::Custom("Invalid MST instance id".into()));
        }
        let p = c.number("priority").unwrap_or(32768);
        if p > 61440 || !p.is_multiple_of(4096) {
            return Err(CliError::Custom(
                "Priority must be a multiple of 4096, 0-61440".into(),
            ));
        }
        stp_of(c)?.ensure_mst(id as u16).priority = p as u16;
        c.eng.stp_topology_changed();
        Ok(String::new())
    }

    /// 实例映射 VLAN：`instance 1 vlan 10 20`
    pub fn stp_mst_instance_vlan(c: &mut Ctx) -> Result<String, CliError> {
        let id = c.number("id").unwrap_or(0);
        if !(1..=4094).contains(&id) {
            return Err(CliError::Custom("Invalid MST instance id".into()));
        }
        let vlans = c
            .vlan_list("vlans")
            .ok_or_else(|| CliError::Custom("Invalid VLAN list".into()))?;
        let stp = stp_of(c)?;
        // 同一 VLAN 只能属于一个实例：先从其它实例摘除
        for m in stp.mst.values_mut() {
            m.vlans.retain(|v| !vlans.contains(v));
        }
        for v in &vlans {
            stp.vlan_instance.insert(*v, id as u16);
        }
        let m = stp.ensure_mst(id as u16);
        m.vlans = vlans;
        m.vlans.sort_unstable();
        m.vlans.dedup();
        stp.mode = sim_core::device::StpMode::Mstp;
        c.eng.stp_topology_changed();
        Ok(String::new())
    }

    /// MSTP 域名/修订号等仅接受不落库（无域协商）。
    pub fn stp_mst_noop(_c: &mut Ctx) -> Result<String, CliError> {
        Ok(String::new())
    }

    /// 接口成本：IOS `spanning-tree vlan <n> cost <c>` / VRP、Comware `stp cost <c>`
    pub fn stp_cost(c: &mut Ctx) -> Result<String, CliError> {
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let cost = c.number("cost").unwrap_or(4);
        if cost == 0 || cost > 200_000_000 {
            return Err(CliError::Custom("Invalid cost".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let Some(pt) = d.port_mut(port) {
                pt.stp_cost = Some(cost);
            }
        }
        c.eng.stp_topology_changed();
        Ok(String::new())
    }

    // ---------- DHCP 服务器 ----------

    /// IOS `ip dhcp pool NAME` → Pool 模式；VRP/Comware `ip pool NAME` / `dhcp server ip-pool NAME`
    pub fn dhcp_pool_enter(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(match c.dialect {
                Dialect::Ios => "Invalid input detected".into(),
                _ => "Command is only available on routers".into(),
            }));
        }
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Pool name required".into()));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        if !rs.dhcp_pools.iter().any(|p| p.name == name) {
            rs.dhcp_pools.push(sim_core::device::DhcpPool {
                name: name.clone(),
                network: 0,
                prefix: 24,
                gateway: 0,
                dns: 0,
            });
        }
        c.sess.mode = Mode::Pool;
        c.sess.sub.pool = Some(name);
        Ok(String::new())
    }

    /// Pool 视图：IOS `network A.B.C.D mask` / VRP、Comware `network A.B.C.D mask X` + gateway-list
    pub fn pool_network(c: &mut Ctx) -> Result<String, CliError> {
        let net_ip = c.ipv4("net");
        let len = c.mask_len("mask");
        let Some(name) = c.sess.sub.pool.clone() else {
            return Err(CliError::Custom("no pool selected".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        match rs.dhcp_pools.iter_mut().find(|p| p.name == name) {
            Some(p) => {
                p.network = net_ip & sim_packets::mask_from_prefix(len);
                p.prefix = len;
            }
            None => return Err(CliError::Custom("pool not found".into())),
        }
        Ok(String::new())
    }

    pub fn pool_gateway(c: &mut Ctx) -> Result<String, CliError> {
        let gw = c.ipv4("gw");
        let Some(name) = c.sess.sub.pool.clone() else {
            return Err(CliError::Custom("no pool selected".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        match rs.dhcp_pools.iter_mut().find(|p| p.name == name) {
            Some(p) => p.gateway = gw,
            None => return Err(CliError::Custom("pool not found".into())),
        }
        Ok(String::new())
    }

    pub fn pool_dns(c: &mut Ctx) -> Result<String, CliError> {
        let dns = c.ipv4("dns");
        let Some(name) = c.sess.sub.pool.clone() else {
            return Err(CliError::Custom("no pool selected".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        match rs.dhcp_pools.iter_mut().find(|p| p.name == name) {
            Some(p) => p.dns = dns,
            None => return Err(CliError::Custom("pool not found".into())),
        }
        Ok(String::new())
    }

    /// 接口启用 DHCP 服务器（VRP `dhcp select global` / Comware `dhcp select server`）。
    /// Stage 2B / C3：不再静默忽略，写入真实服务器接口列表。
    pub fn dhcp_select(c: &mut Ctx) -> Result<String, CliError> {
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let has_pool = c.eng.net.device(c.dev).is_some_and(
            |d| matches!(&d.state, DeviceState::Router(rs) if !rs.dhcp_pools.is_empty()),
        );
        if !has_pool {
            return Err(CliError::Custom("No DHCP pool configured".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if !rs.dhcp_server_ports.contains(&port) {
                    rs.dhcp_server_ports.push(port);
                }
            }
        }
        Ok(String::new())
    }

    /// Stage 2B / C3：`no ip dhcp pool <name>` 删除池并清理其租约。
    pub fn dhcp_pool_remove(c: &mut Ctx) -> Result<String, CliError> {
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Pool name required".into()));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        let Some(i) = rs.dhcp_pools.iter().position(|p| p.name == name) else {
            return Err(CliError::Custom("Pool not found".into()));
        };
        // 完整预检后一次提交：先算出待清理租约，再改状态。
        let pool = rs.dhcp_pools[i].clone();
        let mask = sim_packets::mask_from_prefix(pool.prefix);
        let leases_to_remove: Vec<usize> = rs
            .dhcp_leases
            .iter()
            .enumerate()
            .filter(|(_, l)| (l.ip & mask) == (pool.network & mask))
            .map(|(i, _)| i)
            .collect();
        for j in leases_to_remove.into_iter().rev() {
            rs.dhcp_leases.remove(j);
        }
        rs.dhcp_pools.remove(i);
        Ok(String::new())
    }

    /// Stage 2B / C3：`ip dhcp excluded-address <low> <high>`。
    pub fn dhcp_exclude(c: &mut Ctx) -> Result<String, CliError> {
        let low = c.ipv4("low");
        let high = c.ipv4("high");
        if low == 0 || high == 0 || low > high {
            return Err(CliError::Custom("Invalid excluded address range".into()));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        if !rs.dhcp_excluded.contains(&(low, high)) {
            rs.dhcp_excluded.push((low, high));
        }
        Ok(String::new())
    }

    /// Stage 2B / C3：`no ip dhcp excluded-address <low> <high>`。
    pub fn dhcp_no_exclude(c: &mut Ctx) -> Result<String, CliError> {
        let low = c.ipv4("low");
        let high = c.ipv4("high");
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        let Some(i) = rs.dhcp_excluded.iter().position(|x| *x == (low, high)) else {
            return Err(CliError::Custom("Excluded range not found".into()));
        };
        rs.dhcp_excluded.remove(i);
        Ok(String::new())
    }

    /// Stage 2B / C3：Pool 视图否定形式。`field` 由独立方言命令节点指定，
    /// 共享 handler 不按 dialect 隐式分支。
    fn pool_no_field(c: &mut Ctx, field: &str) -> Result<String, CliError> {
        let Some(name) = c.sess.sub.pool.clone() else {
            return Err(CliError::Custom("no pool selected".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        let Some(p) = rs.dhcp_pools.iter_mut().find(|p| p.name == name) else {
            return Err(CliError::Custom("pool not found".into()));
        };
        match field {
            "network" => {
                p.network = 0;
                p.prefix = 24;
            }
            "gateway" => p.gateway = 0,
            "dns" => p.dns = 0,
            _ => return Err(CliError::Custom("Invalid pool negation".into())),
        }
        Ok(String::new())
    }

    pub fn pool_no_network(c: &mut Ctx) -> Result<String, CliError> {
        pool_no_field(c, "network")
    }

    pub fn pool_no_gateway(c: &mut Ctx) -> Result<String, CliError> {
        pool_no_field(c, "gateway")
    }

    pub fn pool_no_dns(c: &mut Ctx) -> Result<String, CliError> {
        pool_no_field(c, "dns")
    }

    /// Stage 2B / C3：从真实租约/地址池渲染 `show ip dhcp binding`。
    pub fn show_dhcp_binding(c: &mut Ctx) -> Result<String, CliError> {
        Ok(show::dhcp_binding_table(c.eng, c.dev, c.dialect))
    }

    // ---------- ACL ----------

    /// VRP/Comware `acl [basic|advanced] <n>` → Acl 模式
    pub fn acl_enter(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        // `acl 3000`：数字被 Word(kind) 参数吞掉的情况 → 从 kind 解析编号
        let (kind_word, n_from_kind) = match c.val("kind") {
            Some(cli_engine::Val::Word(w)) => match w.parse::<u32>() {
                Ok(n2) => (None, Some(n2)),
                Err(_) => (Some(w.clone()), None),
            },
            _ => (None, None),
        };
        let n = c.number("number").or(n_from_kind).unwrap_or(0);
        let advanced = match &kind_word {
            Some(w) => w.eq_ignore_ascii_case("advanced"),
            None => (3000..=3999).contains(&n),
        };
        let ok_range = if advanced {
            (3000..=3999).contains(&n)
        } else {
            (2000..=2999).contains(&n) || (1..=199).contains(&n)
        };
        if !ok_range {
            return Err(CliError::Custom("Invalid ACL number".into()));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        if !rs.acls.iter().any(|a| a.number == n) {
            rs.acls.push(sim_core::device::Acl {
                number: n,
                rules: vec![],
            });
        }
        c.sess.mode = Mode::Acl;
        c.sess.sub.acl = Some((n, advanced));
        Ok(String::new())
    }

    /// ACL 视图：`rule {deny|permit} [icmp] source X W [destination X W]`
    pub fn acl_rule(c: &mut Ctx) -> Result<String, CliError> {
        acl_rule_with(c, None)
    }

    pub fn acl_rule_icmp(c: &mut Ctx) -> Result<String, CliError> {
        acl_rule_with(c, Some(AclProto::Icmp))
    }

    fn acl_rule_with(c: &mut Ctx, forced: Option<AclProto>) -> Result<String, CliError> {
        let Some((no, _adv)) = c.sess.sub.acl else {
            return Err(CliError::Custom("no acl selected".into()));
        };
        let permit = c.word("action").eq_ignore_ascii_case("permit");
        let proto = forced.unwrap_or(match c.val("proto") {
            Some(cli_engine::Val::Word(w)) if w.eq_ignore_ascii_case("icmp") => AclProto::Icmp,
            _ => AclProto::Any,
        });
        let src = c.ipv4("src");
        let src_w = c.ipv4("src_wildcard");
        let dst = c.ipv4("dst");
        let dst_w = c.ipv4("dst_wildcard");
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        let Some(acl) = rs.acls.iter_mut().find(|a| a.number == no) else {
            return Err(CliError::Custom("acl not found".into()));
        };
        acl.rules.push(AclRule {
            permit,
            proto,
            src,
            src_wildcard: src_w,
            dst,
            dst_wildcard: dst_w,
            src_port: None,
            dst_port: None,
            hits: 0,
        });
        Ok(String::new())
    }

    /// IOS 全局：`access-list <n> {deny|permit} [icmp] A.B.C.D w [A.B.C.D w]`
    pub fn ios_access_list(c: &mut Ctx) -> Result<String, CliError> {
        ios_access_list_with(c, None)
    }

    pub fn ios_access_list_icmp(c: &mut Ctx) -> Result<String, CliError> {
        ios_access_list_with(c, Some(AclProto::Icmp))
    }

    /// Stage 2B / C5：`access-list <n> ... tcp ...` 显式 TCP 规则。
    pub fn ios_access_list_tcp(c: &mut Ctx) -> Result<String, CliError> {
        ios_access_list_with(c, Some(AclProto::Tcp))
    }

    fn ios_access_list_with(c: &mut Ctx, forced: Option<AclProto>) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom("Invalid input detected".into()));
        }
        let n = c.number("number").unwrap_or(0);
        let permit = c.word("action").eq_ignore_ascii_case("permit");
        let proto = forced.unwrap_or(match c.val("proto") {
            Some(cli_engine::Val::Word(w)) if w.eq_ignore_ascii_case("icmp") => AclProto::Icmp,
            _ => AclProto::Any,
        });
        let src = c.ipv4("src");
        let src_w = c.ipv4("src_wildcard");
        let dst = c.ipv4("dst");
        let dst_w = c.ipv4("dst_wildcard");
        // Stage 2B / C5：TCP 端口槽在借用引擎之前读取，保证完整预检后一次提交。
        let src_port = c.number("sport").map(|n| n as u16);
        let dst_port = c.number("dport").map(|n| n as u16);
        if (src_port.is_some() || dst_port.is_some()) && proto != AclProto::Tcp {
            return Err(CliError::Custom(
                "Ports require an explicit tcp rule".into(),
            ));
        }
        // 基础 ACL (1-99) 只允许源
        let advanced = (100..=199).contains(&n);
        if !advanced && !(1..=99).contains(&n) {
            return Err(CliError::Custom(
                "ACL number must be 1-99 or 100-199".into(),
            ));
        }
        if !advanced && (dst != 0 || dst_w != 0) {
            return Err(CliError::Custom(
                "Basic ACL (1-99) cannot match destination".into(),
            ));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        let acl = match rs.acls.iter_mut().find(|a| a.number == n) {
            Some(a) => a,
            None => {
                rs.acls.push(Acl {
                    number: n,
                    rules: vec![],
                });
                rs.acls.last_mut().unwrap()
            }
        };
        acl.rules.push(AclRule {
            permit,
            proto,
            src,
            src_wildcard: src_w,
            dst,
            dst_wildcard: dst_w,
            src_port,
            dst_port,
            hits: 0,
        });
        Ok(String::new())
    }

    /// 接口应用：IOS `ip access-group <n> in|out` / VRP `traffic-filter <dir> acl <n>` / Comware `packet-filter <n> <dir>`
    pub fn acl_apply(c: &mut Ctx) -> Result<String, CliError> {
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let n = c.number("acl").unwrap_or(0);
        let dir = c.word("dir");
        let inbound = dir.eq_ignore_ascii_case("in") || dir.eq_ignore_ascii_case("inbound");
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        if let Some(p) = d.port_mut(port) {
            if inbound {
                p.acl_in = Some(n);
            } else {
                p.acl_out = Some(n);
            }
        }
        Ok(String::new())
    }

    // ---------- NAT ----------

    /// 接口标记：IOS `ip nat inside`
    pub fn nat_mark_inside(c: &mut Ctx) -> Result<String, CliError> {
        nat_mark_side(c, true)
    }

    /// 接口标记：IOS `ip nat outside`
    pub fn nat_mark_outside(c: &mut Ctx) -> Result<String, CliError> {
        nat_mark_side(c, false)
    }

    fn nat_mark_side(c: &mut Ctx, inside: bool) -> Result<String, CliError> {
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        if inside {
            if !rs.nat_inside.contains(&port) {
                rs.nat_inside.push(port);
            }
            rs.nat_outside.retain(|&x| x != port);
        } else {
            if !rs.nat_outside.contains(&port) {
                rs.nat_outside.push(port);
            }
            rs.nat_inside.retain(|&x| x != port);
        }
        Ok(String::new())
    }

    /// VRP/Comware 接口：`nat outbound`（Easy-IP）
    pub fn nat_outbound(c: &mut Ctx) -> Result<String, CliError> {
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        if !rs.nat_outside.contains(&port) {
            rs.nat_outside.push(port);
        }
        rs.nat_inside.retain(|&x| x != port);
        Ok(String::new())
    }

    /// IOS 全局：`ip nat inside source list <n> interface <if> overload`（EasyIP）。
    /// Stage 2B / C4：不再静默忽略 `interface` 参数；完整预检（ACL 存在、
    /// 接口名可解析且已标记 outside）后一次提交真实绑定。
    pub fn nat_easyip_ios(c: &mut Ctx) -> Result<String, CliError> {
        let acl = c.number("acl").unwrap_or(0);
        if acl == 0 {
            return Err(CliError::Custom("Invalid ACL number".into()));
        }
        let Some(d) = c.eng.net.device(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        if !rs.acls.iter().any(|a| a.number == acl) {
            return Err(CliError::Custom("ACL not found".into()));
        }
        let Some(outside) = rs.nat_outside.first().copied() else {
            return Err(CliError::Custom(
                "No NAT outside interface configured".into(),
            ));
        };
        // 命令中显式给出的接口必须解析到已标记的 outside 口。
        let ifname = c.word("if");
        if !ifname.is_empty() {
            let resolved = crate::ifname::resolve(c.dialect, d.kind, &ifname);
            if resolved != Some(outside) {
                return Err(CliError::Custom(
                    "Interface is not a NAT outside interface".into(),
                ));
            }
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                rs.nat_easyip = Some((acl, outside));
            }
        }
        Ok(String::new())
    }

    /// 静态 NAT：IOS `ip nat inside source static A B` / VRP、Comware `nat static global B inside A`
    pub fn nat_static(c: &mut Ctx) -> Result<String, CliError> {
        let inside = c.ipv4("inside");
        let outside = c.ipv4("outside");
        if inside == 0 || outside == 0 {
            return Err(CliError::Custom("Invalid NAT addresses".into()));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        if !rs.nat_static.contains(&(inside, outside)) {
            rs.nat_static.push((inside, outside));
        }
        Ok(String::new())
    }

    // ---------- 管理面（本地用户 / VTY / 远程登录） ----------

    fn require_mgmt_device(c: &Ctx) -> Result<(), CliError> {
        if matches!(
            c.device_kind(),
            DeviceKind::Router | DeviceKind::Switch | DeviceKind::Firewall
        ) {
            Ok(())
        } else {
            Err(CliError::Custom(
                "Command is only available on network devices".into(),
            ))
        }
    }

    /// IOS：`username <name> [privilege <0-15>] secret <pw>`
    pub fn mgmt_username(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let name = c.word("name");
        let secret = c.word("secret");
        if name.is_empty() || secret.is_empty() {
            return Err(CliError::Custom("Invalid username or secret".into()));
        }
        let privilege = c.number("privilege").unwrap_or(1);
        if privilege > 15 {
            return Err(CliError::Custom("Invalid privilege level".into()));
        }
        let privilege = privilege as u8;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            let u = d.mgmt.ensure_user(&name);
            u.secret = secret;
            u.privilege = privilege;
        }
        Ok(String::new())
    }

    /// IOS：`enable secret <pw>`
    pub fn mgmt_enable_secret(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let secret = c.word("secret");
        if secret.is_empty() {
            return Err(CliError::Custom("Invalid secret".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.enable_secret = Some(secret);
        }
        Ok(String::new())
    }

    /// `line vty 0 4` / `user-interface vty 0 4` → Line 视图。
    pub fn mgmt_enter_line(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        c.sess.mode = Mode::Line;
        c.sess.cur_if = None;
        Ok(String::new())
    }

    /// IOS：`login local`（启用本地用户库认证）
    pub fn mgmt_login_local(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.auth_local = true;
            d.mgmt.line_auth = sim_core::device::LineAuthMode::Local;
        }
        Ok(String::new())
    }

    /// Stage 2B / C6a：裸 `login` = 线路口令认证，与 `login local` 分离。
    /// 完整预检：线路口令必须已配置，否则拒绝且不改变状态。
    pub fn mgmt_login_line(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let has_password = c
            .eng
            .net
            .device(c.dev)
            .is_some_and(|d| d.mgmt.line_password.is_some());
        if !has_password {
            return Err(CliError::Custom(
                "% Password required on this line but not set".into(),
            ));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.auth_local = false;
            d.mgmt.line_auth = sim_core::device::LineAuthMode::Line;
        }
        Ok(String::new())
    }

    /// Stage 2B / C6a：`password <word>` 设置线路口令；口令不进入任何设备输出。
    pub fn mgmt_line_password(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let pw = c.word("secret");
        if pw.is_empty() {
            return Err(CliError::Custom("Invalid password".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.line_password = Some(pw);
        }
        Ok(String::new())
    }

    /// Stage 2B / C6a：`no login` 关闭线路认证。
    pub fn mgmt_no_login(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.auth_local = false;
            d.mgmt.line_auth = sim_core::device::LineAuthMode::None;
        }
        Ok(String::new())
    }

    /// Stage 2B / C6a：`no enable secret` 清除特权口令。
    pub fn mgmt_no_enable_secret(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.enable_secret = None;
        }
        Ok(String::new())
    }

    fn parse_transport(list: &[String]) -> Option<VtyTransport> {
        let words: Vec<String> = list.iter().map(|s| s.to_lowercase()).collect();
        if words.iter().any(|w| w == "none") {
            return Some(VtyTransport::None);
        }
        let ssh = words
            .iter()
            .any(|w| w == "ssh" || w == "all" || w == "ssh-only");
        let telnet = words.iter().any(|w| w == "telnet" || w == "all");
        match (ssh, telnet) {
            (true, true) => Some(VtyTransport::All),
            (true, false) => Some(VtyTransport::Ssh),
            (false, true) => Some(VtyTransport::Telnet),
            (false, false) => None,
        }
    }

    /// `transport input ssh|telnet|all|none` / `protocol inbound ...`
    pub fn mgmt_transport(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let list = c.word_list("protos").unwrap_or_default();
        let transport = parse_transport(&list)
            .ok_or_else(|| CliError::Custom("Invalid transport protocol".into()))?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.transport = transport;
        }
        Ok(String::new())
    }

    /// VRP：`aaa` → AAA 视图
    pub fn mgmt_enter_aaa(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        c.sess.mode = Mode::Aaa;
        c.sess.cur_if = None;
        c.sess.sub.aaa_user = None;
        Ok(String::new())
    }

    /// Comware：`local-user <name> class manage` → AAA 用户视图
    pub fn mgmt_local_user_enter(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Invalid username".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.ensure_user(&name);
        }
        c.sess.mode = Mode::Aaa;
        c.sess.cur_if = None;
        c.sess.sub.aaa_user = Some(name);
        Ok(String::new())
    }

    /// AAA 视图中定位用户：命令带 name 参数，否则用当前视图用户。
    fn aaa_user_name(c: &Ctx) -> Option<String> {
        let n = c.word("name");
        if !n.is_empty() {
            Some(n)
        } else {
            c.sess.sub.aaa_user.clone()
        }
    }

    /// `local-user <name> password cipher|simple <pw>` / AAA 视图 `password ...`
    pub fn mgmt_user_password(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let Some(name) = aaa_user_name(c) else {
            return Err(CliError::Custom("No local user selected".into()));
        };
        let secret = c.word("secret");
        if secret.is_empty() {
            return Err(CliError::Custom("Invalid password".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.ensure_user(&name).secret = secret;
        }
        Ok(String::new())
    }

    /// `local-user <name> privilege level <n>` / `authorization-attribute user-role ...`
    pub fn mgmt_user_privilege(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let Some(name) = aaa_user_name(c) else {
            return Err(CliError::Custom("No local user selected".into()));
        };
        let level = c.number("privilege").unwrap_or(1);
        if level > 15 {
            return Err(CliError::Custom("Invalid privilege level".into()));
        }
        let level = level as u8;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.ensure_user(&name).privilege = level;
        }
        Ok(String::new())
    }

    /// `authorization-attribute user-role network-admin|network-operator`（Comware）
    pub fn mgmt_user_role(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let Some(name) = aaa_user_name(c) else {
            return Err(CliError::Custom("No local user selected".into()));
        };
        let role = c.word("role").to_lowercase();
        let level = if role == "network-admin" { 15 } else { 1 };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.ensure_user(&name).privilege = level;
        }
        Ok(String::new())
    }

    /// `service-type telnet ssh` / `service-type ssh`
    pub fn mgmt_user_service(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let Some(name) = aaa_user_name(c) else {
            return Err(CliError::Custom("No local user selected".into()));
        };
        let words: Vec<String> = c
            .word_list("services")
            .unwrap_or_default()
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        let all = words.iter().any(|w| w == "all");
        let telnet = all || words.iter().any(|w| w == "telnet");
        let ssh = all || words.iter().any(|w| w == "ssh");
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            let u = d.mgmt.ensure_user(&name);
            u.telnet = telnet;
            u.ssh = ssh;
        }
        Ok(String::new())
    }

    /// Line 视图：`authentication-mode aaa|scheme|password|none`
    pub fn mgmt_auth_mode(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let mode = c.word("mode").to_lowercase();
        let local = matches!(mode.as_str(), "aaa" | "scheme");
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.mgmt.auth_local = local;
        }
        Ok(String::new())
    }

    fn login_start(c: &mut Ctx, proto: MgmtProto) -> Result<String, CliError> {
        let ip = c.ipv4("target");
        if ip == 0 {
            return Err(CliError::Custom("Invalid IP address".into()));
        }
        match c.eng.login_target(c.dev, ip, proto) {
            Ok(target) => {
                c.sess.login = Some(LoginFlow {
                    target,
                    target_ip: ip,
                    proto,
                    step: LoginStep::Username,
                    user: String::new(),
                });
                Ok(match (c.dialect, proto) {
                    (Dialect::Ios, MgmtProto::Telnet) => {
                        format!("Trying {} ... Open", ipv4_str(ip))
                    }
                    (Dialect::Ios, MgmtProto::Ssh) => format!("Connecting to {} ...", ipv4_str(ip)),
                    (_, _) => format!("  Trying {} ...", ipv4_str(ip)),
                })
            }
            Err(e) => Ok(login_error_text(c.device_kind(), c.dialect, e)),
        }
    }

    pub fn telnet_login(c: &mut Ctx) -> Result<String, CliError> {
        login_start(c, MgmtProto::Telnet)
    }

    pub fn ssh_login(c: &mut Ctx) -> Result<String, CliError> {
        login_start(c, MgmtProto::Ssh)
    }

    // ---------- OSPF（简化） ----------

    /// VRP 区域号既接受 `0` 也接受 `0.0.0.0` / `0.0.0.1`。
    fn parse_ospf_area(s: &str) -> Result<u32, CliError> {
        if s.is_empty() {
            return Ok(0);
        }
        if !s.contains('.') {
            return s
                .parse::<u32>()
                .map_err(|_| CliError::Custom("Invalid OSPF area".into()));
        }
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 4 {
            return Err(CliError::Custom("Invalid OSPF area".into()));
        }
        let mut v: u32 = 0;
        for p in parts {
            let b = p
                .parse::<u32>()
                .map_err(|_| CliError::Custom("Invalid OSPF area".into()))?;
            if b > 255 {
                return Err(CliError::Custom("Invalid OSPF area".into()));
            }
            v = (v << 8) | b;
        }
        Ok(v)
    }

    /// `router ospf 1` / `ospf 1`
    pub fn ospf_enter(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        // Stage 2B / C7·H6：进程号写入真实状态，供配置视图和查询使用。
        let pid = c.number("pid").unwrap_or(1);
        if pid == 0 || pid > 65535 {
            return Err(CliError::Custom("Invalid OSPF process id".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                rs.ospf_process = Some(pid);
            }
        }
        c.sess.mode = Mode::Ospf;
        c.sess.cur_if = None;
        c.sess.sub.ospf = Some((pid, 0));
        Ok(String::new())
    }

    /// VRP/Comware 区域视图：`area 0` / `area 0.0.0.0`
    pub fn ospf_area_enter(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let area = parse_ospf_area(&c.word("area"))?;
        let pid = c.sess.sub.ospf.map(|(p, _)| p).unwrap_or(1);
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                rs.ospf_area = area;
                rs.ospf_process = Some(pid);
            }
        }
        c.sess.mode = Mode::OspfArea;
        c.sess.cur_if = None;
        c.sess.sub.ospf = Some((pid, area));
        Ok(String::new())
    }

    /// `network A.B.C.D wildcard [area N]`：加入 OSPF。
    /// Stage 2B / C7·H6：区域由 IOS `area` 参数或 VRP 当前区域视图决定，
    /// 不再固定 area 0；不同区域的邻居不建邻。
    pub fn ospf_network(c: &mut Ctx) -> Result<String, CliError> {
        let net = c.ipv4("net");
        if net == 0 {
            return Err(CliError::Custom("Invalid network".into()));
        }
        let wc = match c.val("wildcard") {
            Some(cli_engine::Val::Ip(w)) => *w,
            _ => 0,
        };
        let area = match c.val("area") {
            Some(cli_engine::Val::Number(n)) => *n,
            Some(cli_engine::Val::Word(w)) => parse_ospf_area(w)?,
            _ => c.sess.sub.ospf.map(|(_, a)| a).unwrap_or(0),
        };
        let pid = c.sess.sub.ospf.map(|(p, _)| p).unwrap_or(1);
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        if rs.ospf_process.is_none() {
            rs.ospf_process = Some(pid);
        }
        rs.ospf_area = area;
        if !rs.ospf_networks.contains(&(net, wc)) {
            rs.ospf_networks.push((net, wc));
        }
        c.eng.ospf_config_changed(c.dev);
        Ok(String::new())
    }

    /// Stage 2B / C7：`no router ospf <pid>` 撤销进程、network、邻居与派生路由。
    pub fn ospf_no_process(c: &mut Ctx) -> Result<String, CliError> {
        let pid = c.number("process").unwrap_or(1);
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        if rs.ospf_process != Some(pid) && !rs.ospf_networks.is_empty() {
            return Err(CliError::Custom("OSPF process not found".into()));
        }
        rs.ospf_process = None;
        rs.ospf_area = 0;
        rs.ospf_networks.clear();
        rs.ospf_routes.clear();
        c.eng.ospf_reset(c.dev);
        Ok(String::new())
    }

    /// Stage 2B / C7·H6：`undo network <net> <wildcard>` 撤销一条 network 语句，
    /// 配置、派生路由与邻居状态同步。
    pub fn ospf_no_network(c: &mut Ctx) -> Result<String, CliError> {
        let net = c.ipv4("net");
        let wc = match c.val("wildcard") {
            Some(cli_engine::Val::Ip(w)) => *w,
            _ => 0,
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        let Some(i) = rs.ospf_networks.iter().position(|x| *x == (net, wc)) else {
            return Err(CliError::Custom("OSPF network not found".into()));
        };
        rs.ospf_networks.remove(i);
        if rs.ospf_networks.is_empty() {
            rs.ospf_routes.clear();
            c.eng.ospf_reset(c.dev);
        } else {
            c.eng.ospf_config_changed(c.dev);
        }
        Ok(String::new())
    }

    /// Stage 2B / C7：IOS `no network <net> <wildcard> area <area>`。
    /// 区域必须与实际配置一致，否则拒绝且状态不变；校验通过后复用单条 network 撤销。
    pub fn ospf_no_network_ios(c: &mut Ctx) -> Result<String, CliError> {
        let area = c.number("area").unwrap_or(0);
        let Some(d) = c.eng.net.device(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        if rs.ospf_area != area {
            return Err(CliError::Custom("OSPF area mismatch".into()));
        }
        ospf_no_network(c)
    }

    /// Stage 2B / H6：`undo area <id>` 移除当前区域及其 network 语句。
    pub fn ospf_no_area(c: &mut Ctx) -> Result<String, CliError> {
        let area = parse_ospf_area(&c.word("area"))?;
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        if rs.ospf_area != area || rs.ospf_networks.is_empty() {
            return Err(CliError::Custom("OSPF area not found".into()));
        }
        rs.ospf_networks.clear();
        rs.ospf_routes.clear();
        c.eng.ospf_reset(c.dev);
        Ok(String::new())
    }

    pub fn show_ospf_neighbor(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        Ok(show::ospf_neighbors(c.eng, c.dev, c.dialect))
    }

    pub fn ospfv3_enter(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(c.device_kind(), DeviceKind::Router) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                rs.ospfv3_enabled = true;
            }
        }
        c.sess.mode = Mode::Ospfv3;
        Ok(String::new())
    }

    pub fn ospfv3_router_id(c: &mut Ctx) -> Result<String, CliError> {
        let id = c.ipv4("id");
        if id == 0 {
            return Err(CliError::Custom("Invalid router id".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                rs.ospfv3_router_id = Some(id);
            }
        }
        c.eng.ospfv3_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn ospfv3_area(c: &mut Ctx) -> Result<String, CliError> {
        let area = c.number("area").unwrap_or(0);
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let Some(p) = d.port_mut(port) else {
            return Ok(String::new());
        };
        p.ospfv3_area = Some(area);
        c.eng.ospfv3_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn show_ospfv3_neighbor(c: &mut Ctx) -> Result<String, CliError> {
        Ok(show::ospfv3_neighbors(c.eng, c.dev, c.dialect))
    }

    pub fn show_ospfv3_db(c: &mut Ctx) -> Result<String, CliError> {
        Ok(show::ospfv3_prefixes(c.eng, c.dev, c.dialect))
    }

    fn ipv6_prefix_arg(c: &mut Ctx, key: &str) -> Result<(sim_packets::Ipv6Addr, u8), CliError> {
        let raw = c.word(key);
        let (addr, len) = raw
            .split_once('/')
            .ok_or_else(|| CliError::Custom("Expected IPv6 prefix, e.g. 2001:db8::/64".into()))?;
        let ip = sim_packets::ipv6_from_str(addr)
            .ok_or_else(|| CliError::Custom("Invalid IPv6 address".into()))?;
        let prefix: u8 = len
            .parse()
            .map_err(|_| CliError::Custom("Invalid prefix length".into()))?;
        if prefix > 128 {
            return Err(CliError::Custom("Invalid prefix length".into()));
        }
        Ok((ip, prefix))
    }

    /// `srv6 locator 2001:db8:100::/64`
    pub fn srv6_locator(c: &mut Ctx) -> Result<String, CliError> {
        let (net, len) = ipv6_prefix_arg(c, "prefix")?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                rs.srv6.locator = Some((net, len));
            }
        }
        Ok(String::new())
    }

    /// `srv6 sid 2001:db8:100::1`
    pub fn srv6_sid(c: &mut Ctx) -> Result<String, CliError> {
        let raw = c.word("sid");
        let sid = sim_packets::ipv6_from_str(&raw)
            .ok_or_else(|| CliError::Custom("Invalid SID".into()))?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if !rs.srv6.sids.contains(&sid) {
                    rs.srv6.sids.push(sid);
                }
            }
        }
        Ok(String::new())
    }

    /// `srv6 route 2001:db8:2::/64 sid 2001:db8:100::1`
    pub fn srv6_route(c: &mut Ctx) -> Result<String, CliError> {
        let (dst, len) = ipv6_prefix_arg(c, "prefix")?;
        let raw = c.word("sid");
        let sid = sim_packets::ipv6_from_str(&raw)
            .ok_or_else(|| CliError::Custom("Invalid SID".into()))?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                rs.srv6.routes.retain(|r| !(r.dst == dst && r.len == len));
                rs.srv6
                    .routes
                    .push(sim_core::device::Srv6Route { dst, len, sid });
            }
        }
        Ok(String::new())
    }

    pub fn show_srv6(c: &mut Ctx) -> Result<String, CliError> {
        Ok(show::srv6_table(c.eng, c.dev, c.dialect))
    }

    pub fn show_ospf_db(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        Ok(show::ospf_lsdb(c.eng, c.dev, c.dialect))
    }

    // ---------- VRRP（虚拟网关） ----------

    fn vrrp_if(c: &Ctx) -> Result<(sim_core::device::PortId, sim_packets::Ipv4Addr), CliError> {
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let ip = c
            .eng
            .net
            .device(c.dev)
            .and_then(|d| d.port(port))
            .and_then(|p| p.ip)
            .ok_or_else(|| {
                CliError::Custom("VRRP requires an IP address on the interface".into())
            })?;
        Ok((port, ip))
    }

    pub fn vrrp_vip(c: &mut Ctx) -> Result<String, CliError> {
        let vrid = c.number("vrid").unwrap_or(0);
        if !(1..=255).contains(&vrid) {
            return Err(CliError::Custom("VRRP group must be 1-255".into()));
        }
        let vip = c.ipv4("vip");
        if vip == 0 {
            return Err(CliError::Custom("Invalid virtual IP".into()));
        }
        let (port, _) = vrrp_if(c)?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(g) = rs.vrrp.iter_mut().find(|g| g.vrid == vrid as u8) {
                    g.vip = vip;
                    g.port = port;
                } else {
                    rs.vrrp.push(sim_core::device::VrrpGroup {
                        vrid: vrid as u8,
                        vip,
                        port,
                        priority: 100,
                        preempt: true,
                        state: sim_core::device::VrrpState::Init,
                        last_advert_ns: 0,
                    });
                }
            }
        }
        c.eng.vrrp_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn vrrp_priority(c: &mut Ctx) -> Result<String, CliError> {
        let vrid = c.number("vrid").unwrap_or(0);
        let priority = c.number("priority").unwrap_or(100);
        if !(1..=254).contains(&priority) {
            return Err(CliError::Custom("VRRP priority must be 1-254".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(g) = rs.vrrp.iter_mut().find(|g| g.vrid == vrid as u8) {
                    g.priority = priority as u8;
                }
            }
        }
        c.eng.vrrp_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn vrrp_preempt(c: &mut Ctx) -> Result<String, CliError> {
        let vrid = c.number("vrid").unwrap_or(0);
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(g) = rs.vrrp.iter_mut().find(|g| g.vrid == vrid as u8) {
                    g.preempt = true;
                }
            }
        }
        Ok(String::new())
    }

    pub fn show_vrrp(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        Ok(show::vrrp_table(c.eng, c.dev, c.dialect))
    }

    /// Stage 2B / C8：`no vrrp <vrid> preempt` 关闭抢占。
    pub fn vrrp_no_preempt(c: &mut Ctx) -> Result<String, CliError> {
        let vrid = c.number("vrid").unwrap_or(0);
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        let Some(g) = rs.vrrp.iter_mut().find(|g| g.vrid == vrid as u8) else {
            return Err(CliError::Custom("VRRP group not found".into()));
        };
        g.preempt = false;
        Ok(String::new())
    }

    /// Stage 2B / C8：`no vrrp <vrid>` 删除组，协议状态与 ARP 归属同步清除。
    pub fn vrrp_no_group(c: &mut Ctx) -> Result<String, CliError> {
        let vrid = c.number("vrid").unwrap_or(0);
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        let Some(i) = rs.vrrp.iter().position(|g| g.vrid == vrid as u8) else {
            return Err(CliError::Custom("VRRP group not found".into()));
        };
        rs.vrrp.remove(i);
        c.eng.vrrp_config_changed(c.dev);
        Ok(String::new())
    }

    // ---------- Stage 2B：ACL / NAT / mgmt 否定形式 ----------

    /// C5：`no access-list <n> <seq>` 删除第 seq 条规则（1-based）。
    pub fn acl_no_rule(c: &mut Ctx) -> Result<String, CliError> {
        let n = c.number("number").unwrap_or(0);
        let seq = c.number("seq").unwrap_or(0);
        if seq == 0 {
            return Err(CliError::Custom("Invalid rule index".into()));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        let Some(acl) = rs.acls.iter_mut().find(|a| a.number == n) else {
            return Err(CliError::Custom("ACL not found".into()));
        };
        let idx = (seq - 1) as usize;
        if idx >= acl.rules.len() {
            return Err(CliError::Custom("ACL rule not found".into()));
        }
        acl.rules.remove(idx);
        Ok(String::new())
    }

    /// C5：`no access-list <n>` 删除整条 ACL 并解除所有接口绑定。
    pub fn acl_no_all(c: &mut Ctx) -> Result<String, CliError> {
        let n = c.number("number").unwrap_or(0);
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        if !matches!(d.state, DeviceState::Router(_)) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let exists = match &d.state {
            DeviceState::Router(rs) => rs.acls.iter().any(|a| a.number == n),
            _ => false,
        };
        if !exists {
            return Err(CliError::Custom("ACL not found".into()));
        }
        if let DeviceState::Router(rs) = &mut d.state {
            rs.acls.retain(|a| a.number != n);
        }
        for p in d.ports.iter_mut() {
            if p.acl_in == Some(n) {
                p.acl_in = None;
            }
            if p.acl_out == Some(n) {
                p.acl_out = None;
            }
        }
        // ACL 可能被 route-policy 引用；删除后重算 BGP 策略派生路由（无 BGP 时为无操作）。
        c.eng.bgp_policy_changed(c.dev);
        Ok(String::new())
    }

    /// C5：`no ip access-group <n> in|out` 解除接口绑定。
    pub fn acl_no_bind(c: &mut Ctx) -> Result<String, CliError> {
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let n = c.number("number").unwrap_or(0);
        let dir = c.word("dir");
        let inbound = dir.eq_ignore_ascii_case("in") || dir.eq_ignore_ascii_case("inbound");
        let outbound = dir.eq_ignore_ascii_case("out") || dir.eq_ignore_ascii_case("outbound");
        if !inbound && !outbound {
            return Err(CliError::Custom("Invalid ACL direction".into()));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let bound = d
            .port(port)
            .and_then(|p| if inbound { p.acl_in } else { p.acl_out });
        if bound != Some(n) {
            return Err(CliError::Custom("ACL binding not found".into()));
        }
        if let Some(p) = d.port_mut(port) {
            if inbound {
                p.acl_in = None;
            } else {
                p.acl_out = None;
            }
        }
        Ok(String::new())
    }

    /// C4：`no ip nat inside source static <inside> <outside>`。
    pub fn nat_no_static(c: &mut Ctx) -> Result<String, CliError> {
        let pair = (c.ipv4("inside"), c.ipv4("outside"));
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        let Some(i) = rs.nat_static.iter().position(|x| *x == pair) else {
            return Err(CliError::Custom("NAT mapping not found".into()));
        };
        rs.nat_static.remove(i);
        Ok(String::new())
    }

    fn nat_no_role(c: &mut Ctx, inside: bool) -> Result<String, CliError> {
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        let list = if inside {
            &mut rs.nat_inside
        } else {
            &mut rs.nat_outside
        };
        let Some(i) = list.iter().position(|x| *x == port) else {
            return Err(CliError::Custom("NAT role not found".into()));
        };
        list.remove(i);
        // 角色撤销后，动态转换表同步清空，数据面不再残留可转发表项。
        rs.nat_entries.clear();
        Ok(String::new())
    }

    pub fn nat_no_inside(c: &mut Ctx) -> Result<String, CliError> {
        nat_no_role(c, true)
    }

    pub fn nat_no_outside(c: &mut Ctx) -> Result<String, CliError> {
        nat_no_role(c, false)
    }

    /// Stage 2B / C4：`no ip nat inside source list <acl> interface <if> [overload]`
    /// 撤销 EasyIP 绑定。完整预检后才提交：ACL / 接口参数都必须与当前绑定一致，
    /// 否则拒绝且状态不变；撤销同时清空派生转换表，数据面不再残留可转发表项。
    pub fn nat_no_easyip(c: &mut Ctx) -> Result<String, CliError> {
        let acl = c.number("acl").unwrap_or(0);
        if acl == 0 {
            return Err(CliError::Custom("Invalid ACL number".into()));
        }
        let ifname = c.word("if");
        if ifname.is_empty() {
            return Err(CliError::Custom("Outside interface is required".into()));
        }
        let Some(d) = c.eng.net.device(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        let Some((bound_acl, outside)) = rs.nat_easyip else {
            return Err(CliError::Custom("NAT EasyIP rule not found".into()));
        };
        if bound_acl != acl {
            return Err(CliError::Custom("NAT EasyIP ACL mismatch".into()));
        }
        let resolved = crate::ifname::resolve(c.dialect, d.kind, &ifname);
        if resolved != Some(outside) {
            return Err(CliError::Custom(
                "Interface is not the bound NAT outside interface".into(),
            ));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        rs.nat_easyip = None;
        rs.nat_entries.clear();
        Ok(String::new())
    }

    /// C6a：`no username <name>` 删除本地用户。
    pub fn mgmt_no_username(c: &mut Ctx) -> Result<String, CliError> {
        require_mgmt_device(c)?;
        let name = c.word("name");
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let Some(i) = d.mgmt.users.iter().position(|u| u.name == name) else {
            return Err(CliError::Custom("User not found".into()));
        };
        d.mgmt.users.remove(i);
        Ok(String::new())
    }

    // ---------- IPv6（简化） ----------

    /// `ipv6 address 2001:db8::1/64`
    pub fn ipv6_address(c: &mut Ctx) -> Result<String, CliError> {
        let raw = c.word("addr");
        let (addr, len) = raw.split_once('/').ok_or_else(|| {
            CliError::Custom("Expected address/prefix, e.g. 2001:db8::1/64".into())
        })?;
        let ip = sim_packets::ipv6_from_str(addr)
            .ok_or_else(|| CliError::Custom("Invalid IPv6 address".into()))?;
        let prefix: u8 = len
            .parse()
            .map_err(|_| CliError::Custom("Invalid prefix length".into()))?;
        if prefix > 128 {
            return Err(CliError::Custom("Invalid prefix length".into()));
        }
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        c.eng.net.set_port_ipv6(c.dev, port, Some(ip), prefix);
        Ok(String::new())
    }

    /// IOS `ipv6 route 2001:db8:2::/64 2001:db8:1::2` / VRP `ipv6 route-static ...`
    pub fn ipv6_route(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let prefix = c.word("prefix");
        let nh_raw = c.word("nexthop");
        let (net_s, len): (&str, u8) = match prefix.split_once('/') {
            Some((n, l)) => (
                n,
                l.parse()
                    .map_err(|_| CliError::Custom("Invalid prefix length".into()))?,
            ),
            None => (
                prefix.as_str(),
                c.number("len")
                    .filter(|n| *n <= 128)
                    .ok_or_else(|| CliError::Custom("Invalid prefix length".into()))?
                    as u8,
            ),
        };
        let dst = sim_packets::ipv6_from_str(net_s)
            .ok_or_else(|| CliError::Custom("Invalid IPv6 prefix".into()))?;
        let nh = sim_packets::ipv6_from_str(&nh_raw)
            .ok_or_else(|| CliError::Custom("Invalid next hop".into()))?;
        c.eng.net.add_static_route6(c.dev, dst, len, nh);
        Ok(String::new())
    }

    pub fn ping_ipv6(c: &mut Ctx) -> Result<String, CliError> {
        let raw = c.word("target");
        let target = sim_packets::ipv6_from_str(&raw)
            .ok_or_else(|| CliError::Custom("Invalid IPv6 address".into()))?;
        c.eng.start_ping6(c.dev, target);
        Ok(String::new())
    }

    pub fn show_ipv6_brief(c: &mut Ctx) -> Result<String, CliError> {
        Ok(show::ipv6_int_brief(c.eng, c.dev, c.dialect))
    }

    pub fn show_ipv6_route(c: &mut Ctx) -> Result<String, CliError> {
        Ok(show::ipv6_route_table(c.eng, c.dev, c.dialect))
    }

    // ---------- 二层安全与 QoS ----------

    /// 全局开启 DHCP Snooping。
    pub fn dhcp_snooping_enable(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                sw.dhcp_snooping = true;
            }
        }
        Ok(String::new())
    }

    /// 接口设为 DHCP Snooping 信任端口。
    pub fn dhcp_snooping_trust(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let Some(p) = d.port_mut(port) {
                p.dhcp_trust = true;
            }
        }
        Ok(String::new())
    }

    fn port_sec_upsert(
        c: &mut Ctx,
        f: impl FnOnce(&mut sim_core::device::PortSecurity),
    ) -> Result<String, CliError> {
        require_switch(c)?;
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let Some(p) = d.port_mut(port) else {
            return Ok(String::new());
        };
        let cfg = p
            .port_security
            .get_or_insert(sim_core::device::PortSecurity {
                max: 1,
                violation: sim_core::device::PortSecViolation::Restrict,
                sticky: false,
            });
        f(cfg);
        Ok(String::new())
    }

    pub fn port_security_enable(c: &mut Ctx) -> Result<String, CliError> {
        port_sec_upsert(c, |_| {})
    }

    pub fn port_security_max(c: &mut Ctx) -> Result<String, CliError> {
        let n = c.number("max").unwrap_or(1);
        if !(1..=1024).contains(&n) {
            return Err(CliError::Custom("max-mac must be 1-1024".into()));
        }
        port_sec_upsert(c, |cfg| cfg.max = n as u16)
    }

    pub fn port_security_violation(c: &mut Ctx) -> Result<String, CliError> {
        let w = c.word("action").to_lowercase();
        let v = match w.as_str() {
            "shutdown" => sim_core::device::PortSecViolation::Shutdown,
            "restrict" => sim_core::device::PortSecViolation::Restrict,
            _ => {
                return Err(CliError::Custom(
                    "violation must be shutdown or restrict".into(),
                ))
            }
        };
        port_sec_upsert(c, |cfg| cfg.violation = v)
    }

    pub fn port_security_sticky(c: &mut Ctx) -> Result<String, CliError> {
        port_sec_upsert(c, |cfg| cfg.sticky = true)
    }

    /// 端口隔离：`port-isolate enable group N` / `switchport protected`。
    pub fn port_isolate_enable(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let group = c.number("group").unwrap_or(0) as u16;
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let Some(p) = d.port_mut(port) {
                p.isolate_group = Some(group);
            }
        }
        Ok(String::new())
    }

    /// 思科 `switchport protected`：所有 protected 端口两两隔离。
    pub fn switchport_protected(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let Some(p) = d.port_mut(port) {
                p.isolate_group = Some(0);
            }
        }
        Ok(String::new())
    }

    fn mux_set(c: &mut Ctx, role: sim_core::device::MuxRole) -> Result<String, CliError> {
        require_switch(c)?;
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let Some(p) = d.port_mut(port) {
                p.mux_role = Some(role);
            }
        }
        Ok(String::new())
    }

    pub fn mux_vlan_principal(c: &mut Ctx) -> Result<String, CliError> {
        mux_set(c, sim_core::device::MuxRole::Principal)
    }

    pub fn mux_vlan_separate(c: &mut Ctx) -> Result<String, CliError> {
        mux_set(c, sim_core::device::MuxRole::Separate)
    }

    pub fn mux_vlan_group(c: &mut Ctx) -> Result<String, CliError> {
        let group = c.number("group").unwrap_or(0) as u16;
        mux_set(c, sim_core::device::MuxRole::Group(group))
    }

    pub fn show_port_isolate(c: &mut Ctx) -> Result<String, CliError> {
        Ok(show::port_isolate(c.eng, c.dev, c.dialect))
    }

    pub fn show_mux_vlan(c: &mut Ctx) -> Result<String, CliError> {
        Ok(show::mux_vlan(c.eng, c.dev, c.dialect))
    }

    /// 广播风暴抑制（每秒广播帧上限）。
    pub fn storm_control(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let pps = c.number("pps").unwrap_or(0);
        if !(1..=1_000_000).contains(&pps) {
            return Err(CliError::Custom("Invalid broadcast limit".into()));
        }
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let Some(p) = d.port_mut(port) {
                p.broadcast_limit_pps = Some(pps);
            }
        }
        Ok(String::new())
    }

    /// 接口入口限速（kbps）。
    pub fn rate_limit(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let kbps = c.number("kbps").unwrap_or(0);
        if !(1..=1_000_000).contains(&kbps) {
            return Err(CliError::Custom("Invalid rate limit".into()));
        }
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let Some(p) = d.port_mut(port) {
                p.rate_limit_kbps = Some(kbps);
            }
        }
        Ok(String::new())
    }

    pub fn show_dhcp_snooping(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(show::dhcp_snooping_table(c.eng, c.dev, c.dialect))
    }

    pub fn show_port_security(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(show::port_security_table(c.eng, c.dev, c.dialect))
    }

    pub fn show_qos(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(show::qos_table(c.eng, c.dev, c.dialect))
    }

    // ---------- BGP（eBGP 简化） ----------

    pub fn bgp_enter(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let asn = c.number("asn").unwrap_or(0);
        if asn == 0 {
            return Err(CliError::Custom("Invalid AS number".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                match &mut rs.bgp {
                    Some(b) => b.asn = asn,
                    None => {
                        rs.bgp = Some(sim_core::device::BgpConfig {
                            asn,
                            neighbors: vec![],
                            networks: vec![],
                            peer_policies: vec![],
                        })
                    }
                }
            }
        }
        c.sess.mode = Mode::Bgp;
        c.sess.cur_if = None;
        Ok(String::new())
    }

    pub fn bgp_neighbor(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let ip = c.ipv4("ip");
        let asn = c.number("asn").unwrap_or(0);
        if ip == 0 || asn == 0 {
            return Err(CliError::Custom("Invalid neighbor".into()));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        let Some(bgp) = rs.bgp.as_mut() else {
            return Err(CliError::Custom("BGP process not configured".into()));
        };
        if let Some(n) = bgp.neighbors.iter_mut().find(|(n, _)| *n == ip) {
            n.1 = asn;
        } else {
            bgp.neighbors.push((ip, asn));
        }
        c.eng.bgp_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn bgp_network(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let net = c.ipv4("net");
        let len = c.mask_len("mask");
        if net == 0 {
            return Err(CliError::Custom("Invalid network".into()));
        }
        let net = net & sim_packets::mask_from_prefix(len);
        {
            let Some(d) = c.eng.net.device_mut(c.dev) else {
                return Ok(String::new());
            };
            let DeviceState::Router(rs) = &mut d.state else {
                return Ok(String::new());
            };
            let Some(bgp) = rs.bgp.as_mut() else {
                return Err(CliError::Custom("BGP process not configured".into()));
            };
            if !bgp.networks.contains(&(net, len)) {
                bgp.networks.push((net, len));
            }
        }
        c.eng.bgp_config_changed(c.dev);
        Ok(String::new())
    }

    /// VRP 配置视图：`undo bgp <asn>` 撤销 BGP 进程。
    /// ASN 必须与当前进程一致，否则拒绝且状态不变；提交后清空会话/已学习路由。
    pub fn bgp_no_process(c: &mut Ctx) -> Result<String, CliError> {
        let asn = c.number("asn").unwrap_or(0);
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        if !rs.bgp.as_ref().is_some_and(|b| b.asn == asn) {
            return Err(CliError::Custom("BGP process not found".into()));
        }
        rs.bgp = None;
        c.eng.bgp_reset(c.dev);
        Ok(String::new())
    }

    /// VRP BGP 视图：`undo peer <ip>` 删除邻居及其策略绑定，会话/路由同步撤销。
    pub fn bgp_no_neighbor(c: &mut Ctx) -> Result<String, CliError> {
        let ip = c.ipv4("ip");
        if ip == 0 {
            return Err(CliError::Custom("Invalid peer address".into()));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        let Some(bgp) = rs.bgp.as_mut() else {
            return Err(CliError::Custom("BGP process not configured".into()));
        };
        if !bgp.neighbors.iter().any(|(n, _)| *n == ip) {
            return Err(CliError::Custom("BGP neighbor not configured".into()));
        }
        bgp.neighbors.retain(|(n, _)| *n != ip);
        bgp.peer_policies.retain(|p| p.neighbor != ip);
        c.eng.bgp_config_changed(c.dev);
        Ok(String::new())
    }

    /// VRP BGP 视图：`undo network <net> <mask>` 撤销一条宣告。
    /// 对端经下一轮 Update 全表刷新撤销该前缀；不匹配参数拒绝且状态不变。
    pub fn bgp_no_network(c: &mut Ctx) -> Result<String, CliError> {
        let net = c.ipv4("net");
        let len = c.mask_len("mask");
        if net == 0 {
            return Err(CliError::Custom("Invalid network".into()));
        }
        let net = net & sim_packets::mask_from_prefix(len);
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        let Some(bgp) = rs.bgp.as_mut() else {
            return Err(CliError::Custom("BGP process not configured".into()));
        };
        let Some(i) = bgp.networks.iter().position(|x| *x == (net, len)) else {
            return Err(CliError::Custom("BGP network not found".into()));
        };
        bgp.networks.remove(i);
        c.eng.bgp_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn show_bgp_summary(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        Ok(show::bgp_summary(c.eng, c.dev, c.dialect))
    }

    pub fn show_bgp_table(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        Ok(show::bgp_table(c.eng, c.dev, c.dialect))
    }

    // ---------- route-policy（BGP 邻居策略） ----------

    /// IOS `route-map NAME permit|deny SEQ` / VRP `route-policy NAME permit|deny node SEQ`
    pub fn route_policy_enter(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Route policy name is required".into()));
        }
        let action = c.word("action");
        let permit = if action.eq_ignore_ascii_case("permit") {
            true
        } else if action.eq_ignore_ascii_case("deny") {
            false
        } else {
            return Err(CliError::Custom("Action must be permit or deny".into()));
        };
        let seq = c.number("seq").unwrap_or(10);
        if seq == 0 || seq > u16::MAX as u32 {
            return Err(CliError::Custom("Node sequence must be 1-65535".into()));
        }
        let seq = seq as u16;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                let rp = match rs.route_policies.iter_mut().find(|p| p.name == name) {
                    Some(p) => p,
                    None => {
                        rs.route_policies.push(RoutePolicy {
                            name: name.clone(),
                            nodes: vec![],
                        });
                        rs.route_policies.last_mut().unwrap()
                    }
                };
                match rp.nodes.iter_mut().find(|n| n.seq == seq) {
                    Some(n) => n.permit = permit,
                    None => rp.nodes.push(RoutePolicyNode {
                        seq,
                        permit,
                        match_acl: None,
                        set_local_pref: None,
                    }),
                }
            }
        }
        c.sess.sub.rp = Some((name, seq));
        c.sess.mode = Mode::RoutePolicy;
        c.sess.cur_if = None;
        c.eng.bgp_policy_changed(c.dev);
        Ok(String::new())
    }

    fn rp_current_node(c: &Ctx) -> Result<(String, u16), CliError> {
        c.sess
            .sub
            .rp
            .clone()
            .ok_or_else(|| CliError::Custom("no route policy node selected".into()))
    }

    /// IOS `match ip address ACL` / VRP `if-match acl ACL`
    pub fn route_policy_match_acl(c: &mut Ctx) -> Result<String, CliError> {
        let (name, seq) = rp_current_node(c)?;
        let acl = c.number("acl").unwrap_or(0);
        if acl == 0 {
            return Err(CliError::Custom("ACL number is required".into()));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        // 完整预检：引用的 ACL 必须已存在，否则拒绝且状态不变（H5 非法引用失败路径）。
        if !rs.acls.iter().any(|a| a.number == acl) {
            return Err(CliError::Custom("ACL not found".into()));
        }
        if let Some(node) = rs
            .route_policies
            .iter_mut()
            .find(|p| p.name == name)
            .and_then(|p| p.nodes.iter_mut().find(|n| n.seq == seq))
        {
            node.match_acl = Some(acl);
        }
        c.eng.bgp_policy_changed(c.dev);
        Ok(String::new())
    }

    /// IOS `set local-preference N` / VRP `apply local-preference N`
    pub fn route_policy_set_local_pref(c: &mut Ctx) -> Result<String, CliError> {
        let (name, seq) = rp_current_node(c)?;
        let lp = c
            .number("pref")
            .ok_or_else(|| CliError::Custom("Local preference is required".into()))?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(node) = rs
                    .route_policies
                    .iter_mut()
                    .find(|p| p.name == name)
                    .and_then(|p| p.nodes.iter_mut().find(|n| n.seq == seq))
                {
                    node.set_local_pref = Some(lp);
                }
            }
        }
        c.eng.bgp_policy_changed(c.dev);
        Ok(String::new())
    }

    /// IOS `neighbor IP route-map NAME in|out` / VRP `peer IP route-policy NAME import|export`
    pub fn bgp_peer_policy(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let ip = c.ipv4("ip");
        let name = c.word("policy");
        let dir = c.word("dir");
        if ip == 0 || name.is_empty() {
            return Err(CliError::Custom("Invalid peer policy".into()));
        }
        let import = if matches!(dir.to_ascii_lowercase().as_str(), "in" | "import") {
            true
        } else if matches!(dir.to_ascii_lowercase().as_str(), "out" | "export") {
            false
        } else {
            return Err(CliError::Custom("Direction must be in|out".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        // 完整预检：引用的 route-policy 必须已存在，否则拒绝且状态不变（H5 非法引用失败路径）。
        if !rs.route_policies.iter().any(|p| p.name == name) {
            return Err(CliError::Custom("Route policy not found".into()));
        }
        let Some(bgp) = rs.bgp.as_mut() else {
            return Err(CliError::Custom("BGP process not configured".into()));
        };
        if !bgp.neighbors.iter().any(|(n, _)| *n == ip) {
            return Err(CliError::Custom("BGP neighbor not configured".into()));
        }
        bgp.peer_policies
            .retain(|p| !(p.neighbor == ip && p.import == import));
        bgp.peer_policies.push(BgpPeerPolicy {
            neighbor: ip,
            import,
            name,
        });
        c.eng.bgp_policy_changed(c.dev);
        Ok(String::new())
    }

    pub fn show_route_policy(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let name = c.word("name");
        let only = if name.is_empty() {
            None
        } else {
            Some(name.as_str())
        };
        Ok(show::route_policies(c.eng, c.dev, c.dialect, only))
    }

    /// VRP 配置视图：`undo route-policy <name> [node <seq>]`。
    /// 带 node 删除单节点（保留策略名）；不带删除整条策略。
    /// 策略/节点不存在或序号非法 → 拒绝且状态不变，不静默覆盖整条策略。
    pub fn route_policy_no(c: &mut Ctx) -> Result<String, CliError> {
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Route policy name is required".into()));
        }
        let seq = c.number("seq");
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Err(CliError::Custom("device not found".into()));
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        };
        if !rs.route_policies.iter().any(|p| p.name == name) {
            return Err(CliError::Custom("Route policy not found".into()));
        }
        if let Some(s) = seq {
            if s == 0 || s > u16::MAX as u32 {
                return Err(CliError::Custom("Node sequence must be 1-65535".into()));
            }
            let s16 = s as u16;
            let node_exists = rs
                .route_policies
                .iter()
                .find(|p| p.name == name)
                .is_some_and(|p| p.nodes.iter().any(|n| n.seq == s16));
            if !node_exists {
                return Err(CliError::Custom("Route policy node not found".into()));
            }
            if let Some(rp) = rs.route_policies.iter_mut().find(|p| p.name == name) {
                rp.nodes.retain(|n| n.seq != s16);
            }
        } else {
            rs.route_policies.retain(|p| p.name != name);
        }
        c.eng.bgp_policy_changed(c.dev);
        Ok(String::new())
    }

    /// VRP BGP 视图：`undo peer <ip> route-policy <name> import|export` 解除策略绑定。
    /// 绑定不存在（邻居/方向/名称不匹配）→ 拒绝且状态不变；解绑后按协议重算路由。
    pub fn bgp_no_peer_policy(c: &mut Ctx) -> Result<String, CliError> {
        let ip = c.ipv4("ip");
        let name = c.word("policy");
        let dir = c.word("dir");
        if ip == 0 || name.is_empty() {
            return Err(CliError::Custom("Invalid peer policy".into()));
        }
        let import = if matches!(dir.to_ascii_lowercase().as_str(), "in" | "import") {
            true
        } else if matches!(dir.to_ascii_lowercase().as_str(), "out" | "export") {
            false
        } else {
            return Err(CliError::Custom("Direction must be in|out".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        let Some(bgp) = rs.bgp.as_mut() else {
            return Err(CliError::Custom("BGP process not configured".into()));
        };
        if !bgp
            .peer_policies
            .iter()
            .any(|p| p.neighbor == ip && p.import == import && p.name == name)
        {
            return Err(CliError::Custom("Peer route policy not bound".into()));
        }
        bgp.peer_policies
            .retain(|p| !(p.neighbor == ip && p.import == import && p.name == name));
        c.eng.bgp_policy_changed(c.dev);
        Ok(String::new())
    }

    /// VRP route-policy 视图：`undo if-match [acl]` 清除当前节点匹配条件。
    pub fn route_policy_no_match(c: &mut Ctx) -> Result<String, CliError> {
        let (name, seq) = rp_current_node(c)?;
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        let Some(node) = rs
            .route_policies
            .iter_mut()
            .find(|p| p.name == name)
            .and_then(|p| p.nodes.iter_mut().find(|n| n.seq == seq))
        else {
            return Err(CliError::Custom("Route policy node not found".into()));
        };
        node.match_acl = None;
        c.eng.bgp_policy_changed(c.dev);
        Ok(String::new())
    }

    /// VRP route-policy 视图：`undo apply [local-preference]` 清除当前节点 apply 属性。
    pub fn route_policy_no_apply(c: &mut Ctx) -> Result<String, CliError> {
        let (name, seq) = rp_current_node(c)?;
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        let Some(node) = rs
            .route_policies
            .iter_mut()
            .find(|p| p.name == name)
            .and_then(|p| p.nodes.iter_mut().find(|n| n.seq == seq))
        else {
            return Err(CliError::Custom("Route policy node not found".into()));
        };
        node.set_local_pref = None;
        c.eng.bgp_policy_changed(c.dev);
        Ok(String::new())
    }

    // ---------- 防火墙：安全区域与区域间策略 ----------

    fn require_firewall(c: &Ctx) -> Result<(), CliError> {
        // 阶段 6B：设备类型仍是防火墙，或 Profile 明确具备区域化防火墙能力
        // （IOS XE Router 是 ZBF 的官方主线设备）。VRP/Comware 路由器仍被拒绝。
        let kind = c.device_kind();
        if kind == DeviceKind::Firewall {
            return Ok(());
        }
        let profile = profile_mod::profile_for(c.dialect, kind);
        if profile.capability(profile_mod::Capability::ZoneFirewall)
            == CapabilityVerdict::Supported
        {
            Ok(())
        } else {
            Err(CliError::Custom(
                "Security zones are only available on firewalls".into(),
            ))
        }
    }

    fn zone_name(c: &Ctx) -> String {
        let n = c.word("zone");
        if n.is_empty() {
            c.word("name")
        } else {
            n
        }
    }

    /// `firewall zone <name>`：进入（并登记）安全区域视图。
    pub fn security_zone_enter(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = zone_name(c);
        if name.is_empty() {
            return Err(CliError::Custom("Zone name is required".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if !rs.security_zones.iter().any(|z| z.name == name) {
                    rs.security_zones.push(SecurityZone {
                        name: name.clone(),
                        priority: None,
                    });
                }
            }
        }
        c.sess.sub.zone = Some(name);
        c.sess.mode = Mode::Zone;
        c.sess.cur_if = None;
        Ok(String::new())
    }

    /// 安全区域视图：把三层接口加入当前区域（一个接口只属于一个区域）。
    pub fn security_zone_add_if(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let Some(zone) = c.sess.sub.zone.clone() else {
            return Err(CliError::Custom("no zone selected".into()));
        };
        let raw = c.word("if");
        let Some(ifname::IfRef::Physical(port)) =
            ifname::resolve_if(c.dialect, c.device_kind(), &raw)
        else {
            return Err(CliError::Custom("Invalid interface".into()));
        };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if !rs.security_zones.iter().any(|z| z.name == zone) {
                    rs.security_zones.push(SecurityZone {
                        name: zone.clone(),
                        priority: None,
                    });
                }
            }
            if let Some(p) = d.port_mut(port) {
                p.zone = Some(zone);
            }
        }
        Ok(String::new())
    }

    /// 安全区域视图：`undo add interface <if>` —— 把接口移出区域。
    pub fn security_zone_no_add_if(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let Some(zone) = c.sess.sub.zone.clone() else {
            return Err(CliError::Custom("no zone selected".into()));
        };
        let raw = c.word("if");
        let Some(ifname::IfRef::Physical(port)) =
            ifname::resolve_if(c.dialect, c.device_kind(), &raw)
        else {
            return Err(CliError::Custom("Invalid interface".into()));
        };
        let mut removed = false;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let Some(p) = d.port_mut(port) {
                if p.zone.as_deref() == Some(zone.as_str()) {
                    p.zone = None;
                    removed = true;
                }
            }
        }
        if !removed {
            return Err(CliError::Custom(
                "The interface is not in this security zone".into(),
            ));
        }
        Ok(String::new())
    }

    /// IOS 教学抽象：接口视图 `zone-member security <zone>`。
    pub fn security_zone_member(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let zone = c.word("zone");
        if zone.is_empty() {
            return Err(CliError::Custom("Zone name is required".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if !rs.security_zones.iter().any(|z| z.name == zone) {
                    rs.security_zones.push(SecurityZone {
                        name: zone.clone(),
                        priority: None,
                    });
                }
            }
            if let Some(p) = d.port_mut(port) {
                p.zone = Some(zone);
            }
        }
        Ok(String::new())
    }

    // ---------- 阶段 6B：IOS XE 17.x ZBF 处理器 ----------

    /// 当前 zone 是否可作为 zone-pair 端点：已创建的区域、已有成员的区域或 self。
    fn zbf_zone_exists(c: &Ctx, name: &str) -> bool {
        if name == "self" {
            return true;
        }
        let Some(d) = c.eng.net.device(c.dev) else {
            return false;
        };
        let DeviceState::Router(rs) = &d.state else {
            return false;
        };
        rs.security_zones.iter().any(|z| z.name == name)
            || d.ports.iter().any(|p| p.zone.as_deref() == Some(name))
    }

    fn zbf_class_map_enter_with(c: &mut Ctx, match_type: ZbfMatchType) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Class map name is required".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(cm) = rs.zbf_class_maps.iter_mut().find(|m| m.name == name) {
                    if cm.match_type != match_type {
                        return Err(CliError::Custom(
                            "Class map already exists with a different match type".into(),
                        ));
                    }
                } else {
                    rs.zbf_class_maps.push(ZbfClassMap {
                        name: name.clone(),
                        match_type,
                        matches: vec![],
                    });
                }
            }
        }
        c.sess.mode = Mode::ClassMap;
        c.sess.cur_if = None;
        c.sess.sub.cmap = Some(name);
        c.sess.sub.pmap = None;
        c.sess.sub.pmap_class = None;
        c.sess.sub.rule = None;
        Ok(String::new())
    }

    pub fn zbf_class_map_enter_any(c: &mut Ctx) -> Result<String, CliError> {
        zbf_class_map_enter_with(c, ZbfMatchType::Any)
    }

    pub fn zbf_class_map_enter_all(c: &mut Ctx) -> Result<String, CliError> {
        zbf_class_map_enter_with(c, ZbfMatchType::All)
    }

    fn zbf_cmap_name(c: &Ctx) -> Result<String, CliError> {
        c.sess
            .sub
            .cmap
            .clone()
            .ok_or_else(|| CliError::Custom("no class map selected".into()))
    }

    /// `match protocol <name>`：语法与对象照官方；数据面只承载 ICMP/TCP 与常见
    /// TCP 服务端口，L7/ALG 协议显式走模拟器提示，不静默冒充已模拟。
    pub fn zbf_cmap_match_protocol(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let cmap = zbf_cmap_name(c)?;
        let proto = c.word("proto");
        if proto.is_empty() {
            return Err(CliError::Custom("Protocol name is required".into()));
        }
        let l4 = matches!(
            proto.to_ascii_lowercase().as_str(),
            "icmp" | "tcp" | "http" | "ftp" | "smtp" | "esmtp" | "pop3" | "imap" | "dns"
        );
        if !l4 {
            c.notices.push(format!(
                "[netemu] ZBF 模拟器仅实现 L4 匹配（icmp/tcp 与常见 TCP 端口）；{proto} 的 L7/ALG 检查未模拟，不会命中流量"
            ));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(cm) = rs.zbf_class_maps.iter_mut().find(|m| m.name == cmap) {
                    let m = ZbfMatch::Protocol(proto);
                    if !cm.matches.contains(&m) {
                        cm.matches.push(m);
                    }
                }
            }
        }
        Ok(String::new())
    }

    pub fn zbf_cmap_no_match_protocol(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let cmap = zbf_cmap_name(c)?;
        let proto = c.word("proto");
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(cm) = rs.zbf_class_maps.iter_mut().find(|m| m.name == cmap) {
                    cm.matches.retain(|m| m != &ZbfMatch::Protocol(proto.clone()));
                }
            }
        }
        Ok(String::new())
    }

    /// `match access-group <number>`：复用既有的 ACL 逐条首命中判定。
    pub fn zbf_cmap_match_access_group(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let cmap = zbf_cmap_name(c)?;
        let acl = c
            .number("acl")
            .ok_or_else(|| CliError::Custom("Access list number is required".into()))?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(cm) = rs.zbf_class_maps.iter_mut().find(|m| m.name == cmap) {
                    let m = ZbfMatch::AccessGroup(acl);
                    if !cm.matches.contains(&m) {
                        cm.matches.push(m);
                    }
                }
            }
        }
        Ok(String::new())
    }

    pub fn zbf_cmap_no_match_access_group(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let cmap = zbf_cmap_name(c)?;
        let acl = c
            .number("acl")
            .ok_or_else(|| CliError::Custom("Access list number is required".into()))?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(cm) = rs.zbf_class_maps.iter_mut().find(|m| m.name == cmap) {
                    cm.matches.retain(|m| m != &ZbfMatch::AccessGroup(acl));
                }
            }
        }
        Ok(String::new())
    }

    /// `no class-map <name>`：若被 policy-map 引用则拒绝删除（避免悬空引用）。
    pub fn zbf_no_class_map(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Class map name is required".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                let in_use = rs
                    .zbf_policy_maps
                    .iter()
                    .any(|p| p.classes.iter().any(|cl| !cl.is_default && cl.class_name == name));
                if in_use {
                    return Err(CliError::Custom(
                        "Class map is referenced by a policy map".into(),
                    ));
                }
                rs.zbf_class_maps.retain(|m| m.name != name);
            }
        }
        if c.sess.sub.cmap.as_deref() == Some(name.as_str()) {
            c.sess.sub.cmap = None;
            c.sess.mode = Mode::Config;
        }
        Ok(String::new())
    }

    pub fn zbf_policy_map_enter(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Policy map name is required".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if !rs.zbf_policy_maps.iter().any(|p| p.name == name) {
                    rs.zbf_policy_maps.push(ZbfPolicyMap {
                        name: name.clone(),
                        classes: vec![],
                    });
                }
            }
        }
        c.sess.mode = Mode::PolicyMap;
        c.sess.cur_if = None;
        c.sess.sub.pmap = Some(name);
        c.sess.sub.pmap_class = None;
        c.sess.sub.cmap = None;
        Ok(String::new())
    }

    fn zbf_pmap_name(c: &Ctx) -> Result<String, CliError> {
        c.sess
            .sub
            .pmap
            .clone()
            .ok_or_else(|| CliError::Custom("no policy map selected".into()))
    }

    fn zbf_pmap_class_enter_with(
        c: &mut Ctx,
        name: String,
        is_default: bool,
    ) -> Result<String, CliError> {
        require_firewall(c)?;
        let pmap = zbf_pmap_name(c)?;
        if !is_default {
            let exists = match &c.eng.net.device(c.dev) {
                Some(d) => match &d.state {
                    DeviceState::Router(rs) => {
                        rs.zbf_class_maps.iter().any(|m| m.name == name)
                    }
                    _ => false,
                },
                None => false,
            };
            if !exists {
                return Err(CliError::Custom(
                    "Class map does not exist; configure class-map first".into(),
                ));
            }
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(pm) = rs.zbf_policy_maps.iter_mut().find(|p| p.name == pmap) {
                    if !pm.classes.iter().any(|cl| cl.class_name == name) {
                        let entry = ZbfPolicyClass {
                            class_name: name.clone(),
                            is_default,
                            action: None,
                            hits: 0,
                        };
                        if is_default {
                            pm.classes.push(entry);
                        } else {
                            // 官方：class-default 始终位于最后一个。
                            match pm.classes.iter().position(|cl| cl.is_default) {
                                Some(i) => pm.classes.insert(i, entry),
                                None => pm.classes.push(entry),
                            }
                        }
                    }
                }
            }
        }
        c.sess.mode = Mode::PolicyMapClass;
        c.sess.sub.pmap_class = Some(name);
        Ok(String::new())
    }

    pub fn zbf_pmap_class(c: &mut Ctx) -> Result<String, CliError> {
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Class map name is required".into()));
        }
        // 官方示例同时出现 `class class-default` 与
        // `class type inspect class-default`，两者都映射到系统类。
        let is_default = name == "class-default";
        zbf_pmap_class_enter_with(c, name, is_default)
    }

    pub fn zbf_pmap_class_default(c: &mut Ctx) -> Result<String, CliError> {
        zbf_pmap_class_enter_with(c, "class-default".to_string(), true)
    }

    pub fn zbf_pmap_no_class(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let pmap = zbf_pmap_name(c)?;
        let name = c.word("name");
        let is_default = name == "class-default";
        let _ = is_default;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(pm) = rs.zbf_policy_maps.iter_mut().find(|p| p.name == pmap) {
                    pm.classes.retain(|cl| cl.class_name != name);
                }
            }
        }
        if c.sess.sub.pmap_class.as_deref() == Some(name.as_str()) {
            c.sess.sub.pmap_class = None;
            c.sess.mode = Mode::PolicyMap;
        }
        Ok(String::new())
    }

    pub fn zbf_pmap_no_class_default(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let pmap = zbf_pmap_name(c)?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(pm) = rs.zbf_policy_maps.iter_mut().find(|p| p.name == pmap) {
                    pm.classes.retain(|cl| !cl.is_default);
                }
            }
        }
        if c.sess.sub.pmap_class.as_deref() == Some("class-default") {
            c.sess.sub.pmap_class = None;
            c.sess.mode = Mode::PolicyMap;
        }
        Ok(String::new())
    }

    /// `no policy-map <name>`：被 zone-pair 引用时拒绝删除。
    pub fn zbf_no_policy_map(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Policy map name is required".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                let in_use = rs
                    .zbf_zone_pairs
                    .iter()
                    .any(|z| z.service_policy.as_deref() == Some(name.as_str()));
                if in_use {
                    return Err(CliError::Custom(
                        "Policy map is attached to a zone pair".into(),
                    ));
                }
                rs.zbf_policy_maps.retain(|p| p.name != name);
            }
        }
        if c.sess.sub.pmap.as_deref() == Some(name.as_str()) {
            c.sess.sub.pmap = None;
            c.sess.sub.pmap_class = None;
            c.sess.mode = Mode::Config;
        }
        Ok(String::new())
    }

    fn zbf_selected_class_mut(
        c: &mut Ctx,
        f: impl FnOnce(&mut ZbfPolicyClass),
    ) -> Result<(), CliError> {
        let pmap = zbf_pmap_name(c)?;
        let class = c
            .sess
            .sub
            .pmap_class
            .clone()
            .ok_or_else(|| CliError::Custom("no class selected in policy map".into()))?;
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(());
        };
        if let Some(pm) = rs.zbf_policy_maps.iter_mut().find(|p| p.name == pmap) {
            if let Some(cl) = pm.classes.iter_mut().find(|cl| cl.class_name == class) {
                f(cl);
            }
        }
        Ok(())
    }

    pub fn zbf_action_inspect(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let param_map = c.word("param");
        let param_map = (!param_map.is_empty()).then_some(param_map);
        if let Some(p) = &param_map {
            c.notices.push(format!(
                "[netemu] inspect parameter-map {p} 的参数未模拟；使用平台缺省会话参数"
            ));
        }
        zbf_selected_class_mut(c, |cl| {
            cl.action = Some(ZbfAction::Inspect {
                parameter_map: param_map,
            })
        })?;
        Ok(String::new())
    }

    pub fn zbf_action_pass(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        zbf_selected_class_mut(c, |cl| cl.action = Some(ZbfAction::Pass))?;
        Ok(String::new())
    }

    fn zbf_set_drop(c: &mut Ctx, log: bool) -> Result<String, CliError> {
        require_firewall(c)?;
        zbf_selected_class_mut(c, |cl| cl.action = Some(ZbfAction::Drop { log }))?;
        Ok(String::new())
    }

    pub fn zbf_action_drop(c: &mut Ctx) -> Result<String, CliError> {
        zbf_set_drop(c, false)
    }

    pub fn zbf_action_drop_log(c: &mut Ctx) -> Result<String, CliError> {
        zbf_set_drop(c, true)
    }

    pub fn zbf_action_no_inspect(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        zbf_selected_class_mut(c, |cl| {
            if matches!(cl.action, Some(ZbfAction::Inspect { .. })) {
                cl.action = None;
            }
        })?;
        Ok(String::new())
    }

    pub fn zbf_action_no_pass(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        zbf_selected_class_mut(c, |cl| {
            if matches!(cl.action, Some(ZbfAction::Pass)) {
                cl.action = None;
            }
        })?;
        Ok(String::new())
    }

    pub fn zbf_action_no_drop(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        zbf_selected_class_mut(c, |cl| {
            if matches!(cl.action, Some(ZbfAction::Drop { .. })) {
                cl.action = None;
            }
        })?;
        Ok(String::new())
    }

    /// 官方排障示例中的 `no drop log`：只关闭日志意图，保留 drop 动作。
    pub fn zbf_action_no_drop_log(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        zbf_selected_class_mut(c, |cl| {
            if let Some(ZbfAction::Drop { log }) = &mut cl.action {
                *log = false;
            }
        })?;
        Ok(String::new())
    }

    pub fn zbf_zone_pair_enter(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("name");
        let src = c.word("src");
        let dst = c.word("dst");
        if name.is_empty() || src.is_empty() || dst.is_empty() {
            return Err(CliError::Custom(
                "Zone pair name, source and destination are required".into(),
            ));
        }
        if !zbf_zone_exists(c, &src) {
            return Err(CliError::Custom(format!("Security zone {src} does not exist")));
        }
        if !zbf_zone_exists(c, &dst) {
            return Err(CliError::Custom(format!("Security zone {dst} does not exist")));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(other) = rs
                    .zbf_zone_pairs
                    .iter()
                    .find(|z| z.name != name && z.src_zone == src && z.dst_zone == dst)
                {
                    return Err(CliError::Custom(format!(
                        "Zone pair {} already covers {src} to {dst}",
                        other.name
                    )));
                }
                if let Some(z) = rs.zbf_zone_pairs.iter_mut().find(|z| z.name == name) {
                    z.src_zone = src;
                    z.dst_zone = dst;
                } else {
                    rs.zbf_zone_pairs.push(ZbfZonePair {
                        name: name.clone(),
                        src_zone: src,
                        dst_zone: dst,
                        service_policy: None,
                    });
                }
            }
        }
        c.sess.mode = Mode::SecurityRule;
        c.sess.cur_if = None;
        c.sess.sub.rule = Some(name);
        Ok(String::new())
    }

    pub fn zbf_zone_pair_service_policy(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let zp = c
            .sess
            .sub
            .rule
            .clone()
            .ok_or_else(|| CliError::Custom("no zone pair selected".into()))?;
        let policy = c.word("policy");
        if policy.is_empty() {
            return Err(CliError::Custom("Policy map name is required".into()));
        }
        let exists = match &c.eng.net.device(c.dev) {
            Some(d) => match &d.state {
                DeviceState::Router(rs) => rs.zbf_policy_maps.iter().any(|p| p.name == policy),
                _ => false,
            },
            None => false,
        };
        if !exists {
            return Err(CliError::Custom(format!(
                "Policy map {policy} does not exist"
            )));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(z) = rs.zbf_zone_pairs.iter_mut().find(|z| z.name == zp) {
                    z.service_policy = Some(policy);
                }
            }
        }
        Ok(String::new())
    }

    pub fn zbf_zone_pair_no_service_policy(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let zp = c
            .sess
            .sub
            .rule
            .clone()
            .ok_or_else(|| CliError::Custom("no zone pair selected".into()))?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if let Some(z) = rs.zbf_zone_pairs.iter_mut().find(|z| z.name == zp) {
                    z.service_policy = None;
                }
            }
        }
        Ok(String::new())
    }

    pub fn zbf_no_zone_pair(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Zone pair name is required".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                rs.zbf_zone_pairs.retain(|z| z.name != name);
            }
        }
        if c.sess.sub.rule.as_deref() == Some(name.as_str()) {
            c.sess.sub.rule = None;
            c.sess.mode = Mode::Config;
        }
        Ok(String::new())
    }

    /// `no zone security <name>`：删除区域并清除成员关系（唯一状态源 `Port::zone`）。
    pub fn security_zone_delete(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Zone name is required".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if rs
                    .zbf_zone_pairs
                    .iter()
                    .any(|z| z.src_zone == name || z.dst_zone == name)
                {
                    return Err(CliError::Custom(
                        "Security zone is referenced by a zone pair".into(),
                    ));
                }
                rs.security_zones.retain(|z| z.name != name);
            }
            for p in d.ports.iter_mut() {
                if p.zone.as_deref() == Some(name.as_str()) {
                    p.zone = None;
                }
            }
        }
        Ok(String::new())
    }

    pub fn security_zone_member_delete(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let zone = c.word("zone");
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let Some(p) = d.port_mut(port) {
                if p.zone.as_deref() == Some(zone.as_str()) {
                    p.zone = None;
                }
            }
        }
        Ok(String::new())
    }

    pub fn show_zbf_zone_pairs(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        Ok(show::zbf_zone_pairs(c.eng, c.dev))
    }

    pub fn show_zbf_policy_map(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("zp");
        Ok(show::zbf_policy_map(c.eng, c.dev, (!name.is_empty()).then_some(name.as_str()), false))
    }

    pub fn show_zbf_policy_map_session(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("zp");
        Ok(show::zbf_policy_map(c.eng, c.dev, (!name.is_empty()).then_some(name.as_str()), true))
    }

    /// `security-policy` / Comware `security-policy ip`：进入安全策略视图。
    pub fn security_policy_enter(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        c.sess.mode = Mode::SecurityPolicy;
        c.sess.sub.rule = None;
        c.sess.cur_if = None;
        Ok(String::new())
    }

    pub fn security_rule_enter(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Rule name is required".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if !rs.security_policies.iter().any(|p| p.name == name) {
                    rs.security_policies.push(SecurityPolicy {
                        name: name.clone(),
                        src_zone: String::new(),
                        dst_zone: String::new(),
                        permit: false,
                        enabled: true,
                        hits: 0,
                    });
                }
            }
        }
        c.sess.sub.rule = Some(name);
        c.sess.mode = Mode::SecurityRule;
        Ok(String::new())
    }

    /// 安全策略视图：`undo rule name <name>` 删除规则。
    pub fn security_rule_delete(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Rule name is required".into()));
        }
        let mut removed = false;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                let before = rs.security_policies.len();
                rs.security_policies.retain(|p| p.name != name);
                removed = rs.security_policies.len() != before;
            }
        }
        if !removed {
            return Err(CliError::Custom(
                "The security policy rule does not exist".into(),
            ));
        }
        if c.sess.sub.rule.as_deref() == Some(name.as_str()) {
            c.sess.sub.rule = None;
            c.sess.mode = Mode::SecurityPolicy;
        }
        Ok(String::new())
    }

    /// 安全策略视图：`rule move <name1> { before | after } <name2>` 调整规则顺序。
    pub fn security_rule_move(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let from = c.word("from");
        let pos = c.word("pos");
        let to = c.word("to");
        if from.is_empty() || to.is_empty() {
            return Err(CliError::Custom("Rule name is required".into()));
        }
        let before = match pos.as_str() {
            "before" => true,
            "after" => false,
            _ => return Err(CliError::Custom("Position must be before or after".into())),
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        let Some(i) = rs.security_policies.iter().position(|p| p.name == from) else {
            return Err(CliError::Custom(
                "The security policy rule does not exist".into(),
            ));
        };
        let Some(j) = rs.security_policies.iter().position(|p| p.name == to) else {
            return Err(CliError::Custom(
                "The reference security policy rule does not exist".into(),
            ));
        };
        let rule = rs.security_policies.remove(i);
        let j = if i < j { j - 1 } else { j };
        let at = if before { j } else { j + 1 };
        rs.security_policies.insert(at, rule);
        Ok(String::new())
    }

    fn security_rule_mut(c: &mut Ctx, f: impl FnOnce(&mut SecurityPolicy)) -> Result<(), CliError> {
        let Some(name) = c.sess.sub.rule.clone() else {
            return Err(CliError::Custom("no security rule selected".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(());
        };
        if let Some(rule) = rs.security_policies.iter_mut().find(|p| p.name == name) {
            f(rule);
        }
        Ok(())
    }

    pub fn security_rule_source(c: &mut Ctx) -> Result<String, CliError> {
        let zone = c.word("src");
        if zone.is_empty() {
            return Err(CliError::Custom("Source zone is required".into()));
        }
        security_rule_mut(c, |r| r.src_zone = zone)?;
        Ok(String::new())
    }

    pub fn security_rule_destination(c: &mut Ctx) -> Result<String, CliError> {
        let zone = c.word("dst");
        if zone.is_empty() {
            return Err(CliError::Custom("Destination zone is required".into()));
        }
        security_rule_mut(c, |r| r.dst_zone = zone)?;
        Ok(String::new())
    }

    pub fn security_action(c: &mut Ctx) -> Result<String, CliError> {
        let action = c.word("action");
        let permit = if action.eq_ignore_ascii_case("permit") {
            true
        } else if action.eq_ignore_ascii_case("deny") {
            false
        } else {
            return Err(CliError::Custom("Action must be permit or deny".into()));
        };
        security_rule_mut(c, |r| r.permit = permit)?;
        Ok(String::new())
    }

    /// 安全策略规则视图：`undo source-zone` / `undo destination-zone` / `undo action`
    /// 清空对应匹配条件（规则仍在，但不再命中任何流量）。
    pub fn security_rule_no_source(c: &mut Ctx) -> Result<String, CliError> {
        security_rule_mut(c, |r| r.src_zone.clear())?;
        Ok(String::new())
    }

    pub fn security_rule_no_destination(c: &mut Ctx) -> Result<String, CliError> {
        security_rule_mut(c, |r| r.dst_zone.clear())?;
        Ok(String::new())
    }

    pub fn security_rule_no_action(c: &mut Ctx) -> Result<String, CliError> {
        security_rule_mut(c, |r| r.permit = false)?;
        Ok(String::new())
    }

    /// 安全策略规则视图：`disable`（V7，缺省为启用）。
    pub fn security_rule_disable(c: &mut Ctx) -> Result<String, CliError> {
        security_rule_mut(c, |r| r.enabled = false)?;
        Ok(String::new())
    }

    /// 安全策略规则视图：`enable`。
    pub fn security_rule_enable(c: &mut Ctx) -> Result<String, CliError> {
        security_rule_mut(c, |r| r.enabled = true)?;
        Ok(String::new())
    }

    pub fn show_security_policy(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        Ok(show::security_policies(c.eng, c.dev, c.dialect))
    }

    pub fn show_security_zone(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        Ok(show::security_zones(c.eng, c.dev, c.dialect))
    }

    /// `display security-policy rule name <n>`（V9）：单条规则明细。
    pub fn show_security_policy_rule(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("Rule name is required".into()));
        }
        Ok(show::security_policy_rule(c.eng, c.dev, c.dialect, &name))
    }

    /// `display firewall session table`（V3/V9）：会话表与真实状态一致。
    pub fn show_security_session(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        Ok(show::security_sessions(c.eng, c.dev, c.dialect))
    }

    /// `reset firewall session table`（V3）：清除会话表，使策略变更立即生效。
    pub fn reset_security_session(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let n = c.eng.firewall_clear_sessions(c.dev);
        c.notices.push(format!(
            "[netemu] 已清除 {n} 条会话表项（策略变更对新建会话立即生效）"
        ));
        // 模拟器提示走独立通道；设备侧输出保持空（无逐字证据，不伪造厂商回显）。
        Ok(String::new())
    }

    /// 真实 Cisco IOS XE 17.x ZBF 语法入口（class-map / policy-map / zone-pair
    /// source-destination / service-policy / show policy-map type inspect）。
    ///
    /// 这些节点只让课程对照表能按官方语法解析；`specs.rs` 把它们的稳定 ID
    /// 标为 `NotSimulated`，`exec_line_result` 在能力/语义门禁处就会拦截，
    /// 正常路径不会调用此 handler。即使门禁被绕过，也只写模拟器提示通道，
    /// 不伪造厂商成功输出、不修改 sim-core 状态，更不冒充「完整支持」。
    pub fn zbf_recognized_only(c: &mut Ctx) -> Result<String, CliError> {
        c.notices.push(
            "[netemu] 该命令是 Cisco IOS XE 17.x 真实 ZBF 语法，但模拟器尚未实现其检查数据面；命令未执行，设备状态未改变"
                .into(),
        );
        Ok(String::new())
    }

    // ---------- IPsec 站点隧道（教学 IP-in-IP） ----------

    fn ipsec_tunnel_mut(c: &mut Ctx, f: impl FnOnce(&mut IpsecTunnel)) -> Result<(), CliError> {
        let Some(name) = c.sess.sub.ipsec.clone() else {
            return Err(CliError::Custom("no IPsec tunnel selected".into()));
        };
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(());
        };
        if let Some(t) = rs.ipsec_tunnels.iter_mut().find(|t| t.name == name) {
            f(t);
        }
        Ok(())
    }

    /// `ipsec tunnel NAME local A remote B`
    pub fn ipsec_enter(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let name = c.word("name");
        let local = c.ipv4("local");
        let remote = c.ipv4("remote");
        if name.is_empty() || local == 0 || remote == 0 {
            return Err(CliError::Custom("Invalid IPsec tunnel".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if !rs.ipsec_tunnels.iter().any(|t| t.name == name) {
                    rs.ipsec_tunnels.push(IpsecTunnel {
                        name: name.clone(),
                        local,
                        remote,
                        local_subnet: (0, 0),
                        remote_subnet: (0, 0),
                    });
                }
            }
        }
        c.sess.sub.ipsec = Some(name);
        c.sess.mode = Mode::Ipsec;
        c.sess.cur_if = None;
        Ok(String::new())
    }

    pub fn ipsec_local_subnet(c: &mut Ctx) -> Result<String, CliError> {
        let net = c.ipv4("net");
        let len = c.mask_len("len");
        let net = net & sim_packets::mask_from_prefix(len);
        ipsec_tunnel_mut(c, |t| t.local_subnet = (net, len))?;
        Ok(String::new())
    }

    pub fn ipsec_remote_subnet(c: &mut Ctx) -> Result<String, CliError> {
        let net = c.ipv4("net");
        let len = c.mask_len("len");
        let net = net & sim_packets::mask_from_prefix(len);
        ipsec_tunnel_mut(c, |t| t.remote_subnet = (net, len))?;
        Ok(String::new())
    }

    pub fn show_ipsec(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        Ok(show::ipsec_tunnels(c.eng, c.dev, c.dialect))
    }

    // ---------- SSL VPN（远程接入） ----------

    pub fn sslvpn_pool(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        let net = c.ipv4("net");
        let len = c.mask_len("len");
        let net = net & sim_packets::mask_from_prefix(len);
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                rs.sslvpn_pool = Some((net, len));
            }
        }
        Ok(String::new())
    }

    pub fn show_sslvpn(c: &mut Ctx) -> Result<String, CliError> {
        require_firewall(c)?;
        Ok(show::sslvpn_sessions(c.eng, c.dev, c.dialect))
    }

    // ---------- WLAN / CAPWAP ----------

    fn require_ac(c: &Ctx) -> Result<(), CliError> {
        if c.device_kind() == DeviceKind::WlanController {
            Ok(())
        } else {
            Err(CliError::Custom(
                "WLAN configuration is only available on the AC".into(),
            ))
        }
    }

    /// AP：`capwap ac <ip>`
    pub fn capwap_ac(c: &mut Ctx) -> Result<String, CliError> {
        if c.device_kind() != DeviceKind::AccessPoint {
            return Err(CliError::Custom("Only an AP can join an AC".into()));
        }
        let ip = c.ipv4("ip");
        if ip == 0 {
            return Err(CliError::Custom("Invalid AC address".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.wlan.capwap_ac = Some(ip);
            d.wlan.joined = false;
        }
        c.eng.capwap_start(c.dev);
        Ok(String::new())
    }

    pub fn wlan_ssid(c: &mut Ctx) -> Result<String, CliError> {
        require_ac(c)?;
        let ssid = c.word("ssid");
        if ssid.is_empty() {
            return Err(CliError::Custom("SSID is required".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if !d.wlan.profiles.iter().any(|p| p.ssid == ssid) {
                d.wlan.profiles.push(WlanProfile {
                    ssid: ssid.clone(),
                    security: WlanSecurity::Open,
                    psk: None,
                    vlan: 1,
                });
            }
            d.wlan.selected = Some(ssid);
        }
        c.eng.wlan_config_changed(c.dev);
        Ok(String::new())
    }

    fn wlan_update_profile(c: &mut Ctx, f: impl FnOnce(&mut WlanProfile)) -> Result<(), CliError> {
        let Some(ssid) = c
            .eng
            .net
            .device(c.dev)
            .and_then(|d| d.wlan.selected.clone())
        else {
            return Err(CliError::Custom("no SSID selected".into()));
        };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let Some(p) = d.wlan.profile_mut(&ssid) {
                f(p);
            }
        }
        c.eng.wlan_config_changed(c.dev);
        Ok(())
    }

    pub fn wlan_security_open(c: &mut Ctx) -> Result<String, CliError> {
        require_ac(c)?;
        wlan_update_profile(c, |p| {
            p.security = WlanSecurity::Open;
            p.psk = None;
        })?;
        Ok(String::new())
    }

    pub fn wlan_security_wpa2(c: &mut Ctx) -> Result<String, CliError> {
        require_ac(c)?;
        let psk = c.word("psk");
        if psk.is_empty() {
            return Err(CliError::Custom("WPA2 PSK is required".into()));
        }
        wlan_update_profile(c, |p| {
            p.security = WlanSecurity::Wpa2Psk;
            p.psk = Some(psk);
        })?;
        Ok(String::new())
    }

    pub fn wlan_security_wpa3(c: &mut Ctx) -> Result<String, CliError> {
        require_ac(c)?;
        let psk = c.word("psk");
        if psk.is_empty() {
            return Err(CliError::Custom("WPA3 passphrase is required".into()));
        }
        wlan_update_profile(c, |p| {
            p.security = WlanSecurity::Wpa3Sae;
            p.psk = Some(psk);
        })?;
        Ok(String::new())
    }

    pub fn wlan_vlan(c: &mut Ctx) -> Result<String, CliError> {
        require_ac(c)?;
        let vlan = c.number("vlan").unwrap_or(0);
        if !(1..=4094).contains(&vlan) {
            return Err(CliError::Custom("Invalid VLAN".into()));
        }
        wlan_update_profile(c, |p| p.vlan = vlan as u16)?;
        Ok(String::new())
    }

    /// AP：`wlan priority <n>`（漫游选路）
    pub fn wlan_priority(c: &mut Ctx) -> Result<String, CliError> {
        if c.device_kind() != DeviceKind::AccessPoint {
            return Err(CliError::Custom(
                "WLAN priority is only available on APs".into(),
            ));
        }
        let priority = c.number("priority").unwrap_or(100).min(255) as u8;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            d.wlan.priority = priority;
        }
        Ok(String::new())
    }

    pub fn show_wlan(c: &mut Ctx) -> Result<String, CliError> {
        Ok(show::wlan_table(c.eng, c.dev, c.dialect))
    }

    // ---------- VXLAN（EVPN 数据面简化） ----------

    pub fn vxlan_local(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let ip = c.ipv4("ip");
        if ip == 0 {
            return Err(CliError::Custom("Invalid VTEP address".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                sw.vxlan.local = Some(ip);
            }
        }
        c.eng.evpn_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn vxlan_peer(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let ip = c.ipv4("ip");
        if ip == 0 {
            return Err(CliError::Custom("Invalid VTEP peer".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                if !sw.vxlan.peers.contains(&ip) {
                    sw.vxlan.peers.push(ip);
                }
            }
        }
        Ok(String::new())
    }

    pub fn vxlan_vni(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let vni = c.number("vni").unwrap_or(0);
        let vlan = c.number("vlan").unwrap_or(0);
        if vni == 0 || !(1..=4094).contains(&vlan) {
            return Err(CliError::Custom("Invalid VNI mapping".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                sw.ensure_vlan(vlan as u16);
                sw.vxlan.mappings.retain(|(_, v)| *v != vlan as u16);
                sw.vxlan.mappings.push((vni, vlan as u16));
            }
        }
        c.eng.evpn_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn show_vxlan(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(show::vxlan_table(c.eng, c.dev))
    }

    // ---------- EVPN 控制面（简化 Type-3） ----------

    pub fn evpn_enable(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                sw.evpn.enabled = true;
            }
        }
        c.eng.evpn_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn evpn_peer(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let ip = c.ipv4("ip");
        if ip == 0 {
            return Err(CliError::Custom("Invalid EVPN peer".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                if !sw.evpn.peers.contains(&ip) {
                    sw.evpn.peers.push(ip);
                }
            }
        }
        c.eng.evpn_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn show_evpn(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(show::evpn_table(c.eng, c.dev))
    }

    // ---------- F5 LTM（虚拟服务器 / 池） ----------

    fn require_lb(c: &Ctx) -> Result<(), CliError> {
        if c.device_kind() == DeviceKind::LoadBalancer {
            Ok(())
        } else {
            Err(CliError::Custom(
                "LTM configuration is only available on a load balancer".into(),
            ))
        }
    }

    pub fn ltm_pool_member(c: &mut Ctx) -> Result<String, CliError> {
        require_lb(c)?;
        let pool = c.word("pool");
        let ip = c.ipv4("ip");
        let port = c.number("port").unwrap_or(0);
        if pool.is_empty() || ip == 0 || port == 0 || port > u16::MAX as u32 {
            return Err(CliError::Custom("Invalid pool member".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            let p = match d.ltm.pools.iter_mut().find(|p| p.name == pool) {
                Some(p) => p,
                None => {
                    d.ltm.pools.push(LtmPool {
                        name: pool.clone(),
                        members: vec![],
                    });
                    d.ltm.pools.last_mut().unwrap()
                }
            };
            let m = LtmMember {
                ip,
                port: port as u16,
            };
            if !p.members.contains(&m) {
                p.members.push(m);
            }
        }
        Ok(String::new())
    }

    pub fn ltm_virtual(c: &mut Ctx) -> Result<String, CliError> {
        require_lb(c)?;
        let name = c.word("name");
        let vip = c.ipv4("vip");
        let port = c.number("port").unwrap_or(0);
        let pool = c.word("pool");
        if name.is_empty() || vip == 0 || port == 0 || port > u16::MAX as u32 || pool.is_empty() {
            return Err(CliError::Custom("Invalid virtual server".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if !d.ltm.pools.iter().any(|p| p.name == pool) {
                return Err(CliError::Custom("Pool not found".into()));
            }
            let v = LtmVirtual {
                name,
                vip,
                port: port as u16,
                pool,
            };
            if let Some(x) = d.ltm.virtuals.iter_mut().find(|x| x.name == v.name) {
                *x = v;
            } else {
                d.ltm.virtuals.push(v);
            }
        }
        Ok(String::new())
    }

    pub fn show_ltm(c: &mut Ctx) -> Result<String, CliError> {
        require_lb(c)?;
        Ok(show::ltm_table(c.eng, c.dev))
    }

    // ---------- 堆叠 / IRF / M-LAG ----------

    pub fn stack_domain(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let domain = c.number("domain").unwrap_or(0);
        if domain == 0 || domain > u16::MAX as u32 {
            return Err(CliError::Custom("Invalid stack domain".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                let s = sw.stack.get_or_insert(StackConfig {
                    domain: domain as u16,
                    member: 1,
                    priority: 100,
                });
                s.domain = domain as u16;
            }
        }
        Ok(String::new())
    }

    pub fn stack_member(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let member = c.number("member").unwrap_or(0);
        if member == 0 || member > 255 {
            return Err(CliError::Custom("Invalid stack member ID".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                if let Some(s) = sw.stack.as_mut() {
                    s.member = member as u8;
                } else {
                    return Err(CliError::Custom("Configure stack domain first".into()));
                }
            }
        }
        Ok(String::new())
    }

    pub fn stack_priority(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let priority = c.number("priority").unwrap_or(100).min(u16::MAX as u32) as u16;
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                if let Some(s) = sw.stack.as_mut() {
                    s.priority = priority;
                } else {
                    return Err(CliError::Custom("Configure stack domain first".into()));
                }
            }
        }
        Ok(String::new())
    }

    pub fn mlag_domain(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let domain = c.number("domain").unwrap_or(0);
        if domain == 0 || domain > u16::MAX as u32 {
            return Err(CliError::Custom("Invalid M-LAG domain".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                let m = sw.mlag.get_or_insert(MlagConfig {
                    domain: domain as u16,
                    peer_links: vec![],
                });
                m.domain = domain as u16;
            }
        }
        Ok(String::new())
    }

    pub fn mlag_peer_link(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Switch(sw) = &mut d.state {
                if let Some(m) = sw.mlag.as_mut() {
                    if !m.peer_links.contains(&port) {
                        m.peer_links.push(port);
                    }
                } else {
                    return Err(CliError::Custom("Configure mlag domain first".into()));
                }
            }
        }
        Ok(String::new())
    }

    pub fn show_stack(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(show::stack_table(c.eng, c.dev))
    }

    pub fn show_mlag(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(show::mlag_table(c.eng, c.dev))
    }

    // ---------- IS-IS（简化） ----------

    pub fn isis_enter(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        c.sess.mode = Mode::Isis;
        c.sess.cur_if = None;
        Ok(String::new())
    }

    pub fn isis_net(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let net = c.word("net");
        let Some(sid) = sim_core::device::parse_isis_net(&net) else {
            return Err(CliError::Custom(
                "Invalid NET, e.g. 49.0001.0000.0000.0001.00".into(),
            ));
        };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                rs.isis = Some(sim_core::device::IsisConfig {
                    net: net.clone(),
                    system_id: sid,
                });
            }
        }
        c.eng.isis_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn isis_enable_if(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let Some(p) = d.port_mut(port) {
                p.isis_enable = true;
            }
        }
        c.eng.isis_config_changed(c.dev);
        Ok(String::new())
    }

    pub fn show_isis_neighbors(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        Ok(show::isis_neighbors(c.eng, c.dev, c.dialect))
    }

    pub fn show_isis_db(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        Ok(show::isis_lsdb(c.eng, c.dev, c.dialect))
    }

    // ---------- VRF（简化：连接 + 静态路由） ----------

    pub fn vrf_create(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let name = c.word("name");
        if name.is_empty() {
            return Err(CliError::Custom("VRF name required".into()));
        }
        if let Some(d) = c.eng.net.device_mut(c.dev) {
            if let DeviceState::Router(rs) = &mut d.state {
                if !rs.vrfs.iter().any(|v| v.name == name) {
                    let id = rs.vrfs.iter().map(|v| v.id).max().unwrap_or(0) + 1;
                    rs.vrfs.push(sim_core::device::Vrf {
                        id,
                        name,
                        static_routes: vec![],
                    });
                }
            }
        }
        Ok(String::new())
    }

    pub fn vrf_bind(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let Some(cli_engine::IfCtx::Physical(port)) = c.sess.cur_if else {
            return Err(CliError::Custom("no interface selected".into()));
        };
        let name = c.word("name");
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let id = match &d.state {
            DeviceState::Router(rs) => rs.vrfs.iter().find(|v| v.name == name).map(|v| v.id),
            _ => None,
        };
        let Some(id) = id else {
            return Err(CliError::Custom("VRF not found".into()));
        };
        if let Some(p) = d.port_mut(port) {
            p.vrf = Some(id);
        }
        Ok(String::new())
    }

    pub fn vrf_static(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let name = c.word("name");
        let dst = c.ipv4("dst");
        let len = c.mask_len("mask");
        let nh = c.ipv4("nexthop");
        if nh == 0 {
            return Err(CliError::Custom("Invalid route".into()));
        }
        let net = dst & sim_packets::mask_from_prefix(len);
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        let Some(v) = rs.vrfs.iter_mut().find(|v| v.name == name) else {
            return Err(CliError::Custom("VRF not found".into()));
        };
        if !v
            .static_routes
            .iter()
            .any(|r| r.dst == net && r.len == len && r.nexthop == nh)
        {
            v.static_routes.push(sim_core::device::Route {
                dst: net,
                len,
                nexthop: nh,
                vlan: None,
                port: 0,
                src: sim_core::device::RouteSrc::Static,
            });
        }
        Ok(String::new())
    }

    pub fn show_route_vrf(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        let name = c.word("name");
        let id = c
            .eng
            .net
            .device(c.dev)
            .and_then(|d| d.vrf_id(&name))
            .ok_or_else(|| CliError::Custom("VRF not found".into()))?;
        Ok(show::route_table_vrf(c.eng, c.dev, c.dialect, id))
    }

    // ---------- MPLS（静态 LSP） ----------

    fn mpls_lsp_upsert(c: &mut Ctx, lsp: sim_core::device::StaticLsp) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        if lsp.vpn_label.is_some() && lsp.vrf.is_none() {
            return Err(CliError::Custom("vpn-label requires a vrf".into()));
        }
        let Some(d) = c.eng.net.device_mut(c.dev) else {
            return Ok(String::new());
        };
        if let Some(v) = &lsp.vrf {
            if d.vrf_id(v).is_none() {
                return Err(CliError::Custom("VRF not found".into()));
            }
        }
        let DeviceState::Router(rs) = &mut d.state else {
            return Ok(String::new());
        };
        if let Some(x) = rs.mpls_lsps.iter_mut().find(|x| x.name == lsp.name) {
            *x = lsp;
        } else {
            rs.mpls_lsps.push(lsp);
        }
        Ok(String::new())
    }

    pub fn mpls_lsp_ingress(c: &mut Ctx) -> Result<String, CliError> {
        let name = c.word("name");
        let net = c.ipv4("net") & sim_packets::mask_from_prefix(c.mask_len("len"));
        let len = c.mask_len("len");
        let nh = c.ipv4("nexthop");
        let out = c.number("out");
        let vrf = c.word("vrf");
        if name.is_empty() || net == 0 || nh == 0 || out.is_none() {
            return Err(CliError::Custom("Invalid static LSP".into()));
        }
        mpls_lsp_upsert(
            c,
            sim_core::device::StaticLsp {
                name,
                net: Some((net, len)),
                in_label: None,
                out_label: out,
                nexthop: Some(nh),
                vrf: if vrf.is_empty() { None } else { Some(vrf) },
                vpn_label: c.number("vpn"),
            },
        )
    }

    pub fn mpls_lsp_transit(c: &mut Ctx) -> Result<String, CliError> {
        let name = c.word("name");
        let inside = c.number("in");
        let nh = c.ipv4("nexthop");
        let out = c.number("out");
        if name.is_empty() || inside.is_none() || nh == 0 || out.is_none() {
            return Err(CliError::Custom("Invalid static LSP".into()));
        }
        mpls_lsp_upsert(
            c,
            sim_core::device::StaticLsp {
                name,
                net: None,
                in_label: inside,
                out_label: out,
                nexthop: Some(nh),
                vrf: None,
                vpn_label: None,
            },
        )
    }

    pub fn mpls_lsp_egress(c: &mut Ctx) -> Result<String, CliError> {
        let name = c.word("name");
        let inside = c.number("in");
        let vrf = c.word("vrf");
        if name.is_empty() || inside.is_none() {
            return Err(CliError::Custom("Invalid static LSP".into()));
        }
        mpls_lsp_upsert(
            c,
            sim_core::device::StaticLsp {
                name,
                net: None,
                in_label: inside,
                out_label: None,
                nexthop: None,
                vrf: if vrf.is_empty() { None } else { Some(vrf) },
                vpn_label: None,
            },
        )
    }

    pub fn show_mpls(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(
                "Command is only available on routers".into(),
            ));
        }
        Ok(show::mpls_table(c.eng, c.dev, c.dialect))
    }

    // ---------- 显示 ----------

    pub fn show_acl(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(match c.dialect {
                Dialect::Ios => "Invalid input detected".into(),
                _ => "Command is only available on routers".into(),
            }));
        }
        Ok(show::acl_table(c.eng, c.dev, c.dialect))
    }

    pub fn show_nat(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router
                | DeviceKind::Firewall
                | DeviceKind::WlanController
                | DeviceKind::LoadBalancer
        ) {
            return Err(CliError::Custom(match c.dialect {
                Dialect::Ios => "Invalid input detected".into(),
                _ => "Command is only available on routers".into(),
            }));
        }
        Ok(show::nat_table(c.eng, c.dev, c.dialect))
    }

    pub fn show_brief(c: &mut Ctx) -> Result<String, CliError> {
        Ok(show::int_brief(c.eng, c.dev, c.dialect))
    }

    /// `show running-config` / `display current-configuration`：从 DevSummary 实时渲染（ADR D12）。
    pub fn show_running_config(c: &mut Ctx) -> Result<String, CliError> {
        if c.device_kind() == DeviceKind::Host {
            return Err(CliError::Custom(match c.dialect {
                Dialect::Ios => "% Invalid input detected".into(),
                _ => "Command is not available on hosts".into(),
            }));
        }
        let summary = c.eng.net.summary();
        let Some(d) = summary.devices.into_iter().find(|x| x.id == c.dev) else {
            return Ok(String::new());
        };
        Ok(config_render::render(&d, c.dialect))
    }

    pub fn show_route(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router | DeviceKind::Switch | DeviceKind::Firewall
        ) {
            return Err(CliError::Custom(match c.dialect {
                Dialect::Ios => "Invalid input detected".into(),
                _ => "Command is only available on routers".into(),
            }));
        }
        Ok(show::route_table(c.eng, c.dev, c.dialect))
    }

    pub fn show_stp(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(show::stp_table(c.eng, c.dev, c.dialect))
    }

    pub fn show_vlan(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(show::vlan_table(c.eng, c.dev, c.dialect))
    }

    pub fn show_mac(c: &mut Ctx) -> Result<String, CliError> {
        require_switch(c)?;
        Ok(show::mac_table(c.eng, c.dev, c.dialect))
    }

    pub fn static_route(c: &mut Ctx) -> Result<String, CliError> {
        if !matches!(
            c.device_kind(),
            DeviceKind::Router | DeviceKind::Switch | DeviceKind::Firewall
        ) {
            return Err(CliError::Custom(match c.dialect {
                Dialect::Ios => "Invalid input detected".into(),
                _ => "Command is only available on routers".into(),
            }));
        }
        let dst = c.ipv4("dst");
        let len = c.mask_len("mask");
        let nh = c.ipv4("nexthop");
        if nh == 0 {
            return Err(CliError::Custom("Invalid route parameters".into()));
        }
        // 完整预检后才提交，不在写状态之后再失败。
        let net_dst = dst & mask_from_prefix(len);
        c.eng.net.add_static_route(c.dev, net_dst, len, nh);
        Ok(String::new())
    }

    /// `no ip route` / `undo ip route-static`：与写入共用同一匹配键。
    pub fn no_static_route(c: &mut Ctx) -> Result<String, CliError> {
        let dst = c.ipv4("dst");
        let len = c.mask_len("mask");
        let nh = c.ipv4("nexthop");
        if nh == 0 {
            return Err(CliError::Custom("Invalid route parameters".into()));
        }
        let net_dst = dst & mask_from_prefix(len);
        c.eng.net.remove_static_route(c.dev, net_dst, len, nh);
        Ok(String::new())
    }

    pub fn ping(c: &mut Ctx) -> Result<String, CliError> {
        let target = c.ipv4("target");
        if target == 0 {
            return Err(CliError::Custom("Invalid IP address".into()));
        }
        c.eng.start_ping(c.dev, target);
        Ok(String::new())
    }

    /// `save` 的持久化语义由规格门禁拦截（NotSimulated），这里不再把模拟器措辞
    /// 混进设备输出；保留空设备输出作为迁移期兜底。
    pub fn save_msg(_c: &mut Ctx) -> Result<String, CliError> {
        Ok(String::new())
    }
}

// ---------- 执行入口 ----------

/// 设备状态指纹：配置态、协议瞬态、虚拟时间与引擎计数。
/// 用于 `state_changed` 判定和纯读检查的状态不变测试；不参与任何设备输出。
pub fn state_fingerprint(eng: &Engine) -> String {
    format!(
        "{:?}|{}|{}|{}|{}",
        eng.net.devices,
        eng.now_ns(),
        eng.events_processed,
        eng.in_flight(),
        eng.dropped,
    )
}

/// 扫描设备输出中的模拟器措辞。模拟器限制/验证状态只能走 `simulator_notices`。
pub fn scan_simulator_wording(text: &str) -> Vec<&'static str> {
    let lower = text.to_lowercase();
    const MARKERS: &[&str] = &["simulator", "netemu", "模拟器", "仿真器"];
    MARKERS
        .iter()
        .copied()
        .filter(|m| lower.contains(&m.to_lowercase()))
        .collect()
}

fn invalid_inspection(detail: &str) -> Inspection {
    Inspection {
        passed: false,
        detail: detail.into(),
        status: CommandStatus::InvalidCommand,
        support: cli_engine::spec::SupportLabel::InvalidCommand,
        evidence: EvidenceLevel::Unverified.label(),
        spec_id: String::new(),
    }
}

/// 结构化执行结果：能力门禁先于模拟语义门禁，所有非 `Executed` 路径都不调用 handler。
pub fn exec_line_result(
    eng: &mut Engine,
    sess: &mut Session,
    dev: DevId,
    line: &str,
) -> CommandResult {
    // 登录流程优先：后续输入是用户名/口令
    if sess.login.is_some() {
        let before = state_fingerprint(eng);
        let out = login_step(eng, sess, dev, line);
        return CommandResult::executed(out, state_fingerprint(eng) != before);
    }
    // 远程会话：命令转发给目标设备，exit/logout 断开
    if sess.remote.is_some() {
        let before = state_fingerprint(eng);
        let out = remote_step(eng, sess, dev, line);
        return CommandResult::executed(out, state_fingerprint(eng) != before);
    }
    let Some(d) = eng.net.device(dev) else {
        return CommandResult::executed(String::new(), false);
    };
    let kind = d.kind;
    let dialect = d.dialect;
    // 本地 enable 口令门禁：已配置 enable secret 时，本行是特权口令而非命令。
    if sess.enable_prompt {
        let secret = d.mgmt.enable_secret.clone();
        return enable_password_step(sess, secret.as_deref(), line);
    }
    if kind == DeviceKind::Host {
        let before = state_fingerprint(eng);
        let out = pc::exec(eng, sess, dev, line);
        return CommandResult::executed(out, state_fingerprint(eng) != before);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return CommandResult::executed(String::new(), false);
    }
    if trimmed.contains('?') {
        return CommandResult::executed(render_help(dialect, sess, line), false);
    }
    let t = trees(dialect);
    let root = t.root_for(sess.mode);
    let parsed = match cli_engine::parse(root, line) {
        Ok(Some(p)) => p,
        Ok(None) => return CommandResult::executed(String::new(), false),
        Err(e) => {
            return CommandResult::rejected(
                CommandStatus::InvalidCommand,
                format_error(dialect, line, e),
                vec![],
            )
        }
    };

    let id = cli_engine::spec::command_id(sess.mode.tag(), &parsed.path);
    let before = state_fingerprint(eng);

    let Some(spec) = specs::find(dialect, &id) else {
        // 迁移清单中的节点：保持阶段 0 行为，lint 会显式列出未关联项，不设隐藏豁免。
        // handler 产生的模拟器限制仍必须走独立提示通道，不能被丢弃。
        let mut ctx = Ctx {
            eng,
            dev,
            sess,
            dialect,
            vals: parsed.vals,
            notices: vec![],
            not_simulated: false,
        };
        let handler_result = (parsed.run)(&mut ctx);
        let notices = std::mem::take(&mut ctx.notices);
        let changed = state_fingerprint(ctx.eng) != before;
        return match handler_result {
            Ok(_out) if ctx.not_simulated => CommandResult::rejected(
                CommandStatus::NotSimulated,
                String::new(),
                not_simulated_notices(notices),
            ),
            Ok(out) => {
                let mut result = CommandResult::executed(out, changed);
                result.simulator_notices = notices;
                result
            }
            Err(e) => CommandResult::rejected(
                CommandStatus::InvalidCommand,
                format_error(dialect, line, e),
                notices,
            ),
        };
    };

    let profile = profile_mod::profile_for(dialect, kind);
    let capability = spec
        .capability
        .map(|c| profile.capability(c))
        .unwrap_or(CapabilityVerdict::Supported);
    // 与被动命令检查共用同一个纯判断函数；这里只消费结论，不产生副作用。
    let verdict = judge(JudgeInput {
        spec_id: &spec.id,
        parse_ok: true,
        mode_ok: true,
        capability,
        semantics: spec.semantics,
        evidence: spec.evidence,
    });

    match verdict.status {
        // 真机通常把不支持的功能表现为解析器错误；复用既有方言错误结构，
        // 不发明一套真机上不存在的厂商文本，也不额外生成模拟器报错。
        CommandStatus::UnsupportedByDevice => CommandResult::rejected(
            CommandStatus::UnsupportedByDevice,
            format_error(dialect, line, CliError::Unknown { pos: line.len() }),
            vec![],
        ),
        CommandStatus::NotSimulated => {
            let mut notices =
                vec!["[netemu] 该命令的承诺语义尚未模拟；命令未执行，设备状态未改变".to_string()];
            if !spec.evidence.can_decide_capability() {
                notices.push("[netemu] 平台行为证据不足，待验证；不宣称真实设备有效".into());
            }
            CommandResult::rejected(CommandStatus::NotSimulated, String::new(), notices)
        }
        CommandStatus::InvalidCommand => {
            unreachable!("解析成功且模式正确的路径不会是 InvalidCommand")
        }
        CommandStatus::Executed => {
            let mut ctx = Ctx {
                eng,
                dev,
                sess,
                dialect,
                vals: parsed.vals,
                notices: vec![],
                not_simulated: false,
            };
            let handler_result = (parsed.run)(&mut ctx);
            let mut notices = std::mem::take(&mut ctx.notices);
            let changed = state_fingerprint(ctx.eng) != before;
            match handler_result {
                Ok(_out) if ctx.not_simulated => CommandResult::rejected(
                    CommandStatus::NotSimulated,
                    String::new(),
                    not_simulated_notices(notices),
                ),
                Ok(out) => {
                    if !spec.evidence.can_decide_capability() {
                        notices.push(
                            "[netemu] 平台能力或输出证据不足：行为待验证，不得标为完整支持".into(),
                        );
                    }
                    let mut result = CommandResult::executed(out, changed);
                    result.simulator_notices = notices;
                    result
                }
                Err(e) => CommandResult::rejected(
                    CommandStatus::InvalidCommand,
                    format_error(dialect, line, e),
                    notices,
                ),
            }
        }
    }
}

/// 迁移期文本契约：只渲染设备文本，模拟器提示绝不混入 `device_output`。
pub fn exec_line(eng: &mut Engine, sess: &mut Session, dev: DevId, line: &str) -> String {
    exec_line_result(eng, sess, dev, line).device_output
}

/// 登录失败提示（按源设备的方言/主机风格渲染）。
pub fn login_error_text(
    kind: DeviceKind,
    dialect: Dialect,
    e: sim_core::engine::LoginError,
) -> String {
    use sim_core::engine::LoginError;
    if kind == DeviceKind::Host {
        return match e {
            LoginError::Unreachable => "connect: Network is unreachable".into(),
            LoginError::TransportDenied => "connect: Connection refused".into(),
            LoginError::AuthFailed => "Permission denied".into(),
        };
    }
    match dialect {
        Dialect::Ios => match e {
            LoginError::Unreachable => "% Destination unreachable".into(),
            LoginError::TransportDenied => "% Connection refused by remote host".into(),
            LoginError::AuthFailed => "% Authentication failed".into(),
        },
        Dialect::Vrp => match e {
            LoginError::Unreachable => "Error: Destination host unreachable.".into(),
            LoginError::TransportDenied => "Error: The connection was refused.".into(),
            LoginError::AuthFailed => "Error: Authentication failed.".into(),
        },
        Dialect::Comware => match e {
            LoginError::Unreachable => "% Destination unreachable.".into(),
            LoginError::TransportDenied => "% Connection refused.".into(),
            LoginError::AuthFailed => "% Authentication failed.".into(),
        },
        Dialect::Tmos => match e {
            LoginError::Unreachable => "Network is unreachable".into(),
            LoginError::TransportDenied => "Connection refused".into(),
            LoginError::AuthFailed => "Authentication failed".into(),
        },
    }
}

/// 本地 enable 口令校验：正确进入特权模式，错误拒绝并留在用户模式。
/// 口令交互只改会话状态，不改设备状态（state_fingerprint 只追踪设备）。
fn enable_password_step(sess: &mut Session, secret: Option<&str>, line: &str) -> CommandResult {
    sess.enable_prompt = false;
    match secret {
        Some(s) if line.trim() == s => {
            sess.mode = Mode::Priv;
            sess.sub = Default::default();
            CommandResult::executed(String::new(), false)
        }
        _ => CommandResult::executed("% Access denied".into(), false),
    }
}

/// 登录流程：用户名 → 口令 → 建立远程会话。
fn login_step(eng: &mut Engine, sess: &mut Session, dev: DevId, line: &str) -> String {
    let (kind, dialect) = eng
        .net
        .device(dev)
        .map(|d| (d.kind, d.dialect))
        .unwrap_or((DeviceKind::Router, Dialect::Ios));
    let Some(step) = sess.login.as_ref().map(|f| f.step) else {
        return String::new();
    };
    match step {
        LoginStep::Username => {
            let user = line.trim().to_string();
            if user.is_empty() {
                return String::new();
            }
            if let Some(flow) = sess.login.as_mut() {
                flow.user = user;
                flow.step = LoginStep::Password;
            }
            String::new()
        }
        LoginStep::Password => {
            let Some(flow) = sess.login.take() else {
                return String::new();
            };
            match eng.try_login(dev, flow.target_ip, flow.proto, &flow.user, line.trim()) {
                Ok((target, privilege)) => {
                    let target_dialect = eng
                        .net
                        .device(target)
                        .map(|d| d.dialect)
                        .unwrap_or(Dialect::Ios);
                    // 权限 15 直接进特权模式（与 SSH 登录真实行为一致）
                    let mode = if target_dialect == Dialect::Ios && privilege >= 15 {
                        Mode::Priv
                    } else {
                        Mode::Exec
                    };
                    sess.remote = Some(RemoteSession {
                        dev: target,
                        inner: Box::new(Session {
                            mode,
                            ..Default::default()
                        }),
                    });
                    String::new()
                }
                Err(e) => login_error_text(kind, dialect, e),
            }
        }
    }
}

/// 远程会话：转发命令；`exit` / `logout` 断开。
fn remote_step(eng: &mut Engine, sess: &mut Session, dev: DevId, line: &str) -> String {
    let dialect = eng
        .net
        .device(dev)
        .map(|d| d.dialect)
        .unwrap_or(Dialect::Ios);
    let cmd = line.trim().to_lowercase();
    if cmd == "exit" || cmd == "logout" {
        sess.remote = None;
        return match dialect {
            Dialect::Ios => "\n[Connection closed by foreign host]".into(),
            _ => "\nInfo: The connection was closed.".into(),
        };
    }
    let Some(remote) = sess.remote.as_mut() else {
        return String::new();
    };
    let target = remote.dev;
    exec_line(eng, &mut remote.inner, target, line)
}

/// 执行一行命令并驱动引擎至静默，返回结构化结果（设备输出含 ping 结果）。
pub fn exec_and_run_result(
    eng: &mut Engine,
    sess: &mut Session,
    dev: DevId,
    line: &str,
) -> CommandResult {
    let eff = sess.effective_dev(dev);
    let kind = eng.net.device(eff).map(|d| d.kind);
    let mut result = exec_line_result(eng, sess, dev, line);
    let horizon = eng.drain_horizon();
    eng.run_to_quiescence(horizon);
    let mut parts: Vec<String> = vec![];
    if !result.device_output.trim().is_empty() {
        parts.push(result.device_output.clone());
    }
    let dialect = eng
        .net
        .device(eff)
        .map(|d| d.dialect)
        .unwrap_or(Dialect::Ios);
    for s in eng.take_unreported_pings(eff) {
        let text = if kind == Some(DeviceKind::Host) {
            show::render_ping_generic(&s)
        } else {
            show::render_ping(dialect, &s)
        };
        parts.push(text);
    }
    for s in eng.take_unreported_ping6(eff) {
        let text = if kind == Some(DeviceKind::Host) {
            show::render_ping6_generic(&s)
        } else {
            show::render_ping6(dialect, &s)
        };
        parts.push(text);
    }
    for p in eng.take_unreported_tcp(eff) {
        parts.push(show::render_tcp_probe(&p));
    }
    result.device_output = parts.join("\n");
    result
}

/// 迁移期文本契约：只返回设备文本，与阶段 0 的 `String` 返回值一致。
pub fn exec_and_run(eng: &mut Engine, sess: &mut Session, dev: DevId, line: &str) -> String {
    exec_and_run_result(eng, sess, dev, line).device_output
}

/// 命令检查的公共纯判断部分：解析 → 稳定 ID → 规格 → 能力/语义/证据。
/// 不驱动引擎、不推进虚拟时间、不调用 `run_checks`、不发探测包。
fn inspect_parsed(
    dialect: Dialect,
    kind: DeviceKind,
    mode: Mode,
    line: &str,
    platform_level: bool,
) -> Inspection {
    let t = trees(dialect);
    let parsed = match cli_engine::parse(t.root_for(mode), line) {
        Ok(Some(p)) => p,
        Ok(None) => return invalid_inspection("空行或命令不完整"),
        Err(_) => return invalid_inspection("语法、参数或当前模式不正确"),
    };
    let id = cli_engine::spec::command_id(mode.tag(), &parsed.path);
    let Some(spec) = specs::find(dialect, &id) else {
        return Inspection {
            passed: false,
            detail: "命令尚未关联规格元数据（迁移清单），不得标为完整支持".into(),
            status: CommandStatus::Executed,
            support: cli_engine::spec::SupportLabel::PendingVerification,
            evidence: EvidenceLevel::Unverified.label(),
            spec_id: id,
        };
    };
    let profile = profile_mod::profile_for(dialect, kind);
    let capability = spec
        .capability
        .map(|c| profile.capability(c))
        .unwrap_or(CapabilityVerdict::Supported);
    let mut result = judge(JudgeInput {
        spec_id: &spec.id,
        parse_ok: true,
        mode_ok: true,
        capability,
        semantics: spec.semantics,
        evidence: spec.evidence,
    });
    if platform_level {
        result.detail = format!("平台级结果（无设备实例）：{}", result.detail);
    }
    result
}

/// Profile 级纯读检查：供无设备三厂商命令对照表使用，明确标注平台级结果。
pub fn inspect_command(profile_id: &str, mode: Mode, line: &str) -> Inspection {
    let Some(profile) = profile_mod::profile_by_id(profile_id) else {
        return invalid_inspection("未知 Profile ID，不能进行平台级检查");
    };
    inspect_parsed(profile.dialect, profile.kind, mode, line, true)
}

/// 设备会话级纯读检查：真实设备、Profile、当前模式与认证/远程上下文。
/// 纯读：`&Engine` + `&Session`，调用前后状态摘要、虚拟时间和协议状态均不变。
pub fn inspect_device_command(eng: &Engine, dev: DevId, sess: &Session, line: &str) -> Inspection {
    let Some(d) = eng.net.device(dev) else {
        return invalid_inspection("设备不存在，不能伪装成设备级检查");
    };
    let profile = profile_mod::profile_for(d.dialect, d.kind);
    let mut result = inspect_parsed(d.dialect, d.kind, sess.mode, line, false);
    result.detail = format!(
        "{} [profile={} mode={} authenticated={} remote={}]",
        result.detail,
        profile.id,
        sess.mode.tag(),
        sess.login.is_none(),
        sess.remote.is_some()
    );
    result
}

pub fn prompt(eng: &Engine, sess: &Session, dev: DevId) -> String {
    if let Some(flow) = &sess.login {
        return match flow.step {
            LoginStep::Username => "Username: ".into(),
            LoginStep::Password => "Password: ".into(),
        };
    }
    if sess.enable_prompt {
        return "Password: ".into();
    }
    if let Some(remote) = &sess.remote {
        return prompt(eng, &remote.inner, remote.dev);
    }
    let Some(d) = eng.net.device(dev) else {
        return "?".into();
    };
    if d.kind == DeviceKind::Host {
        return format!("{}>", d.name);
    }
    let dialect = d.dialect;
    match dialect {
        Dialect::Ios => match sess.mode {
            Mode::Exec => format!("{}>", d.name),
            Mode::Priv => format!("{}#", d.name),
            Mode::Config => format!("{}(config)#", d.name),
            Mode::If => format!("{}(config-if)#", d.name),
            Mode::Pool => format!("{}(dhcp-config)#", d.name),
            Mode::Acl => format!("{}(config-acl)#", d.name),
            Mode::Line => format!("{}(config-line)#", d.name),
            Mode::Aaa => format!("{}(config-aaa)#", d.name),
            Mode::Mst => format!("{}(config-mst)#", d.name),
            Mode::Ospf | Mode::OspfArea | Mode::Bgp | Mode::Isis | Mode::Ospfv3 => {
                format!("{}(config-router)#", d.name)
            }
            Mode::RoutePolicy => format!("{}(config-route-map)#", d.name),
            Mode::Zone => format!("{}(config-sec-zone)#", d.name),
            Mode::SecurityPolicy => format!("{}(config-sec-policy)#", d.name),
            Mode::SecurityRule => format!("{}(config-sec-zone-pair)#", d.name),
            Mode::ClassMap => format!("{}(config-cmap)#", d.name),
            Mode::PolicyMap => format!("{}(config-pmap)#", d.name),
            Mode::PolicyMapClass => format!("{}(config-pmap-c)#", d.name),
            Mode::Ipsec => format!("{}(config-ipsec)#", d.name),
            // IOS 不会进入该视图（vlan <id> 仍不改模式）；保留可识别提示符兜底。
            Mode::Vlan => match sess.sub.vlan {
                Some(v) => format!("{}(config-vlan{})#", d.name, v),
                None => format!("{}(config-vlan)#", d.name),
            },
            // IOS 不会进入 tmsh 模块；保留可识别兜底。
            Mode::TmosLtm | Mode::TmosSys => format!("{}>", d.name),
        },
        Dialect::Vrp | Dialect::Comware => match sess.mode {
            Mode::Exec | Mode::Priv => format!("<{}>", d.name),
            Mode::Config => format!("[{}]", d.name),
            Mode::If => {
                let ifn = match sess.cur_if {
                    Some(cli_engine::IfCtx::Svi(v)) => ifname::svi_canonical(dialect, v),
                    Some(cli_engine::IfCtx::Physical(p)) => ifname::canonical(dialect, d.kind, p),
                    Some(cli_engine::IfCtx::Lag(id)) => ifname::lag_canonical(dialect, id),
                    None => String::new(),
                };
                format!("[{}-{}]", d.name, ifn)
            }
            Mode::Pool => {
                let pool = sess.sub.pool.clone().unwrap_or_default();
                format!("[{}-ip-pool-{}]", d.name, pool)
            }
            Mode::Acl => match sess.sub.acl {
                Some((n, true)) => format!("[{}-acl-adv-{}]", d.name, n),
                Some((n, false)) => format!("[{}-acl-basic-{}]", d.name, n),
                None => format!("[{}]", d.name),
            },
            Mode::Line => {
                let prefix = if dialect == Dialect::Comware {
                    "line-vty"
                } else {
                    "ui-vty"
                };
                format!("[{}-{}0-4]", d.name, prefix)
            }
            Mode::Aaa => match (dialect, &sess.sub.aaa_user) {
                (Dialect::Comware, Some(u)) => format!("[{}-luser-manage-{}]", d.name, u),
                _ => format!("[{}-aaa]", d.name),
            },
            Mode::Mst => format!("[{}-mst-region]", d.name),
            Mode::Ospf => {
                let pid = sess
                    .sub
                    .ospf
                    .map(|(p, _)| p)
                    .or(match &d.state {
                        DeviceState::Router(rs) => rs.ospf_process,
                        _ => None,
                    })
                    .unwrap_or(1);
                format!("[{}-ospf-{}]", d.name, pid)
            }
            Mode::OspfArea => {
                let (pid, area) = sess.sub.ospf.unwrap_or_else(|| {
                    let p = match &d.state {
                        DeviceState::Router(rs) => rs.ospf_process.unwrap_or(1),
                        _ => 1,
                    };
                    let a = match &d.state {
                        DeviceState::Router(rs) => rs.ospf_area,
                        _ => 0,
                    };
                    (p, a)
                });
                format!(
                    "[{}-ospf-{}-area-{}.{}.{}.{}]",
                    d.name,
                    pid,
                    (area >> 24) & 0xff,
                    (area >> 16) & 0xff,
                    (area >> 8) & 0xff,
                    area & 0xff
                )
            }
            Mode::Bgp => {
                let asn = eng
                    .net
                    .device(dev)
                    .and_then(|d| match &d.state {
                        DeviceState::Router(rs) => rs.bgp.as_ref().map(|b| b.asn),
                        _ => None,
                    })
                    .unwrap_or(0);
                format!("[{}-bgp-{}]", d.name, asn)
            }
            Mode::Isis => format!("[{}-isis-1]", d.name),
            Mode::RoutePolicy => format!("[{}-route-policy]", d.name),
            Mode::Zone => format!(
                "[{}-zone-{}]",
                d.name,
                sess.sub.zone.clone().unwrap_or_default()
            ),
            Mode::SecurityPolicy => format!("[{}-security-policy]", d.name),
            Mode::SecurityRule => format!(
                "[{}-security-policy-rule-{}]",
                d.name,
                sess.sub.rule.clone().unwrap_or_default()
            ),
            Mode::Ipsec => format!(
                "[{}-ipsec-{}]",
                d.name,
                sess.sub.ipsec.clone().unwrap_or_default()
            ),
            Mode::Ospfv3 => format!("[{}-ospfv3-1]", d.name),
            // E1/E11：官方 VLAN 视图提示符 `[Sysname-vlan2]`。
            Mode::Vlan => match sess.sub.vlan {
                Some(v) => format!("[{}-vlan{}]", d.name, v),
                None => format!("[{}]", d.name),
            },
            Mode::TmosLtm | Mode::TmosSys => format!("[{}]", d.name),
            // 阶段 6B：ZBF 子模式不属于 VRP/Comware；保持可识别兜底，不冒充其提示符。
            Mode::ClassMap | Mode::PolicyMap | Mode::PolicyMapClass => {
                format!("[{}]", d.name)
            }
        },
        // F1：tmsh 提示符显示当前模块位置 `(tmos)#` / `(tmos.ltm)#` / `(tmos.sys)#`，
        // 不含设备名、不使用华为系 `<...>` / `[...]` 格式。
        Dialect::Tmos => match sess.mode {
            Mode::TmosLtm => "(tmos.ltm)#".into(),
            Mode::TmosSys => "(tmos.sys)#".into(),
            _ => "(tmos)#".into(),
        },
    }
}

/// Tab 补全（对主机无效）。
pub fn complete_line(
    eng: &Engine,
    sess: &Session,
    dev: DevId,
    line: &str,
) -> (String, Vec<String>) {
    if sess.login.is_some() {
        return (line.to_string(), vec![]);
    }
    if let Some(remote) = &sess.remote {
        return complete_line(eng, &remote.inner, remote.dev, line);
    }
    let Some(d) = eng.net.device(dev) else {
        return (line.to_string(), vec![]);
    };
    if d.kind == DeviceKind::Host {
        return (line.to_string(), vec![]);
    }
    let t = trees(d.dialect);
    cli_engine::complete(t.root_for(sess.mode), line)
}

fn render_help(dialect: Dialect, sess: &Session, line: &str) -> String {
    let t = trees(dialect);
    let Some(h) = cli_engine::help_for(t.root_for(sess.mode), line, dialect) else {
        return String::new();
    };
    format_help(dialect, &h)
}

fn format_help(_dialect: Dialect, h: &HelpList) -> String {
    let mut out = String::new();
    for (w, help) in &h.items {
        out.push_str(&format!("  {:<20}{}\n", w, help));
    }
    if h.cr {
        out.push_str("  <cr>\n");
    }
    out.trim_end().to_string()
}

/// 阶段 5A：handler 按**参数值**判定「语法有效但语义未模拟」时的提示。
/// 例如 tmsh 中非 `/Common/` 分区或 Route Domain —— 稳定 ID 无法区分参数值，
/// 故由 handler 置 `Ctx::not_simulated`，这里统一生成模拟器提示通道内容。
fn not_simulated_notices(mut notices: Vec<String>) -> Vec<String> {
    notices.push("[netemu] 该命令的承诺语义尚未模拟；命令未执行，设备状态未改变".into());
    notices.push("[netemu] 首批只承诺 /Common/ 分区；其他 Partition / Route Domain 待验证".into());
    notices
}

pub fn format_error(dialect: Dialect, line: &str, e: CliError) -> String {
    match dialect {
        Dialect::Ios => match e {
            CliError::Custom(msg) => format!("% {}", msg),
            CliError::Incomplete { .. } => "% Incomplete command.".into(),
            CliError::Ambiguous { pos } => {
                format!("% Ambiguous command:  \"{}\"", token_at(line, pos))
            }
            other => {
                let pos = match other {
                    CliError::Unknown { pos }
                    | CliError::TooMany { pos }
                    | CliError::BadParam { pos } => pos,
                    _ => 0,
                };
                let chars: Vec<char> = line.chars().collect();
                let n = pos.min(chars.len());
                let mut caret = String::new();
                for _ in 0..n {
                    caret.push(' ');
                }
                caret.push('^');
                format!("{}\n% Invalid input detected at '^' marker.", caret)
            }
        },
        Dialect::Vrp => match e {
            CliError::Custom(msg) => format!("Error: {}", msg),
            CliError::Incomplete { .. } => {
                "Error: Incomplete command found at '^' position.".into()
            }
            CliError::Ambiguous { .. } => "Error: Ambiguous command found at '^' position.".into(),
            CliError::TooMany { .. } => "Error: Too many parameters found at '^' position.".into(),
            CliError::BadParam { .. } => "Error: Wrong parameter found at '^' position.".into(),
            CliError::Unknown { .. } => "Error: Unrecognized command found at '^' position.".into(),
        },
        Dialect::Comware => match e {
            CliError::Custom(msg) => format!("% {}", msg),
            CliError::Incomplete { .. } => "% Incomplete command found at '^' position.".into(),
            CliError::Ambiguous { .. } => "% Ambiguous command found at '^' position.".into(),
            CliError::TooMany { .. } => "% Too many parameters found at '^' position.".into(),
            CliError::BadParam { .. } => "% Wrong parameter found at '^' position.".into(),
            CliError::Unknown { .. } => "% Unrecognized command found at '^' position.".into(),
        },
        // F5 tmsh 报错风格：不使用华为系 `Error:` / `%` 前缀。
        // 无设备样本，逐字文本不建黄金；以下是项目侧结构化错误通道。
        Dialect::Tmos => match e {
            CliError::Custom(msg) => format!("Syntax Error: {}", msg),
            CliError::Incomplete { .. } => "Syntax Error: Incomplete command".into(),
            CliError::Ambiguous { .. } => "Syntax Error: Ambiguous command".into(),
            CliError::TooMany { .. } => "Syntax Error: Too many parameters".into(),
            CliError::BadParam { .. } => "Syntax Error: Invalid parameter".into(),
            CliError::Unknown { .. } => "Syntax Error: Unexpected argument".into(),
        },
    }
}

/// 取行内 pos 处的 token（供 IOS `% Ambiguous command` 引用原词）。
fn token_at(line: &str, pos: usize) -> String {
    cli_engine::tokenize(line)
        .into_iter()
        .find(|(_, start)| *start == pos)
        .map(|(w, _)| w)
        .unwrap_or_default()
}
