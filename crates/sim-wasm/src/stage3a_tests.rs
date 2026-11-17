//! 阶段 3A 契约测试：纯读 / 主动检查分离、结构化结果、会话令牌与双通道回归。
//!
//! 这里直接使用与 WASM 同一份 `Sim` / `lab-checks` 代码；浏览器验收再对真实编译出的
//! `net_sim_bg.wasm` 走一遍关键流程。

use super::*;
// 会话由 Sim 自身管理，测试不直接构造 Session。
use lab_checks::{CheckBehavior, CheckGroup, CheckKind, LabCheck, ACTIVE_CONVERGENCE_NS};
use sim_core::device::{MgmtProto, RouteSrc};
use sim_core::profile::{IOS_ROUTER_PROFILE, IOS_SWITCH_PROFILE, VRP_ROUTER_PROFILE};

fn ip(s: &str) -> sim_packets::Ipv4Addr {
    sim_packets::ipv4_from_str(s).unwrap()
}

fn v6(s: &str) -> sim_packets::Ipv6Addr {
    sim_packets::ipv6_from_str(s).unwrap()
}

fn fingerprint(sim: &Sim) -> String {
    cli_dialects::state_fingerprint(&sim.eng)
}

/// 含交换机（STP 事件队列非空）与两台主机的最小拓扑。
fn mixed_lab() -> Sim {
    let mut sim = Sim::new();
    let pc1 = sim.add_device("Host", "Ios").unwrap();
    let sw1 = sim.add_device("Switch", "Vrp").unwrap();
    let pc2 = sim.add_device("Host", "Ios").unwrap();
    sim.rename_device(pc1, "PC1").unwrap();
    sim.rename_device(sw1, "SW1").unwrap();
    sim.rename_device(pc2, "PC2").unwrap();
    sim.add_link(pc1, sw1).unwrap();
    sim.add_link(sw1, pc2).unwrap();
    sim.set_host_config(pc1, "192.168.10.11", 24, "").unwrap();
    sim.set_host_config(pc2, "192.168.10.12", 24, "").unwrap();
    sim
}

const MIXED_DEFS: &str = r#"[
  {"label":"VLAN10","kind":{"type":"vlanExists","dev":"SW1","vlan":10}},
  {"label":"Trunk","kind":{"type":"trunkAllows","dev":"SW1","port":0,"vlan":10}},
  {"label":"正向 Ping","kind":{"type":"ping","from":"PC1","to":"192.168.10.12"}},
  {"label":"负向 Ping","kind":{"type":"ping","from":"PC1","to":"192.168.10.99","expect":false}},
  {"label":"TCP","kind":{"type":"tcpProbe","from":"PC1","to":"192.168.10.12","port":80}},
  {"label":"登录","kind":{"type":"login","from":"PC1","to":"192.168.10.1","proto":"ssh","user":"admin","pass":"x"}},
  {"label":"路由存在","kind":{"type":"routeExists","dev":"SW1","net":"192.168.10.0/24"}},
  {"label":"路由缺失","kind":{"type":"routeAbsent","dev":"SW1","net":"172.16.9.0/24"}}
]"#;

fn parse_results(raw: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<Vec<serde_json::Value>>(raw).unwrap()
}

#[test]
fn stage3a_check_behavior_matrix_is_explicit() {
    use CheckBehavior::{ActiveExperiment, ReadOnly};
    let a = ip("10.0.0.1");
    let b = v6("2001:db8::1");
    let kinds: Vec<(&str, CheckKind, CheckBehavior)> = vec![
        (
            "Ping",
            CheckKind::Ping {
                from: 0,
                to: a,
                expect: true,
            },
            ActiveExperiment,
        ),
        (
            "Ping6",
            CheckKind::Ping6 {
                from: 0,
                to: b,
                expect: true,
            },
            ActiveExperiment,
        ),
        (
            "TcpProbe",
            CheckKind::TcpProbe {
                from: 0,
                to: a,
                port: 80,
                expect: false,
            },
            ActiveExperiment,
        ),
        (
            "LoginAttempt",
            CheckKind::LoginAttempt {
                from: 0,
                to: a,
                proto: MgmtProto::Telnet,
                user: "admin".into(),
                pass: "x".into(),
                expect: false,
            },
            ActiveExperiment,
        ),
        (
            "VlanExists",
            CheckKind::VlanExists { dev: 0, vlan: 10 },
            ReadOnly,
        ),
        (
            "AccessVlan",
            CheckKind::AccessVlan {
                dev: 0,
                port: 0,
                vlan: 10,
            },
            ReadOnly,
        ),
        (
            "TrunkAllows",
            CheckKind::TrunkAllows {
                dev: 0,
                port: 0,
                vlan: 10,
            },
            ReadOnly,
        ),
        (
            "SviIp",
            CheckKind::SviIp {
                dev: 0,
                vlan: 10,
                ip: a,
            },
            ReadOnly,
        ),
        (
            "PortIp",
            CheckKind::PortIp {
                dev: 0,
                port: 0,
                ip: a,
            },
            ReadOnly,
        ),
        (
            "RouteExists (BGP)",
            CheckKind::RouteExists {
                dev: 0,
                dst: a,
                len: 24,
                src: Some(RouteSrc::Bgp),
            },
            ReadOnly,
        ),
        ("RootBridge", CheckKind::RootBridge { dev: 0 }, ReadOnly),
        (
            "PortRole",
            CheckKind::PortRole {
                dev: 0,
                port: 0,
                role: sim_core::device::StpRole::Root,
            },
            ReadOnly,
        ),
        (
            "LagSelected",
            CheckKind::LagSelected {
                dev: 0,
                group: 1,
                count: 2,
            },
            ReadOnly,
        ),
        (
            "MgmtUser",
            CheckKind::MgmtUser {
                dev: 0,
                name: "admin".into(),
            },
            ReadOnly,
        ),
        (
            "VtyTransport",
            CheckKind::VtyTransport {
                dev: 0,
                transport: sim_core::device::VtyTransport::Ssh,
            },
            ReadOnly,
        ),
        (
            "OspfNeighbors",
            CheckKind::OspfNeighbors { dev: 0, min: 1 },
            ReadOnly,
        ),
    ];
    for (name, kind, expected) in &kinds {
        assert_eq!(kind.behavior(), *expected, "{name} 行为分类错误");
        assert_eq!(kind.is_active(), *expected == ActiveExperiment, "{name}");
    }
    assert_eq!(
        kinds
            .iter()
            .filter(|(_, _, b)| *b == ActiveExperiment)
            .count(),
        4,
        "主动探测应恰好四类"
    );
    // 负向探测同样属于主动行为；组合项按内部最主动的一项分类。
    let neg = CheckGroup::Single(CheckKind::Ping {
        from: 0,
        to: a,
        expect: false,
    });
    assert_eq!(neg.behavior(), ActiveExperiment);
    let either = CheckGroup::Either(
        CheckKind::RootBridge { dev: 0 },
        CheckKind::Ping {
            from: 0,
            to: a,
            expect: true,
        },
    );
    assert_eq!(either.behavior(), ActiveExperiment);
    assert_eq!(
        CheckGroup::Negate(CheckKind::VlanExists { dev: 0, vlan: 10 }).behavior(),
        ReadOnly
    );
    assert_eq!(ACTIVE_CONVERGENCE_NS, 45_000_000_000);
}

#[test]
fn stage3a_read_only_batch_keeps_state_time_and_protocol_stats_unchanged() {
    let sim = mixed_lab();
    let before = fingerprint(&sim);
    let now = sim.eng.now_ns();
    let stats = sim.eng.stats();
    // 队列里确实有 STP 事件，纯读若推进协议就会被发现。
    assert!(!sim.eng.net.devices.is_empty());

    let results = parse_results(&sim.read_only_lab_checks(MIXED_DEFS).unwrap());
    assert_eq!(results.len(), 8);
    assert_eq!(fingerprint(&sim), before, "纯读不得改变设备状态指纹");
    assert_eq!(sim.eng.now_ns(), now, "纯读不得推进虚拟时间");
    assert_eq!(
        (
            sim.eng.stats().virtual_ns,
            sim.eng.stats().events,
            sim.eng.stats().in_flight,
            sim.eng.stats().dropped
        ),
        (
            stats.virtual_ns,
            stats.events,
            stats.in_flight,
            stats.dropped
        ),
        "纯读不得改变协议/队列统计"
    );

    // 主动项全部 skipped，不计通过也不计失败（passed=false 仅表示未执行）。
    for r in results
        .iter()
        .filter(|r| r["behavior"] == "activeExperiment")
    {
        assert_eq!(r["skipped"], true);
        assert_eq!(r["passed"], false);
        assert!(r["detail"].as_str().unwrap().contains("纯读检查不执行"));
    }
    assert_eq!(
        results
            .iter()
            .filter(|r| r["behavior"] == "activeExperiment")
            .count(),
        4
    );
    // 纯读项仍给出真实状态结果（此处未配置 VLAN，故为 false）。
    let vlan = results.iter().find(|r| r["label"] == "VLAN10").unwrap();
    assert_eq!(vlan["skipped"], false);
    assert_eq!(vlan["behavior"], "readOnly");
    assert_eq!(vlan["passed"], false);
    assert!(vlan["detail"].as_str().unwrap().contains("VLAN10 未创建"));
    // 负向路由组合（routeAbsent）在纯读路径也必须执行，且语义为取反。
    let absent = results.iter().find(|r| r["label"] == "路由缺失").unwrap();
    assert_eq!(absent["skipped"], false);
    assert_eq!(absent["passed"], true);
}

#[test]
fn stage3a_active_batch_converges_and_runs_probes_with_behavior_metadata() {
    let mut sim = mixed_lab();
    let before = fingerprint(&sim);
    let now = sim.eng.now_ns();
    let events = sim.eng.stats().events;

    let results = parse_results(&sim.run_lab_checks(MIXED_DEFS).unwrap());
    assert_eq!(results.len(), 8);
    assert_ne!(fingerprint(&sim), before, "主动判分应产生真实状态/统计变化");
    assert!(sim.eng.now_ns() > now, "主动判分应推进虚拟时间");
    assert!(
        sim.eng.stats().events > events,
        "主动判分应处理协议事件（一次批量收敛）"
    );
    assert!(
        results.iter().all(|r| r["skipped"] == false),
        "主动入口不得跳过任何项"
    );
    for r in &results {
        assert!(!r["detail"].as_str().unwrap().is_empty());
        assert!(
            r["behavior"] == "readOnly" || r["behavior"] == "activeExperiment",
            "行为元数据必须来自两类分类"
        );
    }
    let pos = results.iter().find(|r| r["label"] == "正向 Ping").unwrap();
    assert_eq!(pos["behavior"], "activeExperiment");
    let neg = results.iter().find(|r| r["label"] == "负向 Ping").unwrap();
    assert_eq!(neg["behavior"], "activeExperiment", "负向探测也是主动行为");
    // 两主机同网段、同 VLAN1，正向应通；不存在的目标应不通（负向断言通过）。
    assert_eq!(pos["passed"], true, "{}", pos["detail"]);
    assert_eq!(neg["passed"], true, "{}", neg["detail"]);
}

#[test]
fn stage3a_read_only_and_active_share_one_judgement() {
    let sim = mixed_lab();
    let labs = vec![
        LabCheck::single("VLAN10", CheckKind::VlanExists { dev: 1, vlan: 10 }),
        LabCheck::single(
            "Trunk",
            CheckKind::TrunkAllows {
                dev: 1,
                port: 0,
                vlan: 10,
            },
        ),
    ];
    let read = lab_checks::run_read_only_checks(&sim.eng, &labs);
    let mut active_sim = mixed_lab();
    let active = lab_checks::run_active_checks(&mut active_sim.eng, &labs);
    for (r, a) in read.iter().zip(active.iter()) {
        assert_eq!(r.label, a.label);
        assert_eq!(r.passed, a.passed, "{} 不应出现两套判断语义", r.label);
        assert_eq!(r.detail, a.detail, "{} detail 必须同源", r.label);
        assert_eq!(r.behavior, CheckBehavior::ReadOnly);
    }
    // 旧兼容入口仍保持「先收敛再判分」语义。
    let mut compat = mixed_lab();
    let checks = vec![lab_checks::Check {
        label: "VLAN10".into(),
        kind: CheckKind::VlanExists { dev: 1, vlan: 10 },
    }];
    let now = compat.eng.now_ns();
    let legacy = lab_checks::run_checks(&mut compat.eng, &checks);
    assert!(compat.eng.now_ns() > now, "run_checks 兼容入口仍应收敛");
    assert_eq!(legacy[0].passed, read[0].passed);
    assert_eq!(legacy[0].detail, read[0].detail);
}

#[test]
fn stage3a_legacy_check_wrappers_split_side_effects() {
    let mut sim = mixed_lab();
    let before = fingerprint(&sim);
    let now = sim.eng.now_ns();
    // 纯状态 check_* 不再隐藏推进副作用。
    let _ = sim.check_vlan(1, 10).unwrap();
    let _ = sim.check_trunk_allows(1, 0, 10).unwrap();
    let _ = sim.check_svi_ip(1, 10, "192.168.10.1").unwrap();
    let _ = sim.check_route(1, "192.168.10.0/24").unwrap();
    let _ = sim.check_route_absent(1, "172.16.9.0/24").unwrap();
    assert_eq!(fingerprint(&sim), before, "纯状态 check_* 必须纯读");
    assert_eq!(sim.eng.now_ns(), now);
    // 主动 check_* 仍按旧契约推进并真实发包。
    let _ = sim.check_ping(0, "192.168.10.12").unwrap();
    assert!(sim.eng.now_ns() > now, "check_ping 必须真实推进探测");
    // 旧 boolean verify_command 保留兼容。
    assert!(sim.verify_command("Ios", "Exec", "show ip route"));
    assert!(!sim.verify_command("Ios", "Exec", "not-a-command"));
}

#[test]
fn stage3a_inspect_and_verify_device_command_are_pure_and_session_scoped() {
    let mut sim = Sim::new();
    let dev = sim.add_device("Router", "Vrp").unwrap();
    sim.rename_device(dev, "R1").unwrap();
    let token = sim.session_id(dev).unwrap();
    assert!(!token.is_empty());

    let before = fingerprint(&sim);
    let now = sim.eng.now_ns();
    let stats = sim.eng.stats();

    let profile: serde_json::Value = serde_json::from_str(
        &sim.inspect_command(VRP_ROUTER_PROFILE, "Exec", "display ip routing-table")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(profile["status"], "Executed");
    assert!(profile["detail"].as_str().unwrap().contains("平台级结果"));

    // 走同一份纯逻辑入口；WASM 包装只负责把 String 错误映射成 JsValue。
    let device = sim
        .verify_device_command_checked(dev, &token, "display ip routing-table")
        .unwrap();
    assert_eq!(device.status.label(), "Executed");
    let detail = &device.detail;
    assert!(detail.contains("profile=vrp-generic-router"), "{detail}");
    assert!(detail.contains("mode=exec"), "{detail}");
    assert!(detail.contains("authenticated=true"), "{detail}");
    assert_eq!(fingerprint(&sim), before, "设备会话级检查必须纯读");
    assert_eq!(sim.eng.now_ns(), now);
    assert_eq!(
        (
            sim.eng.stats().virtual_ns,
            sim.eng.stats().events,
            sim.eng.stats().in_flight,
            sim.eng.stats().dropped
        ),
        (
            stats.virtual_ns,
            stats.events,
            stats.in_flight,
            stats.dropped
        )
    );

    // 错误/过期令牌是会话错误（明确字符串），不得伪装成结构化「设备不支持」。
    let err = sim
        .verify_device_command_checked(dev, "stale-token", "display ip routing-table")
        .unwrap_err();
    assert_eq!(err, "session id mismatch");
    assert_eq!(fingerprint(&sim), before, "错误令牌不得改变引擎状态");

    // 当前会话模式参与真实检查：Exec 下的配置命令是命令错误，进入系统视图后有效。
    let exec_only = sim
        .verify_device_command_checked(dev, &token, "sysname R1")
        .unwrap();
    assert_eq!(exec_only.support.label(), "invalid-command");
    assert!(
        exec_only.detail.contains("mode=exec"),
        "{}",
        exec_only.detail
    );
    sim.exec_command_result(dev, "system-view").unwrap();
    let cfg = sim
        .verify_device_command_checked(dev, &token, "sysname R1")
        .unwrap();
    assert_ne!(cfg.support.label(), "invalid-command");
    assert!(cfg.detail.contains("mode=config"), "{}", cfg.detail);

    // 加载/重置后旧令牌失效；错误仍是会话级而非能力级。
    let archive = sim.dump();
    assert!(sim.load(&archive).is_ok());
    assert_eq!(
        sim.verify_device_command_checked(dev, &token, "display ip routing-table")
            .unwrap_err(),
        "no active device session"
    );
    let token2 = sim.session_id(dev).unwrap();
    assert_ne!(token, token2, "加载后必须重新签发会话令牌");
    sim.reset();
    assert_eq!(
        sim.verify_device_command_checked(dev, &token2, "display ip routing-table")
            .unwrap_err(),
        "no active device session"
    );
}

#[test]
fn stage3a_profile_selection_changes_structured_support() {
    let sim = Sim::new();
    let router: serde_json::Value = serde_json::from_str(
        &sim.inspect_command(IOS_ROUTER_PROFILE, "If", "switchport access vlan 10")
            .unwrap(),
    )
    .unwrap();
    let switch: serde_json::Value = serde_json::from_str(
        &sim.inspect_command(IOS_SWITCH_PROFILE, "If", "switchport access vlan 10")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(router["support"], "unsupported-by-device");
    assert_ne!(switch["support"], "unsupported-by-device");
    assert!(!router["passed"].as_bool().unwrap());
    // 阶段 4A 分层矩阵（替代阶段 3A 的「Comware 全量不得完整支持」）：
    // 首批已闭环且有 H3C 官方证据的原生命令 → 完整支持；
    // security 等未收口主题 → 仍不得完整支持，也不得变成命令错误。
    let comware: serde_json::Value = serde_json::from_str(
        &sim.inspect_command("comware-generic-l3-switch", "If", "port access vlan 10")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(comware["support"], "full-support");
    assert!(!comware["detail"].as_str().unwrap().is_empty());
    let cw_security: serde_json::Value = serde_json::from_str(
        &sim.inspect_command(
            "comware-generic-router",
            "Config",
            "security-zone name trust",
        )
        .unwrap(),
    )
    .unwrap();
    assert_ne!(cw_security["support"], "full-support");
    assert_ne!(cw_security["support"], "invalid-command");
    // VRP 专属写法在 Comware 上必须被独立树拒绝。
    let cw_vrp_form: serde_json::Value = serde_json::from_str(
        &sim.inspect_command("comware-generic-router", "Config", "vlan batch 10 20")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(cw_vrp_form["support"], "invalid-command");
}

#[test]
fn stage3a_dual_channel_and_legacy_text_api_regression() {
    let mut sim = Sim::new();
    let dev = sim.add_device("Router", "Ios").unwrap();
    let structured: serde_json::Value =
        serde_json::from_str(&sim.exec_command_result(dev, "show ip route").unwrap()).unwrap();
    assert_eq!(structured["status"], "Executed");
    let out = structured["deviceOutput"].as_str().unwrap();
    assert!(cli_dialects::scan_simulator_wording(out).is_empty());
    assert!(!structured["simulatorNotices"]
        .as_array()
        .unwrap()
        .is_empty());
    // 旧文本 API 仍同时含设备文本与 [netemu] 提示，不丢设备输出。
    let text = sim.exec_command(dev, "show ip route").unwrap();
    assert!(text.contains("[netemu]"));
    assert!(!text.is_empty());
    assert!(sim.verify_command("Ios", "Exec", "show ip route"));
}

#[test]
fn stage3a_lab_defs_protocol_covers_every_declared_type() {
    let sim = mixed_lab();
    let defs = r#"[
      {"label":"a","kind":{"type":"ping","from":"PC1","to":"192.168.10.12"}},
      {"label":"b","kind":{"type":"ping6","from":"PC1","to":"2001:db8::1"}},
      {"label":"c","kind":{"type":"tcpProbe","from":"PC1","to":"192.168.10.12","port":80}},
      {"label":"d","kind":{"type":"login","from":"PC1","to":"192.168.10.1","proto":"ssh","user":"u","pass":"p"}},
      {"label":"e","kind":{"type":"sviIp","dev":"SW1","vlan":10,"ip":"192.168.10.1"}},
      {"label":"f","kind":{"type":"accessVlan","dev":"SW1","port":0,"vlan":10}},
      {"label":"g","kind":{"type":"trunkAllows","dev":"SW1","port":0,"vlan":10}},
      {"label":"h","kind":{"type":"vlanExists","dev":"SW1","vlan":10}},
      {"label":"i","kind":{"type":"staticRoute","dev":"SW1","net":"10.0.0.0/24"}},
      {"label":"j","kind":{"type":"portIp","dev":"PC1","port":0,"ip":"192.168.10.11"}},
      {"label":"k","kind":{"type":"routeExists","dev":"SW1","net":"10.0.0.0/24"}},
      {"label":"l","kind":{"type":"routeAbsent","dev":"SW1","net":"10.0.0.0/24"}},
      {"label":"m","kind":{"type":"mplsLsp","dev":"SW1","name":"LSP"}},
      {"label":"n","kind":{"type":"vrrpMaster","dev":"SW1","vrid":1}},
      {"label":"o","kind":{"type":"ospfNeighbors","dev":"SW1","min":1}},
      {"label":"p","kind":{"type":"ospfv3Neighbors","dev":"SW1","min":1}},
      {"label":"q","kind":{"type":"securityZone","dev":"SW1","port":0,"zone":"trust"}},
      {"label":"r","kind":{"type":"securityPolicy","dev":"SW1","name":"P","srcZone":"trust","dstZone":"untrust","permit":true}},
      {"label":"s","kind":{"type":"ipsecTunnel","dev":"SW1","name":"T"}},
      {"label":"t","kind":{"type":"vpnConnected","dev":"PC1"}},
      {"label":"u","kind":{"type":"wlanApJoined","ac":"SW1","ap":"PC2"}},
      {"label":"v","kind":{"type":"wlanSsid","ac":"SW1","ssid":"CORP"}},
      {"label":"w","kind":{"type":"wifiConnected","dev":"PC1"}},
      {"label":"x","kind":{"type":"wifiOnAp","dev":"PC1","ap":"PC2"}},
      {"label":"y","kind":{"type":"vxlanVni","dev":"SW1","vlan":10,"vni":10000}},
      {"label":"z","kind":{"type":"stackMember","dev":"SW1","domain":10,"member":1}},
      {"label":"aa","kind":{"type":"mlagDomain","dev":"SW1","domain":1}},
      {"label":"ab","kind":{"type":"evpnRoute","dev":"SW1","vtep":"10.0.0.1","vni":10000}},
      {"label":"ac","kind":{"type":"ltmVirtual","dev":"SW1","name":"VS","vip":"10.0.0.1","port":80}},
      {"label":"ad","kind":{"type":"portIsolate","dev":"SW1","port":0,"group":1}},
      {"label":"ae","kind":{"type":"muxRole","dev":"SW1","port":0,"role":"principal"}},
      {"label":"af","kind":{"type":"srv6Route","dev":"SW1","dst":"2001:db8:2::/64"}},
      {"label":"ag","kind":{"type":"rootBridge","dev":"SW1"}},
      {"label":"ah","kind":{"type":"portRole","dev":"SW1","port":0,"role":"Root"}},
      {"label":"ai","kind":{"type":"portRoleEither","dev":"SW1","portA":0,"roleA":"Root","portB":1,"roleB":"Root"}},
      {"label":"aj","kind":{"type":"lagMember","dev":"SW1","group":1,"port":0}},
      {"label":"ak","kind":{"type":"lagSelected","dev":"SW1","group":1,"count":2}},
      {"label":"al","kind":{"type":"mgmtUser","dev":"SW1","name":"admin"}},
      {"label":"am","kind":{"type":"vtyTransport","dev":"SW1","transport":"ssh"}}
    ]"#;
    let parsed = lab_defs::parse_lab_checks(&sim.eng, defs).unwrap();
    assert_eq!(parsed.len(), 39, "Web 声明的每种检查类型都必须可解析");
    assert!(lab_defs::parse_lab_checks(
        &sim.eng,
        r#"[{"label":"x","kind":{"type":"unknown","dev":"SW1"}}]"#
    )
    .is_err());
    assert!(lab_defs::parse_lab_checks(
        &sim.eng,
        r#"[{"label":"x","kind":{"type":"ping","from":"NO_SUCH_DEV","to":"192.168.10.12"}}]"#
    )
    .is_err());
    // 解析本身不得改动引擎状态。
    assert_eq!(sim.eng.now_ns(), 0);
}

#[test]
fn stage3a_five_representative_lab_defs_parse_and_classify() {
    let mut sim = Sim::new();
    for (kind, dialect, name) in [
        ("Switch", "Vrp", "SW1"),
        ("Switch", "Comware", "SW2"),
        ("Router", "Ios", "R1"),
        ("Router", "Vrp", "R2"),
        ("Router", "Vrp", "R3"),
        ("Host", "Ios", "PC1"),
        ("Host", "Ios", "PC2"),
        ("Host", "Ios", "SRV"),
    ] {
        let id = sim.add_device(kind, dialect).unwrap();
        sim.rename_device(id, name).unwrap();
    }
    let labs: Vec<(&str, &str)> = vec![
        (
            "office-l2",
            r#"[{"label":"ping+","kind":{"type":"ping","from":"PC1","to":"192.168.10.12"}},{"label":"ping-","kind":{"type":"ping","from":"PC1","to":"192.168.10.13","expect":false}},{"label":"trunk","kind":{"type":"trunkAllows","dev":"SW1","port":1,"vlan":10}}]"#,
        ),
        (
            "office-l3",
            r#"[{"label":"svi","kind":{"type":"sviIp","dev":"SW2","vlan":10,"ip":"192.168.10.1"}},{"label":"ping","kind":{"type":"ping","from":"PC1","to":"192.168.20.12"}}]"#,
        ),
        (
            "enterprise",
            r#"[{"label":"dhcp","kind":{"type":"portIp","dev":"PC1","port":0,"ip":"192.168.1.2"}},{"label":"ping","kind":{"type":"ping","from":"PC1","to":"202.100.1.100"}},{"label":"route","kind":{"type":"routeExists","dev":"R1","net":"202.100.1.0/24"}}]"#,
        ),
        (
            "stp-redundancy",
            r#"[{"label":"root","kind":{"type":"rootBridge","dev":"SW1"}},{"label":"role","kind":{"type":"portRoleEither","dev":"SW1","portA":1,"roleA":"Root","portB":2,"roleB":"Root"}},{"label":"ping","kind":{"type":"ping","from":"PC1","to":"192.168.10.12"}}]"#,
        ),
        (
            "bgp-route-policy",
            r#"[{"label":"allowed","kind":{"type":"routeExists","dev":"R2","net":"192.168.1.0/24"}},{"label":"filtered","kind":{"type":"routeAbsent","dev":"R2","net":"172.16.9.0/24"}},{"label":"ping+","kind":{"type":"ping","from":"PC2","to":"192.168.1.10"}},{"label":"ping-","kind":{"type":"ping","from":"PC2","to":"172.16.9.10","expect":false}}]"#,
        ),
    ];
    for (name, defs) in labs {
        let parsed = lab_defs::parse_lab_checks(&sim.eng, defs).unwrap();
        assert!(
            parsed
                .iter()
                .any(|c| c.behavior() == CheckBehavior::ActiveExperiment),
            "{name} 必须含主动项"
        );
        assert!(
            parsed
                .iter()
                .any(|c| c.behavior() == CheckBehavior::ReadOnly),
            "{name} 必须含纯读项"
        );
        let results = lab_checks::run_read_only_checks(&sim.eng, &parsed);
        assert_eq!(results.len(), parsed.len());
        assert!(
            results.iter().any(|r| r.skipped),
            "{name} 纯读路径必须跳过主动项"
        );
    }
}
