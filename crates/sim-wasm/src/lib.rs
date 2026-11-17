//! WASM 桥：把仿真内核与 CLI 暴露给前端。
//!
//! 边界约定：所有结构化数据以 JSON 字符串传递，TS 侧类型见 web/src/lib/sim.ts。

use std::collections::HashMap;

use sim_core::device::{DeviceKind, DeviceState, Dialect};
use sim_core::engine::{Engine, PING_HORIZON_NS};
use sim_core::profile::{
    self as profile_mod, LEGACY_PROFILE_NOTICE, LOAD_RESET_NOTICE, SCHEMA_VERSION,
};
use sim_core::summary::NetSummary;
use sim_core::{DevId, Net};
use sim_packets::ipv4_from_str;
use wasm_bindgen::prelude::*;

mod lab_defs;

/// 阶段 5A：旧 F5 存档（LoadBalancer + 非 Tmos 方言）加载时对用户可见的提示。
/// 抽象 `ltm` 命令已从 IOS/VRP/Comware 树移除，Pool/VS 状态按 sim-core 原样迁移，
/// 但旧命令文本需要按真实 tmsh 重写；不允许静默改变实验判定。
pub const F5_LEGACY_NOTICE: &str =
    "旧 F5 存档已映射到独立的 TMOS 17.x / tmsh 方言；Pool 与 Virtual Server 状态已保留，但旧的抽象 ltm 命令不再可解析，请按课程原文改用真实 tmsh 命令";

/// 存档加载报告：旧档映射与会话清空都必须对用户可见。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadReport {
    pub schema_version: u32,
    pub legacy: bool,
    pub mapped_profiles: usize,
    pub notices: Vec<String>,
}

#[wasm_bindgen]
pub struct Sim {
    eng: Engine,
    sessions: HashMap<DevId, cli_engine::Session>,
    /// 设备会话令牌：设备会话级纯读检查必须携带真实的会话上下文。
    session_ids: HashMap<DevId, String>,
    next_session_seq: u64,
    kind_counts: HashMap<String, u32>,
}

fn mode_from(s: &str) -> Option<cli_engine::Mode> {
    use cli_engine::Mode;
    Some(match s {
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
        "Vlan" => Mode::Vlan,
        // 阶段 5A：tmsh 模块层级。
        "TmosLtm" => Mode::TmosLtm,
        "TmosSys" => Mode::TmosSys,
        _ => return None,
    })
}

fn inspection_json(i: &cli_engine::spec::Inspection) -> String {
    serde_json::json!({
        "passed": i.passed,
        "detail": i.detail,
        "status": i.status.label(),
        "support": i.support.label(),
        "evidence": i.evidence,
        "specId": i.spec_id,
    })
    .to_string()
}

fn kind_from(s: &str) -> Option<DeviceKind> {
    match s {
        "Router" | "router" => Some(DeviceKind::Router),
        "Switch" | "switch" => Some(DeviceKind::Switch),
        "Host" | "host" | "PC" | "pc" => Some(DeviceKind::Host),
        "Firewall" | "firewall" | "FW" | "fw" => Some(DeviceKind::Firewall),
        "AccessPoint" | "accesspoint" | "AP" | "ap" => Some(DeviceKind::AccessPoint),
        "WlanController" | "wlancontroller" | "AC" | "ac" => Some(DeviceKind::WlanController),
        "LoadBalancer" | "loadbalancer" | "LB" | "lb" | "F5" | "f5" => {
            Some(DeviceKind::LoadBalancer)
        }
        _ => None,
    }
}

fn dialect_from(s: &str) -> Option<Dialect> {
    match s {
        "Ios" | "ios" | "cisco" => Some(Dialect::Ios),
        "Vrp" | "vrp" | "huawei" => Some(Dialect::Vrp),
        "Comware" | "comware" | "h3c" => Some(Dialect::Comware),
        // 阶段 5A：F5 BIG-IP TMOS 17.x / tmsh。
        "Tmos" | "tmos" | "f5" | "F5" | "bigip" => Some(Dialect::Tmos),
        _ => None,
    }
}

impl Sim {
    fn sync_sessions(&mut self) {
        // 移除已删除设备的会话
        let ids: Vec<u32> = self.sessions.keys().copied().collect();
        for id in ids {
            if self.eng.net.device(id).is_none() {
                self.sessions.remove(&id);
                self.session_ids.remove(&id);
            }
        }
    }

    /// 存档加载的纯逻辑入口（不经过 JsValue，便于原生测试）。
    /// 保留"先在临时引擎构建，全部校验成功后再替换"的语义。
    pub fn apply_summary_checked(&mut self, json: &str) -> Result<LoadReport, String> {
        let mut summary: NetSummary =
            serde_json::from_str(json).map_err(|e| format!("invalid topology: {e}"))?;
        let legacy = summary.schema_version < SCHEMA_VERSION
            || summary.devices.iter().any(|d| d.profile.is_none());
        let mut mapped = 0usize;
        let mut f5_migrated: Vec<(String, u32)> = Vec::new();
        for d in &mut summary.devices {
            // 阶段 5A：旧 F5 存档用 IOS/VRP/Comware 方言承载抽象 `ltm` 命令。
            // 这里只改方言与 Profile，LTM 状态由 apply_summary 原样还原（唯一状态源），
            // 并强制生成用户可见提示，避免静默改变实验判定。
            if d.kind == DeviceKind::LoadBalancer && d.dialect != Dialect::Tmos {
                d.dialect = Dialect::Tmos;
                d.profile = Some(profile_mod::TMOS_LB_PROFILE.to_string());
                f5_migrated.push((d.name.clone(), d.id));
                mapped += 1;
            }
            if d.profile.is_none() {
                d.profile = Some(profile_mod::profile_id_for(d.dialect, d.kind).to_string());
                mapped += 1;
            }
        }
        summary.schema_version = SCHEMA_VERSION;

        let mut eng = Engine::new(Net::new());
        eng.net.apply_summary(&summary)?;
        eng.after_load();
        self.eng = eng;
        // 加载成功后清空所有终端会话与会话令牌，避免旧上下文进入新工作区。
        self.sessions.clear();
        self.session_ids.clear();
        // 会话序号保持单调：加载/重置后重新签发的令牌不得与旧令牌相同，
        // 否则已失效的旧令牌会重新生效。
        self.kind_counts.clear();
        for d in &self.eng.net.devices {
            let base = match d.kind {
                DeviceKind::Router => "Router",
                DeviceKind::Switch => "Switch",
                DeviceKind::Host => "PC",
                DeviceKind::Firewall => "FW",
                DeviceKind::AccessPoint => "AP",
                DeviceKind::WlanController => "AC",
                DeviceKind::LoadBalancer => "LB",
            };
            if let Some(sfx) = d.name.strip_prefix(base) {
                if let Ok(n) = sfx.parse::<u32>() {
                    let slot = self.kind_counts.entry(base.to_string()).or_insert(0);
                    *slot = (*slot).max(n);
                }
            }
        }
        let mut notices = vec![LOAD_RESET_NOTICE.to_string()];
        if legacy {
            notices.push(LEGACY_PROFILE_NOTICE.to_string());
            for d in &summary.devices {
                if let Some(p) = &d.profile {
                    notices.push(format!("设备 {}(id={}) → Profile {}", d.name, d.id, p));
                }
            }
        }
        // 阶段 5A：F5 迁移提示独立于 legacy 标记，任何旧 F5 档都必须可见。
        if !f5_migrated.is_empty() {
            notices.push(F5_LEGACY_NOTICE.to_string());
            for (name, id) in &f5_migrated {
                notices.push(format!(
                    "旧 F5 设备 {}(id={}) → Dialect::Tmos / Profile {}",
                    name,
                    id,
                    profile_mod::TMOS_LB_PROFILE
                ));
            }
        }
        Ok(LoadReport {
            schema_version: SCHEMA_VERSION,
            legacy,
            mapped_profiles: mapped,
            notices,
        })
    }

    /// 设备会话级检查的纯逻辑入口（不经过 `JsValue`，便于原生契约测试）。
    /// 会话令牌错误以明确字符串返回，不会被当成「设备不支持」的结构化结果。
    pub fn verify_device_command_checked(
        &self,
        dev: u32,
        session_id: &str,
        line: &str,
    ) -> Result<cli_engine::spec::Inspection, String> {
        let Some(token) = self.session_ids.get(&dev) else {
            return Err("no active device session".to_string());
        };
        if token != session_id {
            return Err("session id mismatch".to_string());
        }
        let Some(sess) = self.sessions.get(&dev) else {
            return Err("no active device session".to_string());
        };
        Ok(cli_dialects::inspect_device_command(
            &self.eng, dev, sess, line,
        ))
    }
}

#[wasm_bindgen]
impl Sim {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Sim {
        Sim {
            eng: Engine::new(Net::new()),
            sessions: HashMap::new(),
            session_ids: HashMap::new(),
            next_session_seq: 0,
            kind_counts: HashMap::new(),
        }
    }

    /// 添加设备，自动命名（Router1 / Switch1 / PC1…），返回设备 id。
    pub fn add_device(&mut self, kind: &str, dialect: &str) -> Result<u32, JsValue> {
        let kind = kind_from(kind).ok_or_else(|| JsValue::from_str("unknown device kind"))?;
        let dialect = dialect_from(dialect).ok_or_else(|| JsValue::from_str("unknown dialect"))?;
        let base = match kind {
            DeviceKind::Router => "Router",
            DeviceKind::Switch => "Switch",
            DeviceKind::Host => "PC",
            DeviceKind::Firewall => "FW",
            DeviceKind::AccessPoint => "AP",
            DeviceKind::WlanController => "AC",
            DeviceKind::LoadBalancer => "LB",
        };
        let n = self.kind_counts.entry(base.to_string()).or_insert(0);
        *n += 1;
        let name = format!("{}{}", base, n);
        let id = self.eng.net.add_device(name, kind, dialect);
        self.sessions.insert(id, cli_engine::Session::default());
        self.eng.topology_changed();
        Ok(id)
    }

    pub fn remove_device(&mut self, dev: u32) {
        self.eng.net.remove_device(dev);
        self.sessions.remove(&dev);
        self.sync_sessions();
        self.eng.topology_changed();
    }

    pub fn rename_device(&mut self, dev: u32, name: &str) -> Result<(), JsValue> {
        let Some(d) = self.eng.net.device_mut(dev) else {
            return Err(JsValue::from_str("device not found"));
        };
        d.name = name.to_string();
        Ok(())
    }

    pub fn set_dialect(&mut self, dev: u32, dialect: &str) -> Result<(), JsValue> {
        let d = dialect_from(dialect).ok_or_else(|| JsValue::from_str("unknown dialect"))?;
        let Some(dev_ref) = self.eng.net.device_mut(dev) else {
            return Err(JsValue::from_str("device not found"));
        };
        dev_ref.dialect = d;
        self.sessions.insert(dev, cli_engine::Session::default());
        self.session_ids.remove(&dev);
        Ok(())
    }

    /// 连接两台设备（自动选取两端第一个空闲端口），返回链路 id。
    pub fn add_link(&mut self, a: u32, b: u32) -> Result<u32, JsValue> {
        let id = self
            .eng
            .net
            .add_link(a, b)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.eng.topology_changed();
        Ok(id)
    }

    pub fn remove_link(&mut self, link: u32) {
        self.eng.net.remove_link(link);
        self.eng.topology_changed();
    }

    /// 启用/关闭主机 DHCP，并立即发起 Discover（驱动至静默）。
    pub fn set_host_dhcp(&mut self, dev: u32, enable: bool) -> Result<String, JsValue> {
        let Some(d) = self.eng.net.device_mut(dev) else {
            return Err(JsValue::from_str("device not found"));
        };
        if d.kind != DeviceKind::Host {
            return Err(JsValue::from_str("not a host"));
        }
        if let DeviceState::Host(h) = &mut d.state {
            h.dhcp_enable = enable;
            h.dhcp = if enable {
                Some(sim_core::device::HostDhcp { discovering: false })
            } else {
                None
            };
        }
        if enable {
            self.eng.dhcp_discover(dev);
        } else if let Some(p) = d.port_mut(0) {
            p.ip = None;
            if let DeviceState::Host(h) = &mut d.state {
                h.gw = None;
            }
        }
        let h = self.eng.drain_horizon();
        self.eng.run_to_quiescence(h);
        let Some(d) = self.eng.net.device(dev) else {
            return Ok("{}".into());
        };
        Ok(serde_json::json!({
            "ip": d.ports[0].ip.map(sim_packets::ipv4_str),
            "prefix": d.ports[0].prefix,
        })
        .to_string())
    }

    /// 设置主机 IP/掩码/网关（空串表示清除）。
    pub fn set_host_config(
        &mut self,
        dev: u32,
        ip: &str,
        prefix: u8,
        gw: &str,
    ) -> Result<(), JsValue> {
        let Some(d) = self.eng.net.device(dev) else {
            return Err(JsValue::from_str("device not found"));
        };
        if d.kind != DeviceKind::Host {
            return Err(JsValue::from_str("not a host"));
        }
        let ip = if ip.trim().is_empty() {
            None
        } else {
            Some(ipv4_from_str(ip).ok_or_else(|| JsValue::from_str("invalid ip"))?)
        };
        let gw = if gw.trim().is_empty() {
            None
        } else {
            Some(ipv4_from_str(gw).ok_or_else(|| JsValue::from_str("invalid gateway"))?)
        };
        self.eng.net.set_host(dev, ip, prefix, gw);
        Ok(())
    }

    /// 阶段 5A：实验拓扑预配置设备物理口 IPv4 地址（属性面板语义）。
    /// TMOS 的 `net self` / VLAN 对象不在首批范围，F5 实验用此 API 预置接口地址，
    /// 与「PC 网关在属性面板填」同属拓扑预配置，不是 CLI 任务、不新建数据面。
    pub fn set_port_ip(
        &mut self,
        dev: u32,
        port: u32,
        ip: &str,
        prefix: u8,
    ) -> Result<(), JsValue> {
        let Some(d) = self.eng.net.device(dev) else {
            return Err(JsValue::from_str("device not found"));
        };
        if d.kind == DeviceKind::Host {
            return Err(JsValue::from_str("use set_host_config for hosts"));
        }
        let addr = ipv4_from_str(ip).ok_or_else(|| JsValue::from_str("invalid ip"))?;
        self.eng
            .net
            .set_port_ip(dev, port as usize, Some(addr), prefix);
        Ok(())
    }

    /// 设置主机 IPv6 地址/前缀/网关（空串表示清除）。
    pub fn set_host_v6(&mut self, dev: u32, ip: &str, prefix: u8, gw: &str) -> Result<(), JsValue> {
        let Some(d) = self.eng.net.device(dev) else {
            return Err(JsValue::from_str("device not found"));
        };
        if d.kind != DeviceKind::Host {
            return Err(JsValue::from_str("not a host"));
        }
        let ip = if ip.trim().is_empty() {
            None
        } else {
            Some(sim_packets::ipv6_from_str(ip).ok_or_else(|| JsValue::from_str("invalid ipv6"))?)
        };
        let gw = if gw.trim().is_empty() {
            None
        } else {
            Some(
                sim_packets::ipv6_from_str(gw)
                    .ok_or_else(|| JsValue::from_str("invalid ipv6 gateway"))?,
            )
        };
        self.eng.net.set_host_v6(dev, ip, prefix, gw);
        Ok(())
    }

    /// 执行一行 CLI 并返回结构化结果 JSON（设备文本与模拟器提示分通道）。
    pub fn exec_command_result(&mut self, dev: u32, line: &str) -> Result<String, JsValue> {
        if self.eng.net.device(dev).is_none() {
            return Err(JsValue::from_str("device not found"));
        }
        self.sessions.entry(dev).or_default();
        let Some(sess) = self.sessions.get_mut(&dev) else {
            return Err(JsValue::from_str("device not found"));
        };
        let r = cli_dialects::exec_and_run_result(&mut self.eng, sess, dev, line);
        Ok(serde_json::json!({
            "status": r.status.label(),
            "deviceOutput": r.device_output,
            "simulatorNotices": r.simulator_notices,
            "stateChanged": r.state_changed,
        })
        .to_string())
    }

    /// 迁移期文本契约：返回设备文本；模拟器提示以明确的 `[netemu]` 行单独附加，
    /// 不伪装成厂商输出。前端应改用 `exec_command_result` 分通道各渲染一次。
    pub fn exec_command(&mut self, dev: u32, line: &str) -> Result<String, JsValue> {
        if self.eng.net.device(dev).is_none() {
            return Err(JsValue::from_str("device not found"));
        }
        self.sessions.entry(dev).or_default();
        let Some(sess) = self.sessions.get_mut(&dev) else {
            return Err(JsValue::from_str("device not found"));
        };
        let r = cli_dialects::exec_and_run_result(&mut self.eng, sess, dev, line);
        let mut parts = vec![];
        if !r.device_output.is_empty() {
            parts.push(r.device_output);
        }
        for n in &r.simulator_notices {
            parts.push(format!("[netemu] {}", n.trim_start_matches("[netemu] ")));
        }
        Ok(parts.join("\n"))
    }

    /// 设备会话令牌。加载/重置后旧令牌失效，避免旧上下文进入新工作区。
    pub fn session_id(&mut self, dev: u32) -> Result<String, JsValue> {
        if self.eng.net.device(dev).is_none() {
            return Err(JsValue::from_str("device not found"));
        }
        self.sessions.entry(dev).or_default();
        if let Some(id) = self.session_ids.get(&dev) {
            return Ok(id.clone());
        }
        self.next_session_seq += 1;
        let token = format!("dev{}-s{}", dev, self.next_session_seq);
        self.session_ids.insert(dev, token.clone());
        Ok(token)
    }

    /// Profile 级纯读命令检查（平台级结果，无设备实例）。mode 见 `verify_command`。
    pub fn inspect_command(
        &self,
        profile_id: &str,
        mode: &str,
        line: &str,
    ) -> Result<String, JsValue> {
        let m = mode_from(mode).ok_or_else(|| JsValue::from_str("unknown mode"))?;
        Ok(inspection_json(&cli_dialects::inspect_command(
            profile_id, m, line,
        )))
    }

    /// 设备会话级纯读命令检查：需要真实设备、Profile、模式与认证上下文。
    /// 不推进虚拟时间、不运行 `run_checks`、不发探测包。
    pub fn verify_device_command(
        &self,
        dev: u32,
        session_id: &str,
        line: &str,
    ) -> Result<String, JsValue> {
        let inspection = self
            .verify_device_command_checked(dev, session_id, line)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(inspection_json(&inspection))
    }

    /// 加载并返回迁移/会话清空报告；`load()` 保持原有无返回值契约。
    pub fn load_with_report(&mut self, json: &str) -> Result<String, JsValue> {
        let report = self
            .apply_summary_checked(json)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(serde_json::json!({
            "schemaVersion": report.schema_version,
            "legacy": report.legacy,
            "mappedProfiles": report.mapped_profiles,
            "notices": report.notices,
        })
        .to_string())
    }

    /// 当前提示符。
    pub fn prompt(&mut self, dev: u32) -> Result<String, JsValue> {
        self.sessions.entry(dev).or_default();
        let Some(sess) = self.sessions.get_mut(&dev) else {
            return Err(JsValue::from_str("device not found"));
        };
        Ok(cli_dialects::prompt(&self.eng, sess, dev))
    }

    /// Tab 补全：返回 JSON {line, options}。
    pub fn complete(&mut self, dev: u32, line: &str) -> Result<String, JsValue> {
        self.sessions.entry(dev).or_default();
        let Some(sess) = self.sessions.get_mut(&dev) else {
            return Err(JsValue::from_str("device not found"));
        };
        let (l, opts) = cli_dialects::complete_line(&self.eng, sess, dev, line);
        Ok(serde_json::json!({ "line": l, "options": opts }).to_string())
    }

    /// 运行引擎至静默（加载拓扑或外部改动后调用）。
    pub fn run(&mut self) -> String {
        let s = self.eng.run_to_quiescence(PING_HORIZON_NS);
        serde_json::json!({
            "virtualNs": s.virtual_ns,
            "events": s.events,
            "inFlight": s.in_flight,
            "dropped": s.dropped,
        })
        .to_string()
    }

    /// 引擎统计。
    pub fn stats(&mut self) -> String {
        let s = self.eng.stats();
        serde_json::json!({
            "virtualNs": s.virtual_ns,
            "events": s.events,
            "inFlight": s.in_flight,
            "dropped": s.dropped,
        })
        .to_string()
    }

    /// 拓扑摘要（含配置态）。
    pub fn dump(&self) -> String {
        serde_json::to_string(&self.eng.net.summary()).unwrap_or_else(|_| "{}".into())
    }

    /// 应用拓扑摘要（保存/加载/分享的数据格式）。
    /// 迁移与会话清空逻辑见 `apply_summary_checked`；UI 提示请用 `load_with_report`。
    pub fn load(&mut self, json: &str) -> Result<(), JsValue> {
        self.apply_summary_checked(json)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(())
    }

    /// 清空一切。
    pub fn reset(&mut self) {
        self.eng = Engine::new(Net::new());
        self.sessions.clear();
        self.session_ids.clear();
        // 会话序号保持单调：加载/重置后重新签发的令牌不得与旧令牌相同，
        // 否则已失效的旧令牌会重新生效。
        self.kind_counts.clear();
    }

    // ---------- 实验检查（lab-checks） ----------

    /// 纯读批量检查：只执行纯读项，主动项标 `skipped`；不推进时间、不收敛、不发包。
    /// 入参为 Web `LabCheckDef` 的 `[{label, kind}]` JSON，设备名按真实引擎解析。
    pub fn read_only_lab_checks(&self, defs_json: &str) -> Result<String, JsValue> {
        let checks =
            lab_defs::parse_lab_checks(&self.eng, defs_json).map_err(|e| JsValue::from_str(&e))?;
        Ok(lab_defs::results_json(&lab_checks::run_read_only_checks(
            &self.eng, &checks,
        )))
    }

    /// 主动实验批量判分：整批一次完成必要收敛，再执行全部检查（含收敛后的状态项）。
    /// 返回 `[{label, passed, detail, behavior, skipped}]`，负向探测同样标为主动行为。
    pub fn run_lab_checks(&mut self, defs_json: &str) -> Result<String, JsValue> {
        let checks =
            lab_defs::parse_lab_checks(&self.eng, defs_json).map_err(|e| JsValue::from_str(&e))?;
        let results = lab_checks::run_active_checks(&mut self.eng, &checks);
        Ok(lab_defs::results_json(&results))
    }

    /// 检查连通性：从 from 设备 ping 目标 IP。
    pub fn check_ping(&mut self, from: u32, to: &str) -> Result<String, JsValue> {
        let to = ipv4_from_str(to).ok_or_else(|| JsValue::from_str("invalid ip"))?;
        let checks = vec![lab_checks::Check {
            label: String::new(),
            kind: lab_checks::CheckKind::Ping {
                from,
                to,
                expect: true,
            },
        }];
        // Ping 是主动探测：走明示的主动入口（旧 run_checks 兼容契约）。
        let r = lab_checks::run_checks(&mut self.eng, &checks);
        Ok(serde_json::json!({
            "passed": r[0].passed,
            "detail": r[0].detail,
        })
        .to_string())
    }

    /// 检查路由器是否存在去往 dst/len 的静态路由。
    pub fn check_static_route(&mut self, dev: u32, dst: &str) -> Result<String, JsValue> {
        let (dst, len) = lab_checks::parse_net(dst)
            .ok_or_else(|| JsValue::from_str("invalid net, expect a.b.c.d/len"))?;
        let checks = vec![lab_checks::Check {
            label: String::new(),
            kind: lab_checks::CheckKind::RouteExists {
                dev,
                dst,
                len,
                src: Some(sim_core::device::RouteSrc::Static),
            },
        }];
        let r = lab_checks::run_checks_read_only(&self.eng, &checks);
        Ok(serde_json::json!({
            "passed": r[0].passed,
            "detail": r[0].detail,
        })
        .to_string())
    }

    /// 检查 VLAN 已创建。
    pub fn check_vlan(&mut self, dev: u32, vlan: u16) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::VlanExists { dev, vlan },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查端口 Access VLAN。
    pub fn check_access_vlan(&mut self, dev: u32, port: u32, vlan: u16) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::AccessVlan {
                    dev,
                    port: port as usize,
                    vlan,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 Trunk 放行 VLAN。
    pub fn check_trunk_allows(
        &mut self,
        dev: u32,
        port: u32,
        vlan: u16,
    ) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::TrunkAllows {
                    dev,
                    port: port as usize,
                    vlan,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查接口 IP（含主机口）。
    pub fn check_port_ip(&mut self, dev: u32, port: u32, ip: &str) -> Result<String, JsValue> {
        use sim_packets::ipv4_from_str;
        let ip = ipv4_from_str(ip).ok_or_else(|| JsValue::from_str("invalid ip"))?;
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::PortIp {
                    dev,
                    port: port as usize,
                    ip,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查路由存在（连接或静态均可）。
    pub fn check_route(&mut self, dev: u32, dst: &str) -> Result<String, JsValue> {
        let (dst, len) = lab_checks::parse_net(dst)
            .ok_or_else(|| JsValue::from_str("invalid net, expect a.b.c.d/len"))?;
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::RouteExists {
                    dev,
                    dst,
                    len,
                    src: None,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 EVPN 路由。
    pub fn check_evpn_route(&mut self, dev: u32, vtep: &str, vni: u32) -> Result<String, JsValue> {
        let vtep =
            sim_packets::ipv4_from_str(vtep).ok_or_else(|| JsValue::from_str("invalid VTEP"))?;
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::EvpnRoute { dev, vtep, vni },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 LTM 虚拟服务器。
    pub fn check_ltm_virtual(
        &mut self,
        dev: u32,
        name: &str,
        vip: &str,
        port: u32,
    ) -> Result<String, JsValue> {
        let vip =
            sim_packets::ipv4_from_str(vip).ok_or_else(|| JsValue::from_str("invalid VIP"))?;
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::LtmVirtual {
                    dev,
                    name: name.to_string(),
                    vip,
                    port: port as u16,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 TCP 探测。
    pub fn check_tcp_probe(
        &mut self,
        from: u32,
        to: &str,
        port: u32,
        expect: bool,
    ) -> Result<String, JsValue> {
        let to =
            sim_packets::ipv4_from_str(to).ok_or_else(|| JsValue::from_str("invalid target"))?;
        let r = lab_checks::run_checks(
            &mut self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::TcpProbe {
                    from,
                    to,
                    port: port as u16,
                    expect,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查堆叠成员配置。
    pub fn check_stack_member(
        &mut self,
        dev: u32,
        domain: u32,
        member: u32,
    ) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::StackMember {
                    dev,
                    domain: domain as u16,
                    member: member as u8,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 M-LAG domain。
    pub fn check_mlag_domain(&mut self, dev: u32, domain: u32) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::MlagDomain {
                    dev,
                    domain: domain as u16,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 VXLAN VNI 映射。
    pub fn check_vxlan_vni(&mut self, dev: u32, vlan: u32, vni: u32) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::VxlanVni {
                    dev,
                    vlan: vlan as u16,
                    vni,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查主机当前关联的 AP。
    pub fn check_wifi_on_ap(&mut self, dev: u32, ap: u32) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::WifiOnAp { dev, ap },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 AP 已加入 AC。
    pub fn check_wlan_ap_joined(&mut self, ac: u32, ap: u32) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::WlanApJoined { ac, ap },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 AC 的 SSID。
    pub fn check_wlan_ssid(&mut self, ac: u32, ssid: &str) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::WlanSsid {
                    ac,
                    ssid: ssid.to_string(),
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查主机 Wi-Fi 关联。
    pub fn check_wifi_connected(&mut self, dev: u32) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::WifiConnected { dev },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 IPsec 隧道是否存在。
    pub fn check_ipsec_tunnel(&mut self, dev: u32, name: &str) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::IpsecTunnel {
                    dev,
                    name: name.to_string(),
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查主机 SSL VPN 会话。
    pub fn check_vpn_connected(&mut self, dev: u32) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::VpnConnected { dev },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查端口安全区域。
    pub fn check_security_zone(
        &mut self,
        dev: u32,
        port: u32,
        zone: &str,
    ) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::SecurityZone {
                    dev,
                    port: port as usize,
                    zone: zone.to_string(),
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查区域间策略。
    pub fn check_security_policy(
        &mut self,
        dev: u32,
        name: &str,
        src_zone: &str,
        dst_zone: &str,
        permit: bool,
    ) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::SecurityPolicy {
                    dev,
                    name: name.to_string(),
                    src_zone: src_zone.to_string(),
                    dst_zone: dst_zone.to_string(),
                    permit,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 VRRP 组是否为 Master。
    pub fn check_vrrp_master(&mut self, dev: u32, vrid: u32) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::VrrpMaster {
                    dev,
                    vrid: vrid as u8,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 OSPF 邻居数量。
    pub fn check_ospf_neighbors(&mut self, dev: u32, min: u32) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::OspfNeighbors {
                    dev,
                    min: min as usize,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查端口隔离组。
    pub fn check_port_isolate(
        &mut self,
        dev: u32,
        port: u32,
        group: u32,
    ) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::PortIsolate {
                    dev,
                    port: port as usize,
                    group: group as u16,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 MUX VLAN 端口角色。
    pub fn check_mux_role(&mut self, dev: u32, port: u32, role: &str) -> Result<String, JsValue> {
        let role = match role.to_ascii_lowercase().as_str() {
            "principal" => sim_core::device::MuxRole::Principal,
            "separate" => sim_core::device::MuxRole::Separate,
            other => sim_core::device::MuxRole::Group(
                other
                    .parse()
                    .map_err(|_| JsValue::from_str("invalid mux role"))?,
            ),
        };
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::MuxRole {
                    dev,
                    port: port as usize,
                    role,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 OSPFv3 邻居数。
    pub fn check_ospfv3_neighbors(&mut self, dev: u32, min: u32) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::Ospfv3Neighbors {
                    dev,
                    min: min as usize,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 SRv6 静态路由。
    pub fn check_srv6_route(&mut self, dev: u32, dst: &str) -> Result<String, JsValue> {
        let (addr, len) = dst
            .split_once('/')
            .ok_or_else(|| JsValue::from_str("invalid IPv6 prefix"))?;
        let dst = sim_packets::ipv6_from_str(addr)
            .ok_or_else(|| JsValue::from_str("invalid IPv6 address"))?;
        let len: u8 = len
            .parse()
            .map_err(|_| JsValue::from_str("invalid prefix length"))?;
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::Srv6Route { dev, dst, len },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// IPv6 ping 探测。
    pub fn check_ping6(&mut self, from: u32, to: &str) -> Result<String, JsValue> {
        let to = sim_packets::ipv6_from_str(to)
            .ok_or_else(|| JsValue::from_str("invalid IPv6 address"))?;
        let r = lab_checks::run_checks(
            &mut self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::Ping6 {
                    from,
                    to,
                    expect: true,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查静态 LSP 是否存在（MPLS 实验判分）。
    pub fn check_mpls_lsp(&mut self, dev: u32, name: &str) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::MplsLsp {
                    dev,
                    name: name.to_string(),
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查路由不存在（用于策略过滤后的负向断言）。
    pub fn check_route_absent(&mut self, dev: u32, dst: &str) -> Result<String, JsValue> {
        let (dst, len) = lab_checks::parse_net(dst)
            .ok_or_else(|| JsValue::from_str("invalid net, expect a.b.c.d/len"))?;
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::RouteExists {
                    dev,
                    dst,
                    len,
                    src: None,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": !r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// **仅迁移期兼容**：旧的无状态 parse-only 语法检查，不证明设备支持、执行成功
    /// 或状态正确。课程与对照表应改用 `inspect_command` / `verify_device_command`。
    /// dialect: Ios | Vrp | Comware；mode: Exec | Priv | Config | If | Pool | Acl …
    pub fn verify_command(&self, dialect: &str, mode: &str, line: &str) -> bool {
        let d = match dialect {
            "Ios" | "ios" => Dialect::Ios,
            "Vrp" | "vrp" => Dialect::Vrp,
            "Comware" | "comware" => Dialect::Comware,
            "Tmos" | "tmos" | "f5" => Dialect::Tmos,
            _ => return false,
        };
        let Some(m) = mode_from(mode) else {
            return false;
        };
        let trees = cli_dialects::trees(d);
        cli_engine::parse(trees.root_for(m), line)
            .map(|r| r.is_some())
            .unwrap_or(false)
    }

    /// 检查是否 STP 根桥。
    pub fn check_root_bridge(&mut self, dev: u32) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::RootBridge { dev },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查端口 STP 角色：role ∈ "Designated" | "Root" | "Alternate"。
    pub fn check_port_role(&mut self, dev: u32, port: u32, role: &str) -> Result<String, JsValue> {
        use sim_core::device::StpRole;
        let role = match role {
            "Designated" | "DESI" => StpRole::Designated,
            "Root" | "ROOT" => StpRole::Root,
            "Alternate" | "ALTE" => StpRole::Alternate,
            _ => return Err(JsValue::from_str("unknown role")),
        };
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::PortRole {
                    dev,
                    port: port as usize,
                    role,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 SVI IP。
    pub fn check_svi_ip(&mut self, dev: u32, vlan: u16, ip: &str) -> Result<String, JsValue> {
        use sim_packets::ipv4_from_str;
        let ip = ipv4_from_str(ip).ok_or_else(|| JsValue::from_str("invalid ip"))?;
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::SviIp { dev, vlan, ip },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查端口是否属于指定聚合组。
    pub fn check_lag_member(&mut self, dev: u32, group: u16, port: u32) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::LagMember {
                    dev,
                    group,
                    port: port as usize,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查聚合组已选中的成员数。
    pub fn check_lag_selected(
        &mut self,
        dev: u32,
        group: u16,
        count: u32,
    ) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::LagSelected {
                    dev,
                    group,
                    count: count as usize,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查本地用户存在。
    pub fn check_mgmt_user(&mut self, dev: u32, name: &str) -> Result<String, JsValue> {
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::MgmtUser {
                    dev,
                    name: name.to_string(),
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查 VTY 接入协议：all | ssh | telnet | none。
    pub fn check_vty_transport(&mut self, dev: u32, transport: &str) -> Result<String, JsValue> {
        use sim_core::device::VtyTransport;
        let t = match transport.to_lowercase().as_str() {
            "all" => VtyTransport::All,
            "ssh" => VtyTransport::Ssh,
            "telnet" => VtyTransport::Telnet,
            "none" => VtyTransport::None,
            _ => return Err(JsValue::from_str("unknown transport")),
        };
        let r = lab_checks::run_checks_read_only(
            &self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::VtyTransport { dev, transport: t },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    /// 检查远程登录尝试（proto: telnet | ssh）。
    pub fn check_login(
        &mut self,
        from: u32,
        to: &str,
        proto: &str,
        user: &str,
        pass: &str,
        expect: bool,
    ) -> Result<String, JsValue> {
        use sim_core::device::MgmtProto;
        let to = ipv4_from_str(to).ok_or_else(|| JsValue::from_str("invalid ip"))?;
        let proto = match proto.to_lowercase().as_str() {
            "telnet" => MgmtProto::Telnet,
            "ssh" => MgmtProto::Ssh,
            _ => return Err(JsValue::from_str("unknown protocol")),
        };
        let r = lab_checks::run_checks(
            &mut self.eng,
            &[lab_checks::Check {
                label: String::new(),
                kind: lab_checks::CheckKind::LoginAttempt {
                    from,
                    to,
                    proto,
                    user: user.to_string(),
                    pass: pass.to_string(),
                    expect,
                },
            }],
        );
        Ok(serde_json::json!({ "passed": r[0].passed, "detail": r[0].detail }).to_string())
    }

    // ---------- 抓包 ----------
    /// 开关某条链路的抓包。
    pub fn set_capture(&mut self, link: u32, on: bool) -> Result<(), JsValue> {
        if !self.eng.net.set_capture(link, on) {
            return Err(JsValue::from_str("link not found"));
        }
        Ok(())
    }

    /// 读取抓包记录：返回 JSON { next, rows: [{seq,ms,dir,src,dst,kind,vlan,info}] }。
    /// since 之前的记录被跳过；next = 建议下次传入的 since。
    pub fn capture_rows(&self, link: u32, since: u32) -> Result<String, JsValue> {
        use sim_packets::{ArpOp, L3};
        let Some(l) = self.eng.net.link(link) else {
            return Err(JsValue::from_str("link not found"));
        };
        let rows: Vec<serde_json::Value> = l
            .capture_buf
            .iter()
            .filter(|c| c.seq >= since)
            .map(|c| {
                let (kind, info) = match &c.frame.payload {
                    L3::Arp(a) => (
                        "ARP",
                        format!(
                            "{} {} → {}  谁有 {}? 告诉 {}",
                            if a.op == ArpOp::Request {
                                "请求"
                            } else {
                                "应答"
                            },
                            sim_packets::ipv4_str(a.sender_ip),
                            sim_packets::ipv4_str(a.target_ip),
                            sim_packets::ipv4_str(a.target_ip),
                            sim_packets::ipv4_str(a.sender_ip),
                        ),
                    ),
                    L3::Ipv4(p) => (
                        if p.icmp.is_reply {
                            "EchoReply"
                        } else {
                            "EchoRequest"
                        },
                        format!(
                            "{} → {}  seq={} ttl={}",
                            sim_packets::ipv4_str(p.src),
                            sim_packets::ipv4_str(p.dst),
                            p.icmp.seq,
                            p.ttl
                        ),
                    ),
                    L3::Tcp(t) => {
                        let mut flags = String::new();
                        if t.flags & sim_packets::TCP_SYN != 0 {
                            flags.push_str("SYN ");
                        }
                        if t.flags & sim_packets::TCP_ACK != 0 {
                            flags.push_str("ACK ");
                        }
                        if t.flags & sim_packets::TCP_FIN != 0 {
                            flags.push_str("FIN ");
                        }
                        if t.flags & sim_packets::TCP_RST != 0 {
                            flags.push_str("RST ");
                        }
                        (
                            "TCP",
                            format!(
                                "{}:{} → {}:{}  {}seq={} ack={} ttl={}",
                                sim_packets::ipv4_str(t.src),
                                t.src_port,
                                sim_packets::ipv4_str(t.dst),
                                t.dst_port,
                                flags,
                                t.seq,
                                t.ack,
                                t.ttl
                            ),
                        )
                    }
                    L3::Ipsec(i) => (
                        "IPsec",
                        format!(
                            "{} → {}  inner {} → {}",
                            sim_packets::ipv4_str(i.src),
                            sim_packets::ipv4_str(i.dst),
                            sim_packets::ipv4_str(i.inner.src),
                            sim_packets::ipv4_str(i.inner.dst)
                        ),
                    ),
                    L3::Capwap(c) => (
                        "CAPWAP",
                        format!(
                            "{} → {}  {:?} ssid={} vlan={}",
                            sim_packets::ipv4_str(c.src),
                            sim_packets::ipv4_str(c.dst),
                            c.kind,
                            sim_packets::fixed_str_value(&c.ssid),
                            c.vlan
                        ),
                    ),
                    L3::Vxlan(v) => (
                        "VXLAN",
                        format!(
                            "VNI {}  {} → {}  inner {} → {}",
                            v.vni,
                            sim_packets::ipv4_str(v.src),
                            sim_packets::ipv4_str(v.dst),
                            sim_packets::mac_str(&v.inner_src),
                            sim_packets::mac_str(&v.inner_dst)
                        ),
                    ),
                    L3::Evpn(e) => (
                        "EVPN",
                        format!(
                            "VTEP {} VNI {} VLAN {} → {}",
                            sim_packets::ipv4_str(e.vtep_ip),
                            e.vni,
                            e.vlan,
                            sim_packets::ipv4_str(e.dst)
                        ),
                    ),
                    L3::Bpdu(b) => (
                        "BPDU",
                        format!(
                            "root 优先级={} cost={} 发送者优先级={}",
                            b.root.priority, b.cost, b.sender.priority
                        ),
                    ),
                    L3::Dhcp(d) => (
                        match d.op {
                            sim_packets::DhcpOp::Discover => "DHCP发现",
                            sim_packets::DhcpOp::Offer => "DHCP提供",
                            sim_packets::DhcpOp::Request => "DHCP请求",
                            sim_packets::DhcpOp::Ack => "DHCP确认",
                        },
                        format!(
                            "{} ← {}  提议 {}",
                            sim_packets::mac_str(&d.client_mac),
                            sim_packets::ipv4_str(d.server_ip),
                            sim_packets::ipv4_str(d.offered_ip)
                        ),
                    ),
                    L3::Lacp(l) => (
                        "LACP",
                        format!(
                            "actor 系统={} key={} 端口={} 状态=0x{:02x}",
                            sim_packets::mac_str(&l.actor_system),
                            l.actor_key,
                            l.actor_port,
                            l.actor_state
                        ),
                    ),
                    L3::Ospf(o) => (
                        if matches!(o.kind, sim_packets::OspfKind::Hello) {
                            "OSPF Hello"
                        } else {
                            "OSPF LSU"
                        },
                        format!(
                            "router-id={} area={} 网段={} 邻居={}",
                            sim_packets::ipv4_str(o.router_id),
                            o.area,
                            o.lsa.link_count,
                            o.lsa.neighbor_count
                        ),
                    ),
                    L3::Vrrp(v) => (
                        "VRRP",
                        format!(
                            "vrid={} priority={} vip={}",
                            v.vrid,
                            v.priority,
                            sim_packets::ipv4_str(v.vip)
                        ),
                    ),
                    L3::Ipv6(p) => (
                        match p.icmp {
                            sim_packets::Ipv6Icmp::EchoRequest { .. } => "ICMPv6 Echo Request",
                            sim_packets::Ipv6Icmp::EchoReply { .. } => "ICMPv6 Echo Reply",
                            sim_packets::Ipv6Icmp::NeighborSolicitation { .. } => "NDP NS",
                            sim_packets::Ipv6Icmp::NeighborAdvertisement { .. } => "NDP NA",
                        },
                        format!(
                            "{} → {} hop={}",
                            sim_packets::ipv6_str(&p.src),
                            sim_packets::ipv6_str(&p.dst),
                            p.hop_limit
                        ),
                    ),
                    L3::Ospfv3(p) => (
                        "OSPFv3",
                        format!(
                            "Router-ID {} src={} prefixes={}",
                            sim_packets::ipv4_str(p.router_id),
                            sim_packets::ipv6_str(&p.src),
                            p.count
                        ),
                    ),
                    L3::Srv6(s) => (
                        "SRv6 BE",
                        format!(
                            "{} → {} hop={}",
                            sim_packets::ipv6_str(&s.src),
                            sim_packets::ipv6_str(&s.dst),
                            s.hop_limit
                        ),
                    ),
                    L3::Bgp(b) => (
                        match b.kind {
                            sim_packets::BgpKind::Open => "BGP Open",
                            sim_packets::BgpKind::Keepalive => "BGP Keepalive",
                            sim_packets::BgpKind::Update => "BGP Update",
                        },
                        format!(
                            "AS {} src={} 条目={}",
                            b.asn,
                            sim_packets::ipv4_str(b.src_ip),
                            b.count
                        ),
                    ),
                    L3::Isis(i) => (
                        match i.kind {
                            sim_packets::IsisKind::Hello => "IS-IS Hello",
                            sim_packets::IsisKind::Lsp => "IS-IS LSP",
                        },
                        format!(
                            "sysid={} 网段={} 邻居={}",
                            i.system_id
                                .iter()
                                .map(|b| format!("{:02x}", b))
                                .collect::<Vec<_>>()
                                .join(""),
                            i.lsa.link_count,
                            i.lsa.neighbor_count
                        ),
                    ),
                    L3::Mpls(m) => (
                        "MPLS",
                        format!(
                            "label={} depth={} inner {} → {}",
                            m.labels[0],
                            m.depth,
                            sim_packets::ipv4_str(m.ip.src),
                            sim_packets::ipv4_str(m.ip.dst)
                        ),
                    ),
                };
                serde_json::json!({
                    "seq": c.seq,
                    "ms": c.ns as f64 / 1e6,
                    "dir": c.dir,
                    "srcMac": sim_packets::mac_str(&c.frame.src),
                    "dstMac": sim_packets::mac_str(&c.frame.dst),
                    "kind": kind,
                    "vlan": c.frame.vlan,
                    "info": info,
                })
            })
            .collect();
        let next = l.capture_next;
        Ok(serde_json::json!({ "next": next, "rows": rows }).to_string())
    }
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod stage3a_tests;

#[cfg(test)]
mod stage4a_tests;

#[cfg(test)]
mod stage5a_tests;

#[cfg(test)]
mod stage6a_tests;

#[cfg(test)]
mod stage6b_tests;

#[cfg(test)]
mod c6b_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use cli_engine::{Mode, Session};

    fn two_router_lab() -> Sim {
        let mut sim = Sim::new();
        let a = sim.add_device("Router", "Ios").unwrap();
        let b = sim.add_device("Router", "Vrp").unwrap();
        sim.add_link(a, b).unwrap();
        sim
    }

    /// 把当前存档降级成阶段 0 旧档：去掉 schema_version 与设备 profile。
    fn as_legacy(json: &str) -> String {
        let mut v: serde_json::Value = serde_json::from_str(json).unwrap();
        v.as_object_mut().unwrap().remove("schema_version");
        for d in v["devices"].as_array_mut().unwrap() {
            d.as_object_mut().unwrap().remove("profile");
        }
        v.to_string()
    }

    #[test]
    fn new_archive_has_schema_version_and_profile() {
        let sim = two_router_lab();
        let v: serde_json::Value = serde_json::from_str(&sim.dump()).unwrap();
        assert_eq!(v["schema_version"], SCHEMA_VERSION);
        assert_eq!(v["devices"].as_array().unwrap().len(), 2);
        for d in v["devices"].as_array().unwrap() {
            assert!(d["profile"].is_string(), "新存档必须保存稳定 Profile ID");
        }
    }

    #[test]
    fn legacy_archive_loads_round_trips_and_reports_mapping() {
        let sim = two_router_lab();
        let legacy = as_legacy(&sim.dump());
        assert!(!legacy.contains("schema_version"));
        assert!(!legacy.contains("profile"), "旧档不得含 Profile 字段");

        let mut target = Sim::new();
        let report = target.apply_summary_checked(&legacy).unwrap();
        assert!(report.legacy);
        assert_eq!(report.schema_version, SCHEMA_VERSION);
        assert_eq!(report.mapped_profiles, 2);
        assert!(report
            .notices
            .iter()
            .any(|n| n.contains(LEGACY_PROFILE_NOTICE)));
        assert!(report.notices.iter().any(|n| n.contains(LOAD_RESET_NOTICE)));
        assert!(report
            .notices
            .iter()
            .any(|n| n.contains(profile_mod::IOS_ROUTER_PROFILE)));
        assert!(report
            .notices
            .iter()
            .any(|n| n.contains(profile_mod::VRP_ROUTER_PROFILE)));

        let round: serde_json::Value = serde_json::from_str(&target.dump()).unwrap();
        assert_eq!(round["schema_version"], SCHEMA_VERSION);
        for d in round["devices"].as_array().unwrap() {
            assert!(d["profile"].is_string());
        }
        assert_eq!(round["links"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn successful_load_clears_sessions_and_session_tokens() {
        let mut sim = two_router_lab();
        let dev = 0;
        sim.sessions.insert(
            dev,
            Session {
                mode: Mode::Config,
                ..Default::default()
            },
        );
        sim.session_ids.insert(dev, "stale-token".to_string());
        sim.next_session_seq = 7;
        assert_eq!(sim.sessions.len(), 2);

        let archive = sim.dump();
        sim.apply_summary_checked(&archive).unwrap();
        assert!(sim.sessions.is_empty(), "加载后终端会话必须清空");
        assert!(sim.session_ids.is_empty(), "旧会话令牌不得进入新工作区");
        // 序号不回退：重新签发的令牌与旧令牌不同，旧上下文不会复活。
        assert_eq!(sim.next_session_seq, 7);
        assert_eq!(sim.eng.net.devices.len(), 2);
    }

    #[test]
    fn failed_load_keeps_current_state() {
        let mut sim = two_router_lab();
        let before = sim.dump();
        assert!(sim.apply_summary_checked("{ not json").is_err());
        assert_eq!(sim.dump(), before);
    }

    #[test]
    fn current_schema_archive_is_not_legacy() {
        let sim = two_router_lab();
        let archive = sim.dump();
        let mut target = Sim::new();
        let report = target.apply_summary_checked(&archive).unwrap();
        assert!(!report.legacy);
        assert_eq!(report.mapped_profiles, 0);
        // 即使不是旧档，会话清空提示也必须给出。
        assert!(report.notices.iter().any(|n| n.contains(LOAD_RESET_NOTICE)));
    }

    #[test]
    fn inspect_command_is_platform_level_and_pure() {
        let sim = two_router_lab();
        let before = cli_dialects::state_fingerprint(&sim.eng);
        let json = sim
            .inspect_command(profile_mod::IOS_ROUTER_PROFILE, "Priv", "show ip route")
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["status"], "Executed");
        assert_eq!(v["support"], "pending-verification");
        assert_eq!(v["passed"], false);
        assert!(v["detail"].as_str().unwrap().contains("平台级结果"));
        assert_eq!(cli_dialects::state_fingerprint(&sim.eng), before);
        assert_eq!(sim.eng.now_ns(), 0);
    }
}
