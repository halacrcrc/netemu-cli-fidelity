//! C6b 工作项 B：管理面登录与数据面一致接地（决策 D1）的正/负用例。
//!
//! 决策与现状矩阵见 `docs/cli-fidelity/c6b-implementation-record.md` §2–§3。
//! 每项断言都对应一条数据面事实：
//! - 对端接口 `shutdown` / 链路 down / 无路由 / 转发路径防火墙 drop → 登录必须被拒绝，
//!   且与同一拓扑上 `pc tcp <IP> 22` 的「无响应」结论一致；
//! - 到设备自身的管理流量不因 zone-pair 被 drop（官方 self 例外，6B 语义不回退）；
//! - 认证失败与 VTY 传输限制仍走原有错误分类（既有 `mgmt_golden` 保留）。
//!
//! 判定不发送真实报文、不推进虚拟时间、不伪造会话（登录按新流判定，见 engine
//! `mgmt_path_open` 注释）。

use cli_dialects::{exec_and_run, prompt};
use cli_engine::Session;
use sim_core::device::{DeviceKind, DeviceState, Dialect, MgmtProto};
use sim_core::engine::Engine;
use sim_core::Net;
use sim_packets::ipv4_from_str;

fn ip(s: &str) -> u32 {
    ipv4_from_str(s).unwrap()
}

/// 与实验六（lab6）一致的拓扑：PC1 192.168.1.10/24（网关 .1）—— R1 g0/0 192.168.1.1/24。
fn lab6() -> (Engine, u32, u32) {
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Ios);
    net.add_link(pc, r1).unwrap();
    net.set_host(pc, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    (Engine::new(net), pc, r1)
}

fn run(eng: &mut Engine, sess: &mut Session, dev: u32, lines: &[&str]) {
    for line in lines {
        let out = exec_and_run(eng, sess, dev, line);
        assert!(
            !out.contains("% Invalid") && !out.contains("Error:") && !out.contains("Incomplete"),
            "配置命令 {line:?} 意外报错: {out}"
        );
    }
}

fn ios_addr(eng: &mut Engine, dev: u32, port: &str, addr: &str) {
    let mut s = Session::default();
    run(eng, &mut s, dev, &["enable", "configure terminal"]);
    run(eng, &mut s, dev, &[&format!("interface {port}")]);
    run(eng, &mut s, dev, &[&format!("ip address {addr}")]);
    run(eng, &mut s, dev, &["no shutdown", "exit", "exit"]);
}

/// Lab 6 的管理面配置：本地用户 + VTY 只允许 SSH。
fn ios_mgmt(eng: &mut Engine, dev: u32) {
    let mut s = Session::default();
    run(eng, &mut s, dev, &["enable", "configure terminal"]);
    run(
        eng,
        &mut s,
        dev,
        &["username admin privilege 15 secret Cisco@123"],
    );
    run(
        eng,
        &mut s,
        dev,
        &[
            "line vty 0 4",
            "login local",
            "transport input ssh",
            "exit",
            "exit",
        ],
    );
}

fn lab6_ready() -> (Engine, u32, u32) {
    let (mut eng, pc, r1) = lab6();
    ios_addr(&mut eng, r1, "g0/0", "192.168.1.1 255.255.255.0");
    ios_mgmt(&mut eng, r1);
    (eng, pc, r1)
}

/// 从主机终端尝试 SSH 登录，返回（首条输出, 口令后的输出, 结束提示符）。
fn ssh_attempt(eng: &mut Engine, pc: u32, target: &str) -> (String, String, String) {
    let mut s = Session::default();
    let first = exec_and_run(eng, &mut s, pc, &format!("ssh {target}"));
    let mut last = String::new();
    if first.contains("Trying") || first.contains("Connecting") {
        let _ = exec_and_run(eng, &mut s, pc, "admin");
        last = exec_and_run(eng, &mut s, pc, "Cisco@123");
    }
    let p = prompt(eng, &s, pc);
    (first, last, p)
}

fn tcp_probe_text(eng: &mut Engine, pc: u32, target: &str, port: u16) -> String {
    let mut s = Session::default();
    exec_and_run(eng, &mut s, pc, &format!("tcp {target} {port}"))
}

fn ping_received(eng: &mut Engine, from: u32, target: &str) -> u32 {
    let mut s = Session::default();
    let out = exec_and_run(eng, &mut s, from, &format!("ping {target}"));
    if out.contains("5 received") {
        5
    } else if out.contains("0 received") {
        0
    } else {
        panic!("无法解析 ping 结果: {out}")
    }
}

/// 数据面不通时的统一断言：登录被拒绝、提示符回到本机、错误只在设备输出通道。
fn assert_refused(out: &str, p: &str, ctx: &str) {
    assert!(
        out.contains("Network is unreachable"),
        "{ctx}: 应报网络不可达: {out:?}"
    );
    assert_eq!(p, "PC1>", "{ctx}: 被拒绝后应回到本机提示符");
    assert!(
        !out.contains("[netemu]"),
        "{ctx}: 模拟器提示不得混入设备输出"
    );
    assert!(!out.contains("% Invalid"), "{ctx}: 不得退化成语法错误");
}

// ---------------------------------------------------------------- 负向矩阵（D1 决策输入 → 落地断言）

/// 四种数据面故障必须同时让 `ping` / `tcp` / `ssh` 失败，且三者结论一致。
#[test]
fn c6b_login_denied_by_interface_shutdown() {
    let (mut eng, pc, r1) = lab6_ready();
    // 改前实测：ping 0/5、tcp 无响应，而 ssh 仍能登录成功（G1 漏洞 a）。
    let mut s = Session::default();
    run(&mut eng, &mut s, r1, &["enable", "configure terminal"]);
    run(&mut eng, &mut s, r1, &["interface g0/0", "shutdown"]);

    assert_eq!(ping_received(&mut eng, pc, "192.168.1.1"), 0);
    assert!(tcp_probe_text(&mut eng, pc, "192.168.1.1", 22).contains("无响应"));
    let (out, _, p) = ssh_attempt(&mut eng, pc, "192.168.1.1");
    assert_refused(&out, &p, "接口 shutdown");

    // 恢复接口后必须立刻恢复登录能力（判定不缓存结论）。
    run(&mut eng, &mut s, r1, &["no shutdown"]);
    assert_eq!(ping_received(&mut eng, pc, "192.168.1.1"), 5);
    let (out, last, p) = ssh_attempt(&mut eng, pc, "192.168.1.1");
    assert!(out.contains("Trying"), "恢复后应进入登录流程: {out:?}");
    assert_eq!(last, "");
    assert_eq!(p, "R1#");
}

#[test]
fn c6b_login_denied_by_link_down() {
    let (mut eng, pc, _r1) = lab6_ready();
    let link = eng.net.links[0].id;
    eng.net.link_mut(link).unwrap().up = false;

    assert_eq!(ping_received(&mut eng, pc, "192.168.1.1"), 0);
    assert!(tcp_probe_text(&mut eng, pc, "192.168.1.1", 22).contains("无响应"));
    let (out, _, p) = ssh_attempt(&mut eng, pc, "192.168.1.1");
    assert_refused(&out, &p, "链路 down");

    eng.net.link_mut(link).unwrap().up = true;
    let (out, last, p) = ssh_attempt(&mut eng, pc, "192.168.1.1");
    assert!(out.contains("Trying"), "{out:?}");
    assert_eq!(last, "");
    assert_eq!(p, "R1#");
}

#[test]
fn c6b_login_denied_without_route() {
    // PC1 — R1 — R2：R1 的两端不同网段且无路由，管到 R2 的 192.168.2.2 不可达。
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Ios);
    let r2 = net.add_device("R2".into(), DeviceKind::Router, Dialect::Ios);
    net.add_link(pc, r1).unwrap();
    net.add_link(r1, r2).unwrap();
    net.set_host(pc, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    let mut eng = Engine::new(net);
    ios_addr(&mut eng, r1, "g0/0", "192.168.1.1 255.255.255.0");
    ios_addr(&mut eng, r1, "g0/1", "192.168.3.1 255.255.255.0");
    ios_addr(&mut eng, r2, "g0/0", "192.168.2.2 255.255.255.0");
    ios_mgmt(&mut eng, r2);

    assert_eq!(ping_received(&mut eng, pc, "192.168.2.2"), 0);
    assert!(tcp_probe_text(&mut eng, pc, "192.168.2.2", 22).contains("无响应"));
    let (out, _, p) = ssh_attempt(&mut eng, pc, "192.168.2.2");
    assert_refused(&out, &p, "无路由");

    // 补一条静态路由后必须放行：证明拒绝来自路由缺失而不是硬编码。
    let mut s = Session::default();
    run(&mut eng, &mut s, r1, &["enable", "configure terminal"]);
    run(
        &mut eng,
        &mut s,
        r1,
        &["ip route 192.168.2.0 255.255.255.0 192.168.3.2"],
    );
    let (out, last, p) = ssh_attempt(&mut eng, pc, "192.168.2.2");
    assert!(out.contains("Trying"), "补路由后应进入登录流程: {out:?}");
    assert_eq!(last, "");
    assert_eq!(p, "R2#");
}

/// 正向基线：登录成功路径不能因为接地被误伤。
#[test]
fn c6b_login_succeeds_on_healthy_path() {
    let (mut eng, pc, _r1) = lab6_ready();
    assert_eq!(ping_received(&mut eng, pc, "192.168.1.1"), 5);
    // 路由器对到达自身的管理端口回 RST（不模拟 SSH 协议栈），因此 tcp 探测的
    // 「有响应」才是与登录一致的可达性信号。
    assert!(tcp_probe_text(&mut eng, pc, "192.168.1.1", 22).contains("RST"));
    let (out, last, p) = ssh_attempt(&mut eng, pc, "192.168.1.1");
    assert!(out.contains("Trying"), "{out:?}");
    assert_eq!(last, "");
    assert_eq!(p, "R1#");
}

// ---------------------------------------------------------------- 防火墙（转发 vs self）

/// 到设备自身的管理流量不因 zone-pair 被 drop（6B self 例外不回退）。
#[test]
fn c6b_self_management_traffic_ignores_zone_pair_drop() {
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let fw = net.add_device("FW1".into(), DeviceKind::Firewall, Dialect::Ios);
    net.add_link(pc, fw).unwrap();
    net.set_host(pc, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    let mut eng = Engine::new(net);
    let mut s = Session::default();
    run(&mut eng, &mut s, fw, &["enable", "configure terminal"]);
    run(&mut eng, &mut s, fw, &["interface g0/0"]);
    run(
        &mut eng,
        &mut s,
        fw,
        &["ip address 192.168.1.1 255.255.255.0", "no shutdown"],
    );
    run(
        &mut eng,
        &mut s,
        fw,
        &["zone-member security INSIDE", "exit"],
    );
    run(
        &mut eng,
        &mut s,
        fw,
        &[
            "class-map type inspect match-any TRAFFIC",
            "match protocol tcp",
            "exit",
            "policy-map type inspect DROPALL",
            "class type inspect TRAFFIC",
            "drop",
            "exit",
            "exit",
        ],
    );
    // 显式拒绝「进入自身」的 zone-pair：真实公式里这是 self 区域策略。
    run(
        &mut eng,
        &mut s,
        fw,
        &[
            "zone-pair security IN-SELF source INSIDE destination self",
            "service-policy type inspect DROPALL",
            "exit",
        ],
    );
    run(
        &mut eng,
        &mut s,
        fw,
        &[
            "username admin privilege 15 secret Cisco@123",
            "line vty 0 4",
            "login local",
            "transport input ssh",
            "exit",
            "end",
        ],
    );

    let (out, last, p) = ssh_attempt(&mut eng, pc, "192.168.1.1");
    assert!(
        out.contains("Trying"),
        "self 例外：到设备自身的管理流量不得被 zone-pair 拒绝: {out:?}"
    );
    assert_eq!(last, "");
    assert_eq!(p, "FW1#");
}

/// 转发路径上的 ZBF 策略决定登录能否建立：permit 可登录，删除 service-policy 后拒绝。
#[test]
fn c6b_forwarded_login_follows_zone_based_policy() {
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let fw = net.add_device("FW1".into(), DeviceKind::Firewall, Dialect::Ios);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Ios);
    net.add_link(pc, fw).unwrap();
    net.add_link(fw, r1).unwrap();
    net.set_host(pc, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    let mut eng = Engine::new(net);

    let mut s = Session::default();
    run(&mut eng, &mut s, fw, &["enable", "configure terminal"]);
    run(&mut eng, &mut s, fw, &["interface g0/0"]);
    run(
        &mut eng,
        &mut s,
        fw,
        &["ip address 192.168.1.1 255.255.255.0", "no shutdown"],
    );
    run(
        &mut eng,
        &mut s,
        fw,
        &["zone-member security INSIDE", "exit"],
    );
    run(&mut eng, &mut s, fw, &["interface g0/1"]);
    run(
        &mut eng,
        &mut s,
        fw,
        &["ip address 203.0.113.1 255.255.255.0", "no shutdown"],
    );
    run(
        &mut eng,
        &mut s,
        fw,
        &["zone-member security OUTSIDE", "exit"],
    );
    run(
        &mut eng,
        &mut s,
        fw,
        &[
            "class-map type inspect match-any TRAFFIC",
            "match protocol tcp",
            "exit",
            "policy-map type inspect PMAP",
            "class type inspect TRAFFIC",
            "inspect",
            "exit",
            "exit",
        ],
    );
    run(
        &mut eng,
        &mut s,
        fw,
        &["zone-pair security IN-OUT source INSIDE destination OUTSIDE"],
    );
    run(
        &mut eng,
        &mut s,
        fw,
        &["service-policy type inspect PMAP", "exit", "end"],
    );
    ios_addr(&mut eng, r1, "g0/0", "203.0.113.10 255.255.255.0");
    ios_mgmt(&mut eng, r1);

    let (out, last, p) = ssh_attempt(&mut eng, pc, "203.0.113.10");
    assert!(
        out.contains("Trying"),
        "ZBF inspect permit 时应进入登录流程: {out:?}"
    );
    assert_eq!(last, "");
    assert_eq!(p, "R1#");

    // 登录判定按新流进行，不伪造会话表条目。
    let flows = match &eng.net.device(fw).unwrap().state {
        DeviceState::Router(rs) => rs.tcp_flows.len(),
        _ => 0,
    };
    assert_eq!(flows, 0, "登录前置判定不得凭空写入防火墙会话表");

    // 删除挂接策略：官方语义是 zone-pair 无策略即 drop。
    let mut s2 = Session::default();
    run(&mut eng, &mut s2, fw, &["enable", "configure terminal"]);
    run(
        &mut eng,
        &mut s2,
        fw,
        &["zone-pair security IN-OUT source INSIDE destination OUTSIDE"],
    );
    run(
        &mut eng,
        &mut s2,
        fw,
        &["no service-policy type inspect PMAP", "exit", "end"],
    );
    let (out, _, p) = ssh_attempt(&mut eng, pc, "203.0.113.10");
    assert!(out.contains("Network is unreachable"), "{out:?}");
    assert_eq!(p, "PC1>");

    // 重新挂接后恢复：拒绝来自策略事实，而不是被缓存。
    let mut s3 = Session::default();
    run(&mut eng, &mut s3, fw, &["enable", "configure terminal"]);
    run(
        &mut eng,
        &mut s3,
        fw,
        &["zone-pair security IN-OUT source INSIDE destination OUTSIDE"],
    );
    run(
        &mut eng,
        &mut s3,
        fw,
        &["service-policy type inspect PMAP", "exit", "end"],
    );
    let (out, last, p) = ssh_attempt(&mut eng, pc, "203.0.113.10");
    assert!(out.contains("Trying"), "重新挂接策略后应恢复: {out:?}");
    assert_eq!(last, "");
    assert_eq!(p, "R1#");
}

/// VRP 基线（FlatRules）同样接地：缺省 deny 时拒绝，放行规则后恢复。
#[test]
fn c6b_forwarded_login_follows_vrp_security_policy() {
    let mut net = Net::new();
    let pc = net.add_device("PC1".into(), DeviceKind::Host, Dialect::Ios);
    let fw = net.add_device("FW1".into(), DeviceKind::Firewall, Dialect::Vrp);
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Ios);
    net.add_link(pc, fw).unwrap();
    net.add_link(fw, r1).unwrap();
    net.set_host(pc, Some(ip("192.168.1.10")), 24, Some(ip("192.168.1.1")));
    let mut eng = Engine::new(net);

    let mut s = Session::default();
    run(&mut eng, &mut s, fw, &["system-view"]);
    run(&mut eng, &mut s, fw, &["interface g0/0/0"]);
    run(
        &mut eng,
        &mut s,
        fw,
        &["ip address 192.168.1.1 24", "undo shutdown", "quit"],
    );
    run(&mut eng, &mut s, fw, &["interface g0/0/1"]);
    run(
        &mut eng,
        &mut s,
        fw,
        &["ip address 203.0.113.1 24", "undo shutdown", "quit"],
    );
    run(
        &mut eng,
        &mut s,
        fw,
        &[
            "firewall zone trust",
            "add interface g0/0/0",
            "quit",
            "firewall zone untrust",
            "add interface g0/0/1",
            "quit",
            "quit",
        ],
    );
    ios_addr(&mut eng, r1, "g0/0", "203.0.113.10 255.255.255.0");
    ios_mgmt(&mut eng, r1);

    // 缺省 any/any → deny。
    let (out, _, p) = ssh_attempt(&mut eng, pc, "203.0.113.10");
    assert!(out.contains("Network is unreachable"), "{out:?}");
    assert_eq!(p, "PC1>");

    let mut s2 = Session::default();
    run(&mut eng, &mut s2, fw, &["system-view", "security-policy"]);
    run(
        &mut eng,
        &mut s2,
        fw,
        &[
            "rule name allow-mgmt",
            "source-zone trust",
            "destination-zone untrust",
            "action permit",
            "quit",
            "quit",
        ],
    );
    let (out, last, p) = ssh_attempt(&mut eng, pc, "203.0.113.10");
    assert!(out.contains("Trying"), "放行规则后应恢复登录: {out:?}");
    assert_eq!(last, "");
    assert_eq!(p, "R1#");
}

// ---------------------------------------------------------------- 错误分类不被接地吞掉

/// 可达但被 VTY 传输策略拒绝 → 仍是 `Connection refused`，不能退化成不可达。
#[test]
fn c6b_transport_denied_stays_refused_not_unreachable() {
    let (mut eng, pc, r1) = lab6_ready();
    let mut s = Session::default();
    let out = exec_and_run(&mut eng, &mut s, pc, "telnet 192.168.1.1");
    assert!(
        out.contains("Connection refused"),
        "VTY 只允许 SSH 时 Telnet 应是连接被拒: {out:?}"
    );
    assert_eq!(prompt(&eng, &s, pc), "PC1>");
    assert!(s.login.is_none(), "被拒绝时不应进入登录流程");

    // 放开 Telnet 后，同一目标必须变成认证失败而不是不可达。
    run(&mut eng, &mut s, r1, &["enable", "configure terminal"]);
    run(
        &mut eng,
        &mut s,
        r1,
        &["line vty 0 4", "transport input all", "exit", "exit"],
    );
    let mut s2 = Session::default();
    let out = exec_and_run(&mut eng, &mut s2, pc, "telnet 192.168.1.1");
    assert!(out.contains("Trying"), "{out:?}");
    assert_eq!(prompt(&eng, &s2, pc), "Username: ");
    let _ = exec_and_run(&mut eng, &mut s2, pc, "admin");
    let out = exec_and_run(&mut eng, &mut s2, pc, "wrong");
    assert_eq!(out, "Permission denied");
    assert_eq!(prompt(&eng, &s2, pc), "PC1>");
}

/// 四种源设备类型的错误文案契约（接地后仍然按平台基线，不新造第四种文案）。
#[test]
fn c6b_unreachable_text_follows_platform_baseline() {
    use sim_core::engine::LoginError;
    let e = LoginError::Unreachable;
    assert_eq!(
        cli_dialects::login_error_text(DeviceKind::Host, Dialect::Ios, e),
        "connect: Network is unreachable"
    );
    assert_eq!(
        cli_dialects::login_error_text(DeviceKind::Router, Dialect::Ios, e),
        "% Destination unreachable"
    );
    assert_eq!(
        cli_dialects::login_error_text(DeviceKind::Router, Dialect::Vrp, e),
        "Error: Destination host unreachable."
    );
    assert_eq!(
        cli_dialects::login_error_text(DeviceKind::Router, Dialect::Comware, e),
        "% Destination unreachable."
    );
    // 登录被拒绝时不得出现模拟器提示通道。
    for (kind, d) in [
        (DeviceKind::Host, Dialect::Ios),
        (DeviceKind::Router, Dialect::Vrp),
        (DeviceKind::Router, Dialect::Comware),
        (DeviceKind::LoadBalancer, Dialect::Tmos),
    ] {
        let t = cli_dialects::login_error_text(kind, d, e);
        assert!(!t.contains("[netemu]"), "{t}");
        assert!(!t.contains("netemu"), "{t}");
    }
}

/// 接地判定复用既有机制的直接证据：`MgmtProto::port()` 决定被匹配的服务端口。
#[test]
fn c6b_mgmt_proto_ports_are_standard() {
    assert_eq!(MgmtProto::Ssh.port(), 22);
    assert_eq!(MgmtProto::Telnet.port(), 23);
}

/// 路由器作为登录源（设备自发流量）同样接地，且错误文案回到设备平台基线。
#[test]
fn c6b_router_sourced_login_is_also_grounded() {
    let mut net = Net::new();
    let r1 = net.add_device("R1".into(), DeviceKind::Router, Dialect::Ios);
    let r2 = net.add_device("R2".into(), DeviceKind::Router, Dialect::Ios);
    net.add_link(r1, r2).unwrap();
    let mut eng = Engine::new(net);
    ios_addr(&mut eng, r1, "g0/0", "10.0.0.1 255.255.255.0");
    ios_addr(&mut eng, r2, "g0/0", "10.0.0.2 255.255.255.0");
    ios_mgmt(&mut eng, r2);

    let mut s = Session::default();
    run(&mut eng, &mut s, r1, &["enable"]);
    assert_eq!(prompt(&eng, &s, r1), "R1#");
    let out = exec_and_run(&mut eng, &mut s, r1, "ssh 10.0.0.2");
    assert!(out.contains("Connecting to 10.0.0.2"), "{out:?}");
    assert_eq!(prompt(&eng, &s, r1), "Username: ");
    let _ = exec_and_run(&mut eng, &mut s, r1, "admin");
    let _ = exec_and_run(&mut eng, &mut s, r1, "Cisco@123");
    assert_eq!(prompt(&eng, &s, r1), "R2#");
    let _ = exec_and_run(&mut eng, &mut s, r1, "exit");
    assert_eq!(prompt(&eng, &s, r1), "R1#");

    // 对端接口 shutdown → 设备自发流量同样被数据面拒绝，文案用 IOS 平台基线。
    let mut cs = Session::default();
    run(&mut eng, &mut cs, r2, &["enable", "configure terminal"]);
    run(&mut eng, &mut cs, r2, &["interface g0/0", "shutdown"]);
    let out = exec_and_run(&mut eng, &mut s, r1, "ssh 10.0.0.2");
    assert_eq!(out, "% Destination unreachable", "{}", out);
    assert_eq!(prompt(&eng, &s, r1), "R1#");
    assert!(s.login.is_none());
}
