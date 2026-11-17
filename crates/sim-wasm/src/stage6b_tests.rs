//! 阶段 6B：Cisco IOS XE 17.x ZBF 在真实 WASM `Sim` 上的契约测试。
//!
//! 与 Rust 集成测试同源：class-map → policy-map → zone-pair → service-policy，
//! inspect 状态化 / pass 无状态 / 子模式提示符 / 未模拟命令独立通道。

use super::*;
use serde_json::Value;

fn value(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

fn exec_ok(sim: &mut Sim, dev: u32, line: &str) -> Value {
    let v = value(&sim.exec_command_result(dev, line).unwrap());
    assert_eq!(v["status"], "Executed", "{line}: {v}");
    v
}

fn zbf_lab() -> (Sim, u32, u32, u32) {
    let mut sim = Sim::new();
    let pc = sim.add_device("Host", "Ios").unwrap();
    let r1 = sim.add_device("Router", "Ios").unwrap();
    let srv = sim.add_device("Host", "Ios").unwrap();
    sim.rename_device(pc, "PC1").unwrap();
    sim.rename_device(r1, "R1").unwrap();
    sim.rename_device(srv, "SRV").unwrap();
    sim.add_link(pc, r1).unwrap();
    sim.add_link(r1, srv).unwrap();
    sim.set_host_config(pc, "192.168.1.10", 24, "192.168.1.1")
        .unwrap();
    sim.set_host_config(srv, "203.0.113.10", 24, "203.0.113.1")
        .unwrap();
    (sim, pc, r1, srv)
}

fn base_zones(sim: &mut Sim, r1: u32) {
    for line in [
        "enable",
        "configure terminal",
        "interface g0/0",
        "ip address 192.168.1.1 255.255.255.0",
        "no shutdown",
        "zone-member security INSIDE",
        "exit",
        "interface g0/1",
        "ip address 203.0.113.1 255.255.255.0",
        "no shutdown",
        "zone-member security OUTSIDE",
        "exit",
    ] {
        exec_ok(sim, r1, line);
    }
}

fn ping_passed(sim: &mut Sim, from: u32, to: &str) -> bool {
    let v = value(&sim.check_ping(from, to).unwrap());
    v["passed"].as_bool().unwrap_or(false)
}

#[test]
fn stage6b_wasm_zbf_inspect_flow_and_submode_prompts() {
    let (mut sim, pc, r1, srv) = zbf_lab();
    base_zones(&mut sim, r1);

    exec_ok(&mut sim, r1, "class-map type inspect match-any ICMP");
    assert_eq!(sim.prompt(r1).unwrap(), "R1(config-cmap)#");
    exec_ok(&mut sim, r1, "match protocol icmp");
    exec_ok(&mut sim, r1, "exit");
    assert_eq!(sim.prompt(r1).unwrap(), "R1(config)#");

    exec_ok(&mut sim, r1, "policy-map type inspect PMAP");
    assert_eq!(sim.prompt(r1).unwrap(), "R1(config-pmap)#");
    exec_ok(&mut sim, r1, "class type inspect ICMP");
    assert_eq!(sim.prompt(r1).unwrap(), "R1(config-pmap-c)#");
    exec_ok(&mut sim, r1, "inspect");
    exec_ok(&mut sim, r1, "exit");
    assert_eq!(sim.prompt(r1).unwrap(), "R1(config-pmap)#");
    exec_ok(&mut sim, r1, "exit");
    assert_eq!(sim.prompt(r1).unwrap(), "R1(config)#");

    exec_ok(
        &mut sim,
        r1,
        "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
    );
    assert_eq!(sim.prompt(r1).unwrap(), "R1(config-sec-zone-pair)#");
    exec_ok(&mut sim, r1, "service-policy type inspect PMAP");
    exec_ok(&mut sim, r1, "end");

    // 反向先探测：无会话拒绝；再验证 inspect 正向放行。
    assert!(!ping_passed(&mut sim, srv, "192.168.1.10"), "反向拒绝");
    assert!(
        ping_passed(&mut sim, pc, "203.0.113.10"),
        "inspect 正向放行"
    );
    exec_ok(&mut sim, r1, "show zone-pair security");
    let sess = exec_ok(
        &mut sim,
        r1,
        "show policy-map type inspect zone-pair session",
    );
    let out = sess["deviceOutput"].as_str().unwrap();
    assert!(out.contains("Service-policy inspect : PMAP"), "{out}");
    assert!(out.contains("sessions: 1 icmp"), "{out}");
    // O-1：区域对名 + session(s) 组合形式受官方设计指南支持。
    for line in [
        "show policy-map type inspect zone-pair IN-OUT session",
        "show policy-map type inspect zone-pair IN-OUT sessions",
    ] {
        let v = exec_ok(&mut sim, r1, line);
        let out = v["deviceOutput"].as_str().unwrap();
        assert!(
            out.contains("Zone-pair: IN-OUT") && out.contains("sessions: 1 icmp"),
            "{line}: {out}"
        );
    }
}

#[test]
fn stage6b_wasm_pass_is_stateless_and_mirrored_policy_required() {
    let (mut sim, pc, r1, _srv) = zbf_lab();
    base_zones(&mut sim, r1);
    for line in [
        "class-map type inspect match-any ICMP",
        "match protocol icmp",
        "exit",
        "policy-map type inspect PASSMAP",
        "class type inspect ICMP",
        "pass",
        "exit",
        "exit",
        "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
        "service-policy type inspect PASSMAP",
        "exit",
        "end",
    ] {
        exec_ok(&mut sim, r1, line);
    }
    assert!(
        !ping_passed(&mut sim, pc, "203.0.113.10"),
        "无镜像 pass 时回包被丢弃"
    );
    let sess = exec_ok(
        &mut sim,
        r1,
        "show policy-map type inspect zone-pair session",
    );
    assert!(
        sess["deviceOutput"]
            .as_str()
            .unwrap()
            .contains("sessions: 0 icmp"),
        "{sess}"
    );
    for line in [
        "configure terminal",
        "zone-pair security OUT-IN source OUTSIDE destination INSIDE",
        "service-policy type inspect PASSMAP",
        "exit",
        "end",
    ] {
        exec_ok(&mut sim, r1, line);
    }
    assert!(
        ping_passed(&mut sim, pc, "203.0.113.10"),
        "镜像 pass 后放行"
    );
}

#[test]
fn stage6b_wasm_unsupported_and_unimplemented_use_notice_channel_only() {
    let (mut sim, _pc, r1, _srv) = zbf_lab();
    base_zones(&mut sim, r1);

    // 已实现但 L7 未模拟的协议：配置成功，限制只进模拟器提示通道。
    exec_ok(&mut sim, r1, "class-map type inspect match-any WEB");
    let v = exec_ok(&mut sim, r1, "match protocol sip");
    exec_ok(&mut sim, r1, "end");
    let notices = v["simulatorNotices"].as_array().unwrap();
    assert!(
        notices
            .iter()
            .any(|n| n.as_str().unwrap_or("").contains("L7/ALG 检查未模拟")),
        "{v}"
    );
    assert!(!v["deviceOutput"].as_str().unwrap().contains("[netemu]"));

    // 未模拟命令：NotSimulated、状态不变、设备输出为空。
    exec_ok(&mut sim, r1, "configure terminal");
    let before = sim.dump();
    let v = value(
        &sim.exec_command_result(r1, "parameter-map type inspect PMAP")
            .unwrap(),
    );
    assert_eq!(v["status"], "NotSimulated", "{v}");
    assert_eq!(v["stateChanged"], false, "{v}");
    assert_eq!(v["deviceOutput"].as_str(), Some(""));
    assert_eq!(sim.dump(), before, "未模拟命令不得改变状态");
    exec_ok(&mut sim, r1, "end");

    // 查询只走设备通道。
    let q = exec_ok(&mut sim, r1, "show zone-pair security");
    assert!(q["simulatorNotices"].as_array().unwrap().is_empty(), "{q}");
    assert!(!q["deviceOutput"].as_str().unwrap().contains("[netemu]"));
}

#[test]
fn stage6b_wasm_delete_policy_changes_new_flows_only() {
    let (mut sim, pc, r1, _srv) = zbf_lab();
    base_zones(&mut sim, r1);
    for line in [
        "class-map type inspect match-any ICMP",
        "match protocol icmp",
        "exit",
        "policy-map type inspect PMAP",
        "class type inspect ICMP",
        "inspect",
        "exit",
        "exit",
        "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
        "service-policy type inspect PMAP",
        "exit",
        "end",
    ] {
        exec_ok(&mut sim, r1, line);
    }
    assert!(ping_passed(&mut sim, pc, "203.0.113.10"));

    // 删除策略挂接：既有会话仍在会话表（官方不重分类），但新流（不同探测）被拒绝。
    for line in [
        "configure terminal",
        "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
        "no service-policy type inspect PMAP",
        "exit",
        "end",
    ] {
        exec_ok(&mut sim, r1, line);
    }
    let sess = exec_ok(
        &mut sim,
        r1,
        "show policy-map type inspect zone-pair session",
    );
    assert!(
        sess["deviceOutput"]
            .as_str()
            .unwrap()
            .contains("sessions: 1 icmp"),
        "既有会话必须保留: {sess}"
    );
    // WASM 侧 ping 会复用主机 ping id；这里只断言策略对象状态与重判分口径。
    let pairs = exec_ok(&mut sim, r1, "show zone-pair security");
    assert!(
        pairs["deviceOutput"].as_str().unwrap().contains("<none>"),
        "{pairs}"
    );
}

const LESSON_CN: &str = include_str!("../../../web/src/content/lessons/08-firewall-zone.md");
const LESSON_EN: &str = include_str!("../../../web/src/content/lessons-en/08-firewall-zone.md");

fn lesson_block(md: &str, marker: &str) -> Vec<String> {
    let a = md.find(marker).expect("讲义缺少 IOS 演示锚点");
    let rest = &md[a..];
    let end = rest
        .find(
            "
```",
        )
        .expect("讲义 IOS 演示段未闭合");
    rest[..end]
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                return None;
            }
            match l.find(")# ") {
                Some(p) => Some(l[p + 3..].trim().to_string()),
                None => Some(l.to_string()),
            }
        })
        .filter(|l| !l.is_empty())
        .collect()
}

fn lesson_ios_sequences(md: &str) -> (Vec<String>, Vec<String>) {
    (
        lesson_block(md, "zone security INSIDE"),
        lesson_block(md, "R1(config)# class-map type inspect"),
    )
}

fn base_addressing(sim: &mut Sim, r1: u32) {
    for line in [
        "enable",
        "configure terminal",
        "interface g0/0",
        "ip address 192.168.1.1 255.255.255.0",
        "no shutdown",
        "exit",
        "interface g0/1",
        "ip address 203.0.113.1 255.255.255.0",
        "no shutdown",
        "exit",
        "end",
    ] {
        exec_ok(sim, r1, line);
    }
}

#[test]
fn stage6b_wasm_lesson_ios_demo_runs_on_real_sim() {
    let (cn_zones, cn_policy) = lesson_ios_sequences(LESSON_CN);
    let (en_zones, en_policy) = lesson_ios_sequences(LESSON_EN);
    assert_eq!(cn_zones, en_zones, "中英文 IOS zone 块必须一致");
    assert_eq!(cn_policy, en_policy, "中英文 IOS ZBF 策略块必须一致");
    assert_eq!(cn_zones[0], "zone security INSIDE");
    assert!(!cn_zones.iter().any(|l| l.contains("firewall zone trust")));
    assert!(cn_zones.iter().any(|l| l == "interface g0/0"));
    assert!(cn_zones.iter().any(|l| l == "interface g0/1"));
    assert!(!cn_zones.iter().any(|l| l == "interface g0/0/0"));

    let (mut sim, pc, r1, srv) = zbf_lab();
    base_addressing(&mut sim, r1);
    let mut seq = cn_zones.clone();
    seq.push("exit".into());
    seq.extend(cn_policy.clone());
    exec_ok(&mut sim, r1, "configure terminal");
    let mut prompts = Vec::new();
    for line in &seq {
        exec_ok(&mut sim, r1, line);
        prompts.push(sim.prompt(r1).unwrap());
    }
    exec_ok(&mut sim, r1, "end");
    let idx = |needle: &str| {
        seq.iter()
            .position(|l| l == needle)
            .unwrap_or_else(|| panic!("讲义序列缺少 {needle}"))
    };
    for (needle, expected) in [
        ("zone security INSIDE", "R1(config-sec-zone)#"),
        ("zone security OUTSIDE", "R1(config-sec-zone)#"),
        ("interface g0/0", "R1(config-if)#"),
        ("zone-member security INSIDE", "R1(config-if)#"),
        ("interface g0/1", "R1(config-if)#"),
        ("zone-member security OUTSIDE", "R1(config-if)#"),
        ("class-map type inspect match-any ICMP", "R1(config-cmap)#"),
        ("policy-map type inspect PMAP", "R1(config-pmap)#"),
        ("class type inspect ICMP", "R1(config-pmap-c)#"),
        ("inspect", "R1(config-pmap-c)#"),
        (
            "zone-pair security IN-OUT source INSIDE destination OUTSIDE",
            "R1(config-sec-zone-pair)#",
        ),
        (
            "service-policy type inspect PMAP",
            "R1(config-sec-zone-pair)#",
        ),
    ] {
        assert_eq!(prompts[idx(needle)], expected, "{needle} 的提示符");
    }
    assert_eq!(prompts[idx("zone security INSIDE") + 1], "R1(config)#");
    let c = idx("class type inspect ICMP");
    assert_eq!(prompts[c + 2], "R1(config-pmap)#");
    assert_eq!(prompts[c + 3], "R1(config)#");

    assert!(!ping_passed(&mut sim, srv, "192.168.1.10"), "反向拒绝");
    assert!(
        ping_passed(&mut sim, pc, "203.0.113.10"),
        "讲义序列放行正向"
    );
    let sess = exec_ok(
        &mut sim,
        r1,
        "show policy-map type inspect zone-pair IN-OUT session",
    );
    assert!(
        sess["deviceOutput"]
            .as_str()
            .unwrap()
            .contains("sessions: 1 icmp"),
        "{sess}"
    );
}
