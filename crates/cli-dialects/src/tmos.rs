//! F5 BIG-IP TMOS 17.x / tmsh 独立命令树（阶段 5A）。
//!
//! 与 IOS / VRP / Comware 三棵树完全隔离：本文件只定义 tmsh 模块层级与
//! `ltm pool` / `ltm virtual` 官方语法的可执行子集；三棵厂商树不再承载任何
//! 抽象 `ltm` 命令。共享 `ParamKind` 参数工具、`specs` 规格机制与
//! `sim-core` 唯一状态事实来源；`CNode` 仍是语法、帮助、补全与稳定 ID 的唯一来源。
//! 官方依据见 `docs/cli-fidelity/stage5a-implementation-record.md` §2（F1–F6）。
//! 无设备样本：所有 `list` / `show` 输出只做结构化断言，不冒充逐字厂商文本。

use cli_engine::{CNode, CliError, Ctx, Mode, ParamKind};
use sim_core::device::{DeviceKind, LtmMember, LtmPool, LtmVirtual};

use crate::{ipv4_from_str, ipv4_str, Trees};

/// 默认工作文件夹（F6：`/Common` is the default folder for creating new
/// configurations objects）。首批只承诺 `/Common/` 及裸对象名。
const COMMON: &str = "/Common/";

enum ObjName {
    Ok(String),
    /// 其他 Partition / Route Domain：语法有效但模拟器未实现（NotSimulated）。
    NotSimulated,
}

/// 把 `/Common/<name>` 规范化为裸名；裸名按默认 `/Common` 处理。
/// 以 `/` 开头但不属于 `/Common/` 的全路径、含 `%` 的 Route Domain 语法
/// 一律标记为未模拟，不静默改判、不写状态。
fn obj_name(raw: &str) -> ObjName {
    if let Some(rest) = raw.strip_prefix(COMMON) {
        return ObjName::Ok(rest.to_string());
    }
    if raw.starts_with('/') || raw.contains('%') {
        return ObjName::NotSimulated;
    }
    ObjName::Ok(raw.to_string())
}

fn name_of(c: &mut Ctx, key: &str) -> Result<String, CliError> {
    let raw = c.word(key);
    match obj_name(&raw) {
        ObjName::Ok(n) if !n.is_empty() => Ok(n),
        ObjName::Ok(_) => Err(CliError::Custom("Invalid object name".into())),
        ObjName::NotSimulated => {
            c.not_simulated = true;
            Ok(String::new())
        }
    }
}

/// 解析 tmsh 的 `<ipv4>:<port>`（F3：`destination a.b.c.d[:port]`）。
fn parse_endpoint(s: &str) -> Result<(u32, u16), CliError> {
    let Some((addr, port)) = s.rsplit_once(':') else {
        return Err(CliError::Custom("Expected <ipv4>:<port>".into()));
    };
    let ip = ipv4_from_str(addr).ok_or_else(|| CliError::Custom("Invalid IPv4 address".into()))?;
    let p: u32 = port
        .parse()
        .map_err(|_| CliError::Custom("Invalid service port".into()))?;
    if p == 0 || p > u16::MAX as u32 {
        return Err(CliError::Custom("Invalid service port".into()));
    }
    Ok((ip, p as u16))
}

fn require_lb(c: &Ctx) -> Result<(), CliError> {
    if c.device_kind() == DeviceKind::LoadBalancer {
        Ok(())
    } else {
        Err(CliError::Custom(
            "LTM components are only available on a load balancer".into(),
        ))
    }
}

// ---------------------------------------------------------------- 模块导航

/// F1：`(tmos)# ltm` → `(tmos.ltm)#`。
fn h_enter_ltm(c: &mut Ctx) -> Result<String, CliError> {
    require_lb(c)?;
    c.sess.mode = Mode::TmosLtm;
    Ok(String::new())
}

/// F1：`sys` 是 root 的下级模块之一；`(tmos)# sys` → `(tmos.sys)#`。
fn h_enter_sys(c: &mut Ctx) -> Result<String, CliError> {
    c.sess.mode = Mode::TmosSys;
    Ok(String::new())
}

/// F4：`exit` 退出当前模块/组件、返回上一级。
fn h_exit(c: &mut Ctx) -> Result<String, CliError> {
    c.sess.mode = Mode::Exec;
    Ok(String::new())
}

/// F5：`quit` 退出整个 tmsh。模拟器没有独立的 BIG-IP 系统 shell 层，
/// 统一回到 root 模块提示符 `(tmos)#`；这是明示的覆盖边界（见实施记录 §11）。
fn h_quit(c: &mut Ctx) -> Result<String, CliError> {
    c.sess.mode = Mode::Exec;
    Ok(String::new())
}

fn h_cd(_c: &mut Ctx) -> Result<String, CliError> {
    Ok(String::new())
}

/// F6：只有 `/Common` 与 `/` 是首批承诺的工作文件夹。
fn h_cd_other(c: &mut Ctx) -> Result<String, CliError> {
    let folder = c.word("folder");
    if folder == "/Common" || folder == "/" {
        return Ok(String::new());
    }
    c.not_simulated = true;
    Ok(String::new())
}

fn h_pwd(_c: &mut Ctx) -> Result<String, CliError> {
    Ok("/Common".into())
}

fn h_help(_c: &mut Ctx) -> Result<String, CliError> {
    Ok("Modules: auth cli gtm ltm net sys wom\nCommands: create modify delete list show cd pwd exit quit help".into())
}

fn h_version(_c: &mut Ctx) -> Result<String, CliError> {
    // 无设备样本，不伪造版本号；只给出结构化标识。
    Ok("TMOS 17.x (Documentation Baseline)".into())
}

// ---------------------------------------------------------------- LTM 语义

fn h_pool_create(c: &mut Ctx) -> Result<String, CliError> {
    let name = name_of(c, "name")?;
    if name.is_empty() {
        return Ok(String::new());
    }
    require_lb(c)?;
    let Some(d) = c.eng.net.device_mut(c.dev) else {
        return Err(CliError::Custom("Device unavailable".into()));
    };
    if d.ltm.pools.iter().any(|p| p.name == name) {
        return Err(CliError::Custom("Pool already exists".into()));
    }
    d.ltm.pools.push(LtmPool {
        name,
        members: vec![],
    });
    Ok(String::new())
}

/// F2 官方示例 `create pool my_pool members add { 10.2.3.11:http 10.2.3.12:http }`：
/// 一次命令创建池并加入成员。首批按单个成员逐 token 实现。
fn h_pool_create_with_member(c: &mut Ctx) -> Result<String, CliError> {
    let name = name_of(c, "name")?;
    if name.is_empty() {
        return Ok(String::new());
    }
    let (ip, port) = parse_endpoint(&c.word("member"))?;
    require_lb(c)?;
    let Some(d) = c.eng.net.device_mut(c.dev) else {
        return Err(CliError::Custom("Device unavailable".into()));
    };
    if !d.ltm.pools.iter().any(|p| p.name == name) {
        d.ltm.pools.push(LtmPool {
            name: name.clone(),
            members: vec![],
        });
    }
    let pool = d.ltm.pools.iter_mut().find(|p| p.name == name).unwrap();
    let m = LtmMember { ip, port };
    if pool.members.contains(&m) {
        return Err(CliError::Custom("Pool member already exists".into()));
    }
    pool.members.push(m);
    Ok(String::new())
}

fn h_pool_delete(c: &mut Ctx) -> Result<String, CliError> {
    let name = name_of(c, "name")?;
    if name.is_empty() {
        return Ok(String::new());
    }
    require_lb(c)?;
    let Some(d) = c.eng.net.device_mut(c.dev) else {
        return Err(CliError::Custom("Device unavailable".into()));
    };
    // F2：删除 pool 前必须移除所有引用。
    if d.ltm.virtuals.iter().any(|v| v.pool == name) {
        return Err(CliError::Custom(
            "Pool is referenced by a virtual server".into(),
        ));
    }
    let before = d.ltm.pools.len();
    d.ltm.pools.retain(|p| p.name != name);
    if d.ltm.pools.len() == before {
        return Err(CliError::Custom("Pool not found".into()));
    }
    d.ltm.sessions.clear();
    d.ltm.rr = 0;
    Ok(String::new())
}

/// F2：`modify pool <name> members add { <ip>:<port> }`。
fn h_member_add(c: &mut Ctx) -> Result<String, CliError> {
    let name = name_of(c, "name")?;
    if name.is_empty() {
        return Ok(String::new());
    }
    let (ip, port) = parse_endpoint(&c.word("member"))?;
    require_lb(c)?;
    let Some(d) = c.eng.net.device_mut(c.dev) else {
        return Err(CliError::Custom("Device unavailable".into()));
    };
    let Some(pool) = d.ltm.pools.iter_mut().find(|p| p.name == name) else {
        return Err(CliError::Custom("Pool not found".into()));
    };
    let m = LtmMember { ip, port };
    if pool.members.contains(&m) {
        return Err(CliError::Custom("Pool member already exists".into()));
    }
    pool.members.push(m);
    Ok(String::new())
}

fn h_member_delete(c: &mut Ctx) -> Result<String, CliError> {
    let name = name_of(c, "name")?;
    if name.is_empty() {
        return Ok(String::new());
    }
    let (ip, port) = parse_endpoint(&c.word("member"))?;
    require_lb(c)?;
    let Some(d) = c.eng.net.device_mut(c.dev) else {
        return Err(CliError::Custom("Device unavailable".into()));
    };
    let Some(pool) = d.ltm.pools.iter_mut().find(|p| p.name == name) else {
        return Err(CliError::Custom("Pool not found".into()));
    };
    let m = LtmMember { ip, port };
    let before = pool.members.len();
    pool.members.retain(|x| *x != m);
    if pool.members.len() == before {
        return Err(CliError::Custom("Pool member not found".into()));
    }
    Ok(String::new())
}

/// F3：`create virtual <name> destination <ipv4:port> pool <pool_name>`。
fn h_virtual_create(c: &mut Ctx) -> Result<String, CliError> {
    let name = name_of(c, "name")?;
    let pool = name_of(c, "pool")?;
    if name.is_empty() || pool.is_empty() {
        return Ok(String::new());
    }
    let (vip, port) = parse_endpoint(&c.word("destination"))?;
    require_lb(c)?;
    let Some(d) = c.eng.net.device_mut(c.dev) else {
        return Err(CliError::Custom("Device unavailable".into()));
    };
    if !d.ltm.pools.iter().any(|p| p.name == pool) {
        return Err(CliError::Custom("Pool not found".into()));
    }
    let v = LtmVirtual {
        name,
        vip,
        port,
        pool,
    };
    if d.ltm.virtuals.iter().any(|x| x.name == v.name) {
        return Err(CliError::Custom("Virtual server already exists".into()));
    }
    d.ltm.virtuals.push(v);
    Ok(String::new())
}

fn h_virtual_delete(c: &mut Ctx) -> Result<String, CliError> {
    let name = name_of(c, "name")?;
    if name.is_empty() {
        return Ok(String::new());
    }
    require_lb(c)?;
    let Some(d) = c.eng.net.device_mut(c.dev) else {
        return Err(CliError::Custom("Device unavailable".into()));
    };
    let before = d.ltm.virtuals.len();
    d.ltm.virtuals.retain(|v| v.name != name);
    if d.ltm.virtuals.len() == before {
        return Err(CliError::Custom("Virtual server not found".into()));
    }
    d.ltm.sessions.clear();
    d.ltm.rr = 0;
    Ok(String::new())
}

fn pool_list_text(c: &Ctx, only: &str) -> Result<String, CliError> {
    require_lb(c)?;
    let Some(d) = c.eng.net.device(c.dev) else {
        return Err(CliError::Custom("Device unavailable".into()));
    };
    let mut out = String::new();
    for p in d
        .ltm
        .pools
        .iter()
        .filter(|p| only.is_empty() || p.name == only)
    {
        out.push_str(&format!("ltm pool /Common/{} {{\n", p.name));
        out.push_str("    members {\n");
        for m in &p.members {
            out.push_str(&format!(
                "        {}:{} {{ address {} }}\n",
                ipv4_str(m.ip),
                m.port,
                ipv4_str(m.ip)
            ));
        }
        out.push_str("    }\n}\n");
    }
    if out.is_empty() {
        out.push_str("(none)\n");
    }
    Ok(out.trim_end().to_string())
}

fn virtual_list_text(c: &Ctx, only: &str) -> Result<String, CliError> {
    require_lb(c)?;
    let Some(d) = c.eng.net.device(c.dev) else {
        return Err(CliError::Custom("Device unavailable".into()));
    };
    let mut out = String::new();
    for v in d
        .ltm
        .virtuals
        .iter()
        .filter(|v| only.is_empty() || v.name == only)
    {
        out.push_str(&format!("ltm virtual /Common/{} {{\n", v.name));
        out.push_str(&format!("    destination {}:{}\n", ipv4_str(v.vip), v.port));
        out.push_str(&format!("    pool /Common/{}\n", v.pool));
        out.push_str("}\n");
    }
    if out.is_empty() {
        out.push_str("(none)\n");
    }
    Ok(out.trim_end().to_string())
}

fn h_list_pool(c: &mut Ctx) -> Result<String, CliError> {
    let only = match obj_name(&c.word("name")) {
        ObjName::Ok(n) => n,
        ObjName::NotSimulated => {
            c.not_simulated = true;
            return Ok(String::new());
        }
    };
    pool_list_text(c, &only)
}

fn h_list_virtual(c: &mut Ctx) -> Result<String, CliError> {
    let only = match obj_name(&c.word("name")) {
        ObjName::Ok(n) => n,
        ObjName::NotSimulated => {
            c.not_simulated = true;
            return Ok(String::new());
        }
    };
    virtual_list_text(c, &only)
}

/// F1：`show` 显示统计与运行时状态。这里输出当前会话分配（来自唯一状态源）。
fn h_show_pool(c: &mut Ctx) -> Result<String, CliError> {
    let base = h_list_pool(c)?;
    require_lb(c)?;
    let sessions = c.eng.ltm_sessions_of(c.dev);
    let mut out = base;
    out.push_str("\nSessions:\n");
    if sessions.is_empty() {
        out.push_str("  (none)");
    } else {
        for s in sessions {
            out.push_str(&format!(
                "  {}:{} -> {}:{} via {}:{}\n",
                ipv4_str(s.client_ip),
                s.client_port,
                ipv4_str(s.vip),
                s.vip_port,
                ipv4_str(s.member_ip),
                s.member_port
            ));
        }
    }
    Ok(out.trim_end().to_string())
}

fn h_show_virtual(c: &mut Ctx) -> Result<String, CliError> {
    h_list_virtual(c)
}

// ---------------------------------------------------------------- 语法组装

fn object_name_node(key: &str) -> CNode {
    CNode::param(
        ParamKind::Word,
        key,
        "Object name, optionally /Common/<name>",
    )
}

/// `members add { <ip>:<port> }` / `members delete { <ip>:<port> }`（F2）。
fn members_branch(add_handler: fn(&mut Ctx) -> Result<String, CliError>) -> CNode {
    CNode::lit("members", "Pool members")
        .kid(
            CNode::lit("add", "Add members").kid(
                CNode::lit("{", "Begin member list").kid(
                    CNode::param(ParamKind::Word, "member", "<ipv4>:<port>")
                        .kid(CNode::lit("}", "End member list").run(add_handler)),
                ),
            ),
        )
        .kid(
            CNode::lit("delete", "Delete members").kid(
                CNode::lit("{", "Begin member list").kid(
                    CNode::param(ParamKind::Word, "member", "<ipv4>:<port>")
                        .kid(CNode::lit("}", "End member list").run(h_member_delete)),
                ),
            ),
        )
}

fn pool_verbs(add_handler: fn(&mut Ctx) -> Result<String, CliError>) -> Vec<CNode> {
    // `members` 节点只挂一份，避免同层重复字面量导致解析歧义 / lint duplicate。
    // create 时 add_handler 先建池再加成员（F2 官方示例）；modify 时要求池已存在。
    vec![CNode::lit("pool", "Load balancing pool").kid(
        object_name_node("name")
            .run(h_pool_create)
            .kid(members_branch(add_handler)),
    )]
}

fn virtual_verbs() -> Vec<CNode> {
    vec![CNode::lit("virtual", "Virtual server").kid(
        object_name_node("name").kid(
            CNode::lit("destination", "Virtual address and service").kid(
                CNode::param(ParamKind::Word, "destination", "<ipv4>:<port>").kid(
                    CNode::lit("pool", "Default pool")
                        .kid(object_name_node("pool").run(h_virtual_create)),
                ),
            ),
        ),
    )]
}

fn delete_verbs() -> Vec<CNode> {
    vec![
        CNode::lit("pool", "Load balancing pool").kid(object_name_node("name").run(h_pool_delete)),
        CNode::lit("virtual", "Virtual server").kid(object_name_node("name").run(h_virtual_delete)),
    ]
}

fn list_verbs() -> Vec<CNode> {
    vec![
        CNode::lit("pool", "List pool configuration")
            .run(h_list_pool)
            .kid(object_name_node("name").run(h_list_pool)),
        CNode::lit("virtual", "List virtual server configuration")
            .run(h_list_virtual)
            .kid(object_name_node("name").run(h_list_virtual)),
    ]
}

fn show_verbs() -> Vec<CNode> {
    vec![
        CNode::lit("pool", "Show pool status")
            .run(h_show_pool)
            .kid(object_name_node("name").run(h_show_pool)),
        CNode::lit("virtual", "Show virtual server status")
            .run(h_show_virtual)
            .kid(object_name_node("name").run(h_show_virtual)),
    ]
}

fn cd_branch() -> CNode {
    CNode::lit("cd", "Change the current working folder")
        .kid(CNode::lit("/Common", "Default folder").run(h_cd))
        .kid(CNode::param(ParamKind::Word, "folder", "Folder name").run(h_cd_other))
}

fn common_cmds() -> Vec<CNode> {
    vec![
        cd_branch(),
        CNode::lit("pwd", "Display the current working folder").run(h_pwd),
        CNode::lit("help", "Display the tmsh module hierarchy").run(h_help),
        CNode::lit("exit", "Exit the current tmsh module").run(h_exit),
        CNode::lit("quit", "Exit tmsh").run(h_quit),
    ]
}

/// root 模块 `(tmos)#`：命令在前、模块路径在后（F1：`list ltm profile`）。
fn root_cmds() -> Vec<CNode> {
    let ltm_prefix = |verb: &str, help: &str, kids: Vec<CNode>| {
        CNode::lit(verb, help).kid(CNode::lit("ltm", "Local Traffic Manager module").kids(kids))
    };
    vec![
        CNode::lit("ltm", "Enter the Local Traffic Manager module").run(h_enter_ltm),
        CNode::lit("sys", "Enter the System module").run(h_enter_sys),
        ltm_prefix(
            "create",
            "Create a configuration object",
            pool_verbs(h_pool_create_with_member)
                .into_iter()
                .chain(virtual_verbs())
                .collect(),
        ),
        ltm_prefix(
            "modify",
            "Modify a configuration object",
            pool_verbs(h_pool_create_with_member)
                .into_iter()
                .chain(virtual_verbs())
                .collect(),
        ),
        ltm_prefix("delete", "Delete a configuration object", delete_verbs()),
        ltm_prefix("list", "Display configuration", list_verbs()),
        // `show` 只出现一个根节点，同时承载 ltm 组件与 sys 版本。
        CNode::lit("show", "Display statistics and runtime status")
            .kid(CNode::lit("ltm", "Local Traffic Manager module").kids(show_verbs()))
            .kid(
                CNode::lit("sys", "System module")
                    .kid(CNode::lit("version", "Software version").run(h_version)),
            ),
    ]
}

/// 构建 TMOS 17.x / tmsh 独立命令树。
pub fn build() -> Trees {
    let mut exec = root_cmds();
    exec.extend(common_cmds());

    // ltm 模块 `(tmos.ltm)#`：组件名相对当前模块，不带 `ltm` 前缀（F1/F2/F3）。
    let mut tmos_ltm = vec![
        CNode::lit("create", "Create a configuration object")
            .kids(pool_verbs(h_pool_create_with_member))
            .kids(virtual_verbs()),
        CNode::lit("modify", "Modify a configuration object").kids(pool_verbs(h_member_add)),
        CNode::lit("delete", "Delete a configuration object").kids(delete_verbs()),
        CNode::lit("list", "Display configuration").kids(list_verbs()),
        CNode::lit("show", "Display statistics and runtime status").kids(show_verbs()),
    ];
    tmos_ltm.extend(common_cmds());

    let mut tmos_sys = vec![
        CNode::lit("list", "Display configuration")
            .kid(CNode::lit("version", "Software version").run(h_version)),
        CNode::lit("show", "Display statistics and runtime status")
            .kid(CNode::lit("version", "Software version").run(h_version)),
    ];
    tmos_sys.extend(common_cmds());

    Trees {
        exec,
        tmos_ltm,
        tmos_sys,
        priv_: vec![],
        config: vec![],
        if_: vec![],
        pool: vec![],
        acl: vec![],
        line: vec![],
        aaa: vec![],
        mst: vec![],
        ospf: vec![],
        ospf_area: vec![],
        bgp: vec![],
        isis: vec![],
        route_policy: vec![],
        zone: vec![],
        security_policy: vec![],
        security_rule: vec![],
        ipsec: vec![],
        ospfv3: vec![],
        // F5 TMOS 无 SRv6 Policy 视图。
        srv6_policy: vec![],
        vlan: vec![],
        // 阶段 6B：ZBF 子模式只存在于 IOS XE 树。
        class_map: vec![],
        policy_map: vec![],
        policy_map_class: vec![],
    }
}
