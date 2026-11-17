# 阶段 5A · F5 TMOS 17.x 官方证据 / 独立 Dialect::Tmos / tmsh 层级与 LTM 首批闭环 · 实施记录

> 执行日期：2026-11-13
> 基线：`3d784e022c8e70ca3c4aafbce65edd1220064ed7`（detached HEAD，未提交、未推送）
> 上一记录：`docs/cli-fidelity/stage4a-implementation-record.md`（含 §0 复核补修，保留，**未回写**）
> 范围依据：`netemu-stage5a-implementation-handoff.md`；技术约束取主方案 §4.3 / §4.4 / §10.1 / §11 / §13 / §15
> 明确不做：C6b、完整防火墙 / ASA / VRP 拆分、完整 IOS ZBF、Comware 非首批族、`run()` 前端事件动画、TMOS 全功能（GTM/ASM/APM/iRule 等）
> 停止点：完成官方证据、独立 Tmos 方言、tmsh 层级与 LTM 首批闭环、课程与实验去抽象化、Member 分配验收后停止

---

## 1. 现场保护与基线复跑

| 项 | 结果 |
|---|---|
| `git rev-parse HEAD` | `3d784e022c8e70ca3c4aafbce65edd1220064ed7` |
| `git branch --show-current` | 空 → **detached HEAD（no branch）** |
| 起点工作区 | **29 个已跟踪 M + 19 组未跟踪**（阶段 0/1、2A/2B/2C、3A/3B、4A 含补修全部保留） |
| 现场保护 | 全程未 `reset` / `clean` / 覆盖 / 删测 / 注释测试 / 放宽断言；未提交、未推送 |
| 基线 `cargo test --workspace --quiet` | **291 passed / 0 failed，48 个测试二进制**，`EXIT=0`（与交接方一致） |
| 构建目录 | 独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-5a-target` |
| 阅读范围 | 交接文档、主方案 §4.3/4.4/10.1/11/13/15、Stage 4A 记录全文（含 §0）、`cli-engine/src/lib.rs`、`cli-dialects/src/{lib,ios,huawei,comware,show,config_render,specs,ifname}.rs`、`sim-core/src/{device,engine,summary,profile}.rs`、`sim-wasm/src/{lib,lab_defs,stage4a_tests}.rs`、`lab-checks/src/lib.rs`、`ltm_golden.rs`、`stage3a_cmdmap.rs`、`stage4a_comware.rs`、`web/src` 课程/对照表/存档/UI 相关文件 |

**命名澄清（事实核对）**：交接中称「lab14」指**第 14 课 F5 LTM**；代码里 `labs.ts` 的 `lab14` 变量实际是 `wlan-advanced`，F5 实验是 **`lab19` / `id:'f5-ltm'`**（`labs.ts:1129-1170`），讲义为 `lessons/14-f5-ltm.md`。本包以 `f5-ltm` 实验为去抽象化对象，并同步处理 `dc-comprehensive`（lab23）中的 LTM 任务，避免残留抽象命令。

---

## 2. F5 TMOS 17.x 官方证据（访问日期均为 2026-11-13）

### 2.1 证据表

全部为 F5 官方 **F5 TMSH Reference v17** 网页正文全文核验（非搜索快照摘要）。页面版本选择器列出 `v17.0.0 / v16.0.0 / v15.0.0 / v14.0.0 / v13.0.0`，本包取 **v17.0.0** 为 17.x 主证据。证据等级统一 `DocumentationOnly`：无带版本 BIG-IP 设备 / VE 样本，**不建立逐字输出黄金**，只建立语法、模块层级、结构化状态与数据面断言。

| # | 文档标题 | URL | 版本 / 适用性 | 全文边界 |
|---|---|---|---|---|
| F1 | **TMSH** — `tmsh(1)`, *F5 TMSH Reference v17* | `https://clouddocs.f5.com/cli/tmsh-reference/latest/general/tmsh.html` | v17.0.0（页面版本选择器 v17.0.0 为最新） | 全文 `MODULES` 段：结构分层，root module 的下级模块为 `auth, cli, gtm, ltm, net, sys, wom`；`(tmos)# ltm` → `(tmos.ltm)#`；`list` 显示配置、`show` 显示统计与运行状态；`/` 前缀表示相对 root 模块的完整路径（`list /net vlan`、`list ltm profile`）；`exit` 退出模块：`(tmos.ltm)# exit` → `(tmos)#`；`help` 显示模块层级 |
| F2 | **ltm pool** — `ltm pool(1)`, *F5 TMSH Reference v17* | `https://clouddocs.f5.com/cli/tmsh-reference/latest/modules/ltm/ltm_pool.html` | v17 | 全文 `SYNTAX` / `EXAMPLES` / `OPTIONS`：`create pool [name]`、`modify pool [name] members add { [node_name:port] { address [ip] } }`、`list pool [name]`、`show pool`、`delete pool [name]`；示例 `create pool my_pool members add { 10.2.3.11:http 10.2.3.12:http }`；`delete pool` 说明「You must remove all references to a pool before you can delete the pool」；`mv /ltm pool /Common/my_pool ...` 给出 `/Common/` 全路径；`load-balancing-mode` 默认 `round-robin` |
| F3 | **ltm virtual** — `ltm virtual(1)`, *F5 TMSH Reference v17* | `https://clouddocs.f5.com/cli/tmsh-reference/latest/modules/ltm/ltm_virtual.html` | v17 | 全文 `SYNTAX` / `EXAMPLES` / `OPTIONS`：`create virtual [name]`、`modify virtual [name]`、`list virtual [name]`、`show virtual`、`delete virtual [name]`；`destination [ipv4:port]`（`a.b.c.d[:port]`，默认 `any:any`）；`pool [pool_name | none]`；`ip-protocol` 默认 `any`；示例 `create virtual myV2 { destination 11.11.11.12:any ... } pool myPool`、`mv /ltm virtual /Common/my_vip ...` |
| F4 | **exit** — `exit(1)`, *F5 TMSH Reference v17* | `https://clouddocs.f5.com/cli/tmsh-reference/latest/commands/exit.html` | v17 | 全文：「Exits a tmsh module or component」；「Use the command `exit` within a tmsh module or component to leave that module or component and return to the higher level of the shell structure」；「Note that to exit tmsh and return to the BIG-IP system prompt, use the command `quit`」 |
| F5 | **quit** — `quit(1)`, *F5 TMSH Reference v17* | `https://clouddocs.f5.com/cli/tmsh-reference/latest/commands/quit.html` | v17 | 全文：「Exits tmsh」；「close tmsh and return to the BIG-IP system prompt」；「Note that to exit a tmsh module or component, you use the command `exit`」 |
| F6 | **cd** — `cd(1)`, *F5 TMSH Reference v17* | `https://clouddocs.f5.com/cli/tmsh-reference/latest/commands/cd.html` | v17 | 全文：「`/` is the root folder」「**`/Common` is the default folder for creating new configurations objects**」；`cd /Common` 示例；文件夹名以 `/` 分隔；`pwd` 显示当前工作目录 |
| A1 | **TMSH** — `tmsh(1)`, *F5 TMSH Reference v14*（**仅辅助**） | `https://clouddocs.f5.com/cli/tmsh-reference/v14/general/tmsh.html` | v14（对应 14.1.0 资料族） | 仅用于交叉核对模块层级描述；**不据此宣称 17.x 已验证**，不作为本项目语法或模式的裁定依据 |

### 2.2 官方语义要点（不复制整篇文档）

| 主题 | 官方结论 | 依据 |
|---|---|---|
| 模块层级 | tmsh 分层模块化；root 为 `(tmos)#`，下级 `auth/cli/gtm/ltm/net/sys/wom`；进入 `ltm` 后提示符 `(tmos.ltm)#`；提示符显示当前模块位置 | F1 |
| 模块内命令 | 在 `(tmos.ltm)#` 下用 `list virtual` / `show pool` 等（组件名相对当前模块）；从 root 用 `list ltm pool`、`list /ltm pool` 或 `list / ltm pool`；`/` 表示相对 root 的完整路径 | F1、F2、F3 |
| `exit` vs `quit` | **`exit` 退出当前模块/组件、返回上一级**；**`quit` 退出整个 tmsh 回到 BIG-IP 系统提示符**。二者不是同义词 | F4、F5 |
| `list` vs `show` | `list` = 配置对象与属性；`show` = 统计与运行时状态 | F1、F2、F3 |
| 对象全路径 / 分区 | `/Common` 是创建新配置对象的**默认文件夹**；对象可用 `/Common/<name>` 全路径引用；`/` 为 root 文件夹 | F6、F2 (`mv /ltm pool /Common/my_pool`)、F3 |
| pool | `create pool <name>`、`modify pool <name> members add { <ip>:<port> }`、`list pool`、`show pool`、`delete pool <name>`；默认 `round-robin` | F2 |
| virtual | `create virtual <name> destination <ipv4:port> pool <pool_name>`、`list virtual`、`show virtual`、`delete virtual <name>`；`pool` 默认 `none` | F3 |
| 删除约束 | 删除 pool 前必须先移除所有对它的引用 | F2 |
| 首批分区承诺 | 只承诺 `/Common/` 及对象全路径的必要解析；其他 Partition / Route Domain 明确 `NotSimulated` | 主方案 §13(9)、交接 §4(4) |

### 2.3 证据边界

- 无带版本 BIG-IP / TMOS VE 设备样本：`list ltm pool` / `list ltm virtual` / `show ltm pool` 的**逐字输出**、tmsh 语法错误的逐字文本、健康监视器状态、统计字段一律列为「输出待验证」；本包只做结构 / 状态 / 数据面断言，不新增逐字厂商黄金。
- F1–F6 为 F5 Cloud Docs 官方 man page（v17），A1 为 v14 同族资料；**17.x 结论只引用 F1–F6**。
- tmsh 的 `{ ... }` 属性分组、花括号内 `address` 子属性、`profiles`/`monitor` 等完整选项未纳入首批；本包只实现官方 `SYNTAX` 中可直接逐 token 执行的子集，并在 §11 明确列出。

---

## 3. 独立方言 / 树 / Profile 边界

| 边界 | 实现 |
|---|---|
| 独立方言 | `sim-core/src/device.rs` 新增 `Dialect::Tmos`（序列化枚举第 4 变体）。`cli-dialects/src/lib.rs::trees()` 挂 `tmos::build()`；`std::ptr::eq` 断言四棵树互不相同 |
| 独立命令树 | `crates/cli-dialects/src/tmos.rs`（新增）：只含 tmsh 语法。`exec` = root 模块 `(tmos)#`；新增 `Trees::tmos_ltm` / `tmos_sys` 两个模式根（`Mode::TmosLtm` / `Mode::TmosSys`），`root_for()` / `roots_for()` 同步纳入稳定 ID 派生。三棵厂商树的这两根为空 |
| 抽象 `ltm` 移除 | `ios.rs` / `huawei.rs` / `comware.rs` 各删 config 根的 `ltm pool …` / `ltm virtual …` 子树与 exec 根的 `show ltm` / `display ltm`（共 6 处）。`derived(Ios/Vrp/Comware)` 断言不再出现 `ltm` 段 |
| Profile | `sim-core/src/profile.rs`：`Vendor::F5`、`TMOS_V17_BASELINE`、`TMOS_LB_PROFILE = "f5-bigip-tmos-17-loadbalancer"`、F1–F6 证据常量；`common_capabilities` / `profile_id_for` / `profile_for` / `profile_by_id` / Trunk 平台策略全部显式补 Tmos 分支。TMOS 只有 LoadBalancer 一种设备类型，不按 router/switch 拆分 |
| 模式与上下文 | `cli-engine/src/lib.rs`：`Mode::TmosLtm` / `Mode::TmosSys`（tag `tmos_ltm` / `tmos_sys`）；`Ctx::not_simulated` 供 handler 按**参数值**返回 `NotSimulated`（稳定 ID 无法区分分区名） |
| 提示符 / 报错 | `cli-dialects/src/lib.rs::prompt`：`(tmos)#` / `(tmos.ltm)#` / `(tmos.sys)#`，不沿用 `<...>` / `[...]`；`format_error` 用独立 `Syntax Error: …`（无设备样本，不冒充逐字厂商文本）；登录错误文案独立分支 |
| 接口名 / 输出 | `ifname.rs` 的 8 处穷尽方言分支按 TMOS 分叉（BIG-IP `1.<port>`）；`show.rs` 6 处穷尽 match 补中性 Tmos 分支；`config_render::render()` 对 Tmos 提前返回独立 `render_tmos()`，其余段落 Tmos 分支为显式 `unreachable!` |
| 规格机制 | `specs.rs` 新增 `tmos!` 宏与 **47 条 `Dialect::Tmos` SpecRow**：首批命令 `Implemented + DocumentationOnly + strict`，引用 F1–F6；`exec/cd/@0word` 与 `tmos_ltm/cd/@0word` 2 条 `NotSimulated + Unimplemented`（另由 `Ctx::not_simulated` 按参数值覆盖对象路径上的其他分区）。其余未接线节点（19 个）显式留在 `lint_report().unassociated` 迁移清单，不隐藏豁免。`lint_report(Tmos)`：`strict_ok()=true`，`duplicates/dangling/drift/strict_missing` 全空，`unassociated + rows == derived_total` |
| 状态唯一来源 | `sim-core` 的 `LtmConfig / LtmPool / LtmMember / LtmVirtual` 与 `Engine::ltm_translate` / `ltm_owns_vip` / `ltm_sessions_of` 原样复用；tmos handler 只读写这些字段，未建平行数据面或第二套配置 |
| WASM / Web | `sim-wasm`：`dialect_from` 接受 `Tmos/tmos/f5/bigip`、`mode_from` 接受 `TmosLtm/TmosSys`、`verify_command` 加 Tmos、`set_port_ip` 新增（拓扑预配置）；`web/src/lib/sim.ts` 的 `Dialect` 联合加 `'Tmos'`；`LeftPanel.svelte` 新增/编辑下拉加第 4 项；`cmdmap.ts` 扩为四平台列；`CourseView.svelte` `VENDORS` 加 F5 列；`i18n-dict.ts` 中英加 `course.tmos`、状态栏改四方言 |

**共享 / 独立边界**：`CNode` 仍是语法、帮助、补全与稳定 ID 的唯一来源；四棵树共享 `cli-engine` 解析器、`ParamKind` 参数工具、`specs` 规格机制、`judge()` 四态映射、`sim-core` 唯一状态与 `ifname/show/prompt/format_error` 的方言分支。`Tmos` 没有复制任何 IOS/VRP/Comware 语法，也不复用其提示符或报错格式。

**未模拟命令的处理**：GTM / ASM / APM / iRule 等模块与对象**未挂载**在 tmsh 树中（输入得到 `InvalidCommand`，不会被判为「识别但未模拟」或「完整支持」）；其他 Partition / Route Domain（如 `create ltm pool /TenantA/WEB`、`cd /TenantA%1`）通过 `Ctx::not_simulated` 显式返回 `NotSimulated`，设备输出为空、状态不变、只有模拟器提示。首批只承诺 `/Common/` 及对象全路径的必要解析。

---

## 4. tmsh 层级与 LTM 首批语义验收

| 验收项 | 实现与断言 | 证据 |
|---|---|---|
| root / 模块导航 | `(tmos)# ltm` → `(tmos.ltm)#`；`(tmos)# sys` → `(tmos.sys)#`；模块内组件名相对当前模块（`create pool WEB`）；root 下用完整路径（`create ltm pool …` / `list ltm pool /Common/WEB`） | F1 |
| `exit` vs `quit` | **`exit` 只退一级**（`(tmos.ltm)# → (tmos)#`）；**`quit` 从任何模块直接回到 `(tmos)#`**。两者不是同义词；Rust 与浏览器均断言三种提示符出现 | F4、F5 |
| `cd /Common` / `pwd` | `cd /Common` 默认文件夹、`pwd` 返回 `/Common`；`cd /TenantA` → `NotSimulated` | F6、主方案 §13(9) |
| `list` vs `show` | `list` 输出配置对象（池/成员/VS 属性），`show` 追加运行时 `Sessions:` 段；输出来自唯一状态源，只做结构断言，不建逐字黄金 | F1、F2、F3 |
| pool CRUD | `create pool WEB` / `modify pool WEB members add { <ip>:<port> }` / `members delete { … }` / `delete pool WEB` / `list pool /Common/WEB`；删除被 VS 引用的池被拒 | F2 |
| virtual CRUD | `create virtual VS destination 10.0.0.100:80 pool WEB` / `delete virtual VS` / `list virtual /Common/VS` | F3 |
| `/Common/` 全路径 | 对象名可为裸名（默认 `/Common`）或 `/Common/<name>`；`obj_name()` 统一规范化，状态层只存裸名 | F6、F2 `mv /ltm pool /Common/my_pool` |
| 负向路径 | 引用不存在的 pool（`Pool not found`）、Member 重复（`Pool member already exists`）、非法 IPv4（`10.0.1.999:80`）、非法端口（`:70000` / `:0`）、缺少 `:port`、向不存在的池加成员、同名 VS 重复创建、被引用池删除 —— 全部 `InvalidCommand` 且状态指纹不变 | F2、F3 |
| 错误模式 / 参数 | 上述负向用例经 `exec_line_result` 断言 `status=InvalidCommand`、`state_changed=false`、`state_fingerprint` 前后相同；设备输出不含 `simulator/netemu` 措辞 | 主方案 §4.4 |
| 配置渲染重放 | `config_render::render_tmos()` 输出 root 模块完整路径序列（`create ltm pool /Common/WEB`、`modify ltm pool … members add { … }`、`create ltm virtual …`），逐行 `trim()` 重执行后末态 `(tmos)#`，池/成员/VS 数量与 VIP/端口完全一致；输出不含抽象 `ltm pool WEB member` / `ltm virtual VS vip` / `sysname` / `hostname` | ADR D12、F1–F3 |
| **Member 实际分配** | 4 个真实 TCP 会话每次 `连接成功`，`ltm_sessions_of()` 至少 4 条，`member_ip` 去重后 ≥2 且同时含 `10.0.1.10` 与 `10.0.2.11`；主动判分项 `ltmDistribution` 再执行 3 次会话并断言「3 个 TCP 会话 · 2 个不同 Member」 | 主方案 §10.2 / §13(6) |
| 数据面 | `ltm_translate` 的轮询分配、VIP→Member DNAT、回程 SNAT 与 VIP ARP 应答沿用 `sim-core` 既有实现，未改动 | 主方案 §13(4) |

---

## 5. 课程原文三层测试映射（与方言同一变更集）

### 5.1 课程与对照表去抽象化

`web/src/lib/labs.ts` 的 **`lab19` / `id:'f5-ltm'`**（第 14 课 F5 LTM，非变量 `lab14`）与 `web/src/content/labs-en.ts` 新增 `'f5-ltm'` 条目，中英文前三步是**严格可顺序执行**的同一条 12 条 tmsh 序列：

```
ltm
create pool WEB
modify pool WEB members add { 10.0.1.10:80 }
modify pool WEB members add { 10.0.2.11:80 }
create virtual VS destination 10.0.0.100:80 pool WEB
list pool /Common/WEB
list virtual /Common/VS
exit
sys
list version
exit
quit
```

第 4 步为主机网关（属性面板）、第 5 步为 PC1 三次 `tcp` 与 `show ltm pool` 观察说明，均不进入 CLI 序列。课程原文不再出现 `ltm pool WEB member …` / `ltm virtual VS vip …` / `display ltm` 等跨厂商抽象命令；`lessons/14-f5-ltm.md` 与 `lessons-en/14-f5-ltm.md` 的代码块同步改为带 `(tmos)#` / `(tmos.ltm)#` 提示符的真实 tmsh。`lab23` / `dc-comprehensive` 的中英文 F5 任务与 `LoadBalancer` 设备方言同步改为 Tmos，避免残留抽象命令。

`web/src/content/cmdmap.ts` 的 ltm 段改为 **4 行**：IOS / VRP / Comware 三列全部为空（UI 显示「— 本平台无逐字对应命令」），新增第四列 `tmos` 给出真实命令：
`create ltm pool WEB members add { 10.0.1.10:80 }`、`create ltm virtual VS destination 10.0.0.100:80 pool WEB`、`list ltm pool /Common/WEB`、`list ltm virtual /Common/VS`。`CmdVendor` / `PROFILE_IDS` / `CmdRow` / `rowMode` / `profileForRow` 全部扩为四平台；`CourseView.svelte` 的 `VENDORS` 与表头同理。

### 5.2 三层测试（均从课程原文解析，不维护手写验收清单）

| 层 | 位置 | 断言 |
|---|---|---|
| Rust 集成 | `crates/cli-dialects/tests/ltm_golden.rs::tmos_f5_lab_course_original_sequence_runs_top_to_bottom` | 从 `labs.ts` / `labs-en.ts` 解析，中英序列相等、12 条、逐条 `Executed`、三种提示符出现；末态池/成员/VS 与 VIP 正确；随后三次 TCP 会话落到不同 Member |
| WASM 契约 | `crates/sim-wasm/src/stage5a_tests.rs::stage5a_f5_lab_course_original_sequence_runs_on_real_wasm_sim` | 同一解析规则在真实 `Sim` 上逐条执行 0 失败；`prompt()` trace 含 `(tmos)#` / `(tmos.ltm)#` / `(tmos.sys)#`；纯读前后 `dump()` / `stats()` 不变；主动判分 2/2 |
| 真实浏览器 | `web/dist/stage5a-verify.js`（验收后删除）+ `playwright-cli run-code` | 从 DOM `ol.task li` 解析中文 12 条与英文 12 条，各 0 失败且序列相等；tmsh 三层提示符；纯读/主动判分；双通道；控制台 0 错误 |

三层共用同一套解析规则（去括号附注 → 只在**第一个箭头之前**且后跟空格处识别标签冒号 → 按 `→` / `->` 切分）。阶段 5A 同步修正了 `stage4a_comware.rs` 与 `sim-wasm/src/stage4a_tests.rs` 的 `task_cli`：原实现用全局第一个 `:`，会把 tmsh 的 `destination 10.0.0.100:80` 误判为「标签: 正文」；Stage 4A 的 19 条序列在新规则下仍全部通过。

### 5.3 检查项与完成门禁

`lab19.checks` = 1 个 `ltmVirtual` **纯读**（VS = 10.0.0.100:80）+ 1 个新增 `ltmDistribution` **主动**（3 个 TCP 会话、至少 2 个不同 Member）。`lab-checks` 的 `behavior()` 把新项归入 `ActiveExperiment`，纯读路径跳过；Web 侧 `checkBehavior()` / `labActiveBehaviors()` 同步归类为 `tcp`，主动判分前明示「发送 TCP 连接探测」。浏览器实测：纯读 `good,skip` 且 `labPassed['f5-ltm']=false`；主动 `good=2` 且 `labPassed=true`。

---

## 6. 旧断言替换逐项说明

| 位置 | 旧断言 | 替换 / 处置 |
|---|---|---|
| `ltm_golden.rs::ltm_commands_parse_in_all_dialects` | 7 条抽象 `ltm` 命令在 IOS/VRP/Comware 上「所有方言均可解析」 | **删除**，替换为 `tmos_tree_is_independent_and_abstract_ltm_is_removed_from_three_vendor_trees`：四棵树 `ptr::eq` 互异；4 条抽象命令在 3 个方言 × 2 模式全部被拒；5 条 tmsh 命令仅 Tmos 可解析；三棵厂商树的 derived ID 中无 `ltm` 段 |
| `ltm_golden.rs::ltm_tcp_load_balances_and_translates_return` | VRP 抽象命令 + 两次 TCP 成功 + `display ltm` | 保留并增强为 4 个数据面/CRUD/配置渲染测试：拓扑改用 `Dialect::Tmos`，4 次真实 TCP 会话 + 成员去重断言（≥2 个不同 Member）、`show pool` 会话段、`config_render` 重放 |
| `ltm_golden.rs` 新增 | — | `tmos_spec_lint_is_clean_and_layered`、`tmsh_module_navigation_prompts_and_exit_quit_hierarchy`、`tmos_non_common_partition_is_not_simulated_and_stateless`、`tmos_pool_virtual_crud_lists_and_negative_paths`、`tmos_config_render_is_tmsh_replayable`、课程原文执行测试，共 8 个测试（原 2 个） |
| `stage3a_cmdmap.rs::rows()` | 固定三厂商 tuple | 加第 4 个 `("tmos","tmos","modeTmos")`，**所有非空单元格（含 F5 列）继续参与 `REJECTED=0` 硬断言**；`profiles()` 加 tmos 映射 |
| `stage3a_cmdmap.rs::stage3a_cmdmap_probe_rejected_rows` | `rejected == 0` | 未放宽；实测 `TOTAL_ROWS=433 REJECTED=0`（ltm 段由 3 行三厂商 9 个单元格改为 4 行各 1 个 tmos 单元格） |
| `stage4a_comware.rs::task_cli` / `sim-wasm/src/stage4a_tests.rs::task_cli` | 全局第一个 `:` 当标签 | 改为只在第一个箭头之前、且后跟空格的标签冒号；Stage 4A 19 条序列回归通过 |
| `cli-dialects/src/{ios,huawei,comware}.rs` | 各 1 处 config `ltm` 子树 + 1 处 exec `show/display ltm` | 全部移除（6 处），`Trees` 补 `tmos_ltm/tmos_sys: vec![]` |
| `cmd::ltm_pool_member` / `ltm_virtual` / `show_ltm` | 三树共享的抽象 handler | 保留在 `cmd`（未接线，不删既有代码）；`config_render` 不再输出抽象 LTM 行；`show::ltm_table` 保留但无树节点引用 |
| 全部 Stage 2/3/4 既有测试 | 291 项 | 全量复跑 **300 项 / 48 二进制 0 失败**，未删测、未注释、未放宽断言 |

---

## 7. 旧 F5 存档迁移

- `sim-wasm/src/lib.rs::apply_summary_checked()`：对 `DeviceKind::LoadBalancer && dialect != Tmos` 的旧档，把 `dialect` 改为 `Dialect::Tmos`、`profile` 设为 `TMOS_LB_PROFILE`，计入 `mapped_profiles`；Pool / Virtual Server 状态不做任何转换，由 `Net::apply_summary` 从唯一状态源原样还原。
- 新增 `F5_LEGACY_NOTICE`，并在 `LoadReport.notices` 中**独立于 `legacy` 标记**追加总提示与逐设备提示（`旧 F5 设备 LB1(id=…) → Dialect::Tmos / Profile f5-bigip-tmos-17-loadbalancer`），明确说明「旧的抽象 ltm 命令不再可解析，请按课程原文改用真实 tmsh 命令」。加载成功后既有 `sessions` / `session_ids` 清空行为不变，并继续提示「加载将重置所有终端会话」。
- UI 侧延续既有可见通道：`main.ts` 的 `showLoadReport` → `setLoadNotices(report.notices)` → `LeftPanel.svelte` 的 `takeLoadNotices()`；文件导入时 `topoMsg = [导入成功, ...report.notices]`。`persistence.ts::normalizeWorkspace()` 只补 editor 字段，不改 net/dialect，避免前端静默改变判定。
- `stage5a_tests.rs::stage5a_legacy_f5_save_is_migrated_with_visible_notices_and_state_preserved`：用当前 Tmos 拓扑生成真实存档 → 把 LB 退回 `Vrp` + 旧 Profile → 加载 → 断言 `dialect == "Tmos"`、Profile ID 正确、Pool 的 2 个成员与 VS 的 VIP 原样保留、notices 含三类提示、旧会话令牌失效（`no active device session`）、新令牌下 `list ltm pool /Common/WEB` 为 `full-support`。

---

## 8. 真实 WASM / 浏览器证据

方式：`playwright-cli` + `vite preview @ localhost:4173`，消费**本次** `wasm-pack build` + `vite build` 生成的 `net_sim_bg-CMGy9_GD.wasm`（浏览器 `performance` 资源条目实测）。全部结论来自页内真实 WASM API / 真实 UI 点击，**未使用 TypeScript 假结果或声明桩**。临时脚本 `web/dist/stage5a-verify.js` 验收后删除；`.playwright-cli/` 快照已清理。每次运行先 `localStorage.clear()` 并断言中英文界面标记，避免语言持久化造成假绿。

| 验收点 | 最终实测结果 |
|---|---|
| 实验设备 | `PC1(Host/Ios)`、`LB1(LoadBalancer/**Tmos** / f5-bigip-tmos-17-loadbalancer)`、`SRV1/SRV2(Host/Ios)` |
| 课程原文（中文） | DOM 解析 12 条，逐条 `Executed`，`failures=0`；提示符出现 `(tmos)#` / `(tmos.ltm)#` / `(tmos.sys)#` |
| 课程原文（英文） | 切换语言后重新加载实验，DOM 解析 12 条、`failures=0`，`sequenceEqual=true` |
| tmsh 层级 | 三种官方格式提示符实测；`exit` 退一级、`quit` 回 root 由 Rust 与 WASM 断言 |
| 纯读不变 | UI「纯读检查」→ `good,skip`；`dump()` / `stats()` 前后完全一致；`labPassed['f5-ltm']=false` |
| 主动判分 / Member 分配 | UI「运行实验判分」→ `good=2`、`labPassed=true`；明细 `3 个 TCP 会话 · 2 个不同 Member · 10.0.1.10, 10.0.2.11` |
| 双通道 | `list pool /Common/WEB`：`Executed`、`deviceOutputLen=130`、`simulatorNotices=[]`；`create pool /TenantA/WEB`：`NotSimulated`、`deviceOutput=""`、2 条模拟器提示；旧文本入口 `[netemu]` 提示保留且不混入设备输出。`TerminalPanel.svelte` 单一渲染静态门禁（Stage 4A）未改动 |
| 会话级纯读 inspection | `session_id(LB1)` → `verify_device_command(..., "list pool /Common/WEB")` = `full-support`；错误令牌 `session id mismatch`；前后 `dump()` / `stats()` 不变 |
| ltm 课程表 | **4 行 × 5 列**（功能 / 思科 IOS / 华为 VRP / 华三 Comware / **F5 TMOS 17.x**）；F5 列 4 行 `✓ 完整支持 / Executed / DocumentationOnly`；三厂商列各 4 行「— 本平台无逐字对应命令」 |
| 控制台 | 全流程新增错误 **0**（`consoleErrors=0`，无 pageerror） |

**截图**（`docs/cli-fidelity/`）：

| 文件 | 内容 |
|---|---|
| `stage5a-f5-ltm-active.png` | F5 实验纯读/主动两个按钮 + 2/2 全绿判分（含 Member 分配明细）+ 拓扑；状态栏「四方言 CLI」、方言按钮「F5 TMOS 17.x」 |
| `stage5a-ltm-course.png` | 第 14 课 F5 LTM 的四平台对照表：三厂商列「无逐字对应」，F5 TMOS 列 4 行完整支持及 evidence/specId |

**覆盖边界（如实说明）**：截图为单页流程证据，判分结果与课程表需滚动 `.left-panel` / `.reader` 才完整可见；只在中文界面抓屏，但**英文课程原文已在真实浏览器 DOM 上解析并逐条执行**（不只依赖 Rust `include_str!`）；未逐课程、逐终端窗口或逐语言抓屏。终端 xterm 的逐行 DOM 渲染未逐帧抓屏，改由 `TerminalPanel.svelte` 单一渲染静态门禁 + 浏览器结构化双通道结果共同约束。最终六项门禁复跑只改了两处测试文件的 clippy 表述，WASM 产物名与字节大小不变，故上述结论对最终交付产物有效。

---

## 9. 六项门禁（最终真实结果）

| 门禁 | 退出码 | 结果 |
|---|---|---|
| `cargo fmt --all -- --check` | **0** | 先 `cargo fmt --all` 统一新增文件，复跑 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | **0** | 修掉 2 处 `clippy::len_zero`（新 WASM 契约测试断言），无 `#[allow]` 规避 |
| `cargo test --workspace` | **0** | **300 passed / 0 failed / 0 ignored，48 个测试二进制**（基线 291 → `ltm_golden.rs` 由 2 测试替换为 8 测试 + WASM `stage5a_tests.rs` 3 测试 + 删除临时 ID 探针）；课程表 `TOTAL_ROWS=433 REJECTED=0` |
| `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | **0** | 见 9.1 |
| `npx --yes pnpm@10 --dir web run check` | **0** | `tsc --noEmit` 0 错误 |
| `npx --yes pnpm@10 --dir web run build` | **0** | 消费 `net_sim_bg-CMGy9_GD.wasm`（1,633.02 kB，gzip 568.75 kB）；`✓ built in 12.28s` |

### 9.1 WASM 总门禁逐段

| 段 | 结果 |
|---|---|
| Rust → WASM 编译 | ✅ `release` profile，基于当前源码（最终 30.51s） |
| wasm-bindgen 声明 | ✅ `net_sim.js` / `net_sim.d.ts` / `net_sim_bg.wasm.d.ts` / `package.json` 重新生成（新增 `set_port_ip` 声明） |
| wasm-opt | ✅ `[INFO]: Optimizing wasm binaries with wasm-opt`，本机**未复现** `拒绝访问 (os error 5)`；**未关闭优化、未改配置换绿** |
| WASM 总门禁 | ✅ `:-) Done in 45.02s`，退出 0 |
| Web 结果 | ✅ `check` / `build` 均 0；浏览器实测资源名 `net_sim_bg-CMGy9_GD.wasm`，控制台 0 消息 |

| 层 | 数量 / 结果 |
|---|---|
| Rust 单元 + 集成 + doctest | 300 passed / 0 failed（阶段 5A 净新增 9） |
| 浏览器真实 CLI | 中文 12 条 + 英文 12 条课程原文逐条执行（非手写清单），`failures=0`；三种 tmsh 提示符 |
| 浏览器真实检查 | 纯读 1 通过 + 1 跳过且状态/时间/统计不变；主动 2/2；3 个 TCP 会话落到 2 个不同 Member |
| 课程表 | 433 个非空单元格（含第 4 列 F5），`REJECTED=0` 且为断言；ltm 段 F5 列 4 行完整支持 |

---

## 10. HEAD、工作区、未完成项与阻塞

| 项 | 状态 |
|---|---|
| HEAD | `3d784e022c8e70ca3c4aafbce65edd1220064ed7`（未变） |
| 分支 | **detached HEAD（no branch）**，`git branch --show-current` 为空 |
| 提交 / 推送 | **未提交、未推送** |
| 工作区 | **32 个已跟踪 M + 21 组未跟踪**（起点 29 M + 19 组；本包新增 `ltm_golden.rs`、两份 `14-f5-ltm.md` 讲义 3 个 M，`tmos.rs` / `stage5a_tests.rs` 2 组未跟踪，以及 `docs/cli-fidelity/` 内本记录与 2 张截图）。起点改动全部保留 |
| 构建目录 | 独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-5a-target`；未使用 `-j 1`（本机无编译挂起） |
| 临时产物 | `web/dist/stage5a-verify.js` 与临时 ID 探针 `stage5a_ids.rs` 已删除；`.playwright-cli/` 已清理；preview 服务与浏览器已关闭 |

**未完成项（明确后置，未开始）**

| 项 | 边界 |
|---|---|
| TMOS 全功能（GTM / ASM / APM / iRule / 健康监视器 / profile / persistence / SNAT 策略等） | 未挂载到 tmsh 树；输入得到 `InvalidCommand`，不显示完整支持。首批只实现 ltm/sys 模块导航与 pool / member / virtual 的最小闭环 |
| `net self` / VLAN / Route Domain 对象 | 未实现；F5 实验的 LB 接口地址通过新增 WASM `set_port_ip` 作为**实验拓扑预配置**（属性面板语义），不属于课程 tmsh CLI 任务 |
| 其他 Partition / Route Domain | 显式 `NotSimulated`（非 `/Common/` 路径、含 `%` 的域名），不静默改判 |
| `quit` 的 BIG-IP 系统 shell 层 | 模拟器只有 tmsh root 一个 shell 层，`quit` 统一回到 `(tmos)#`；没有伪造 Linux shell 提示符 |
| 逐字输出黄金 / 真机校准 | 无带版本 BIG-IP / TMOS VE 样本；`list` / `show` 与报错文本只做结构断言，证据等级保持 `DocumentationOnly` |
| `create pool … members add { … }` 的多成员花括号列表、`modify virtual` 属性组 | 首批按单成员逐 token 实现；完整花括号属性组和 `modify virtual` 属性修改留在 lint 迁移清单（部分标 `NotSimulated`） |
| C6b、阶段 6 防火墙（ASA/VRP 拆分、完整 IOS ZBF）、Comware 非首批族、`run()` 前端事件动画 | 本包明确不做 |

**阻塞**：无功能阻塞。环境事项：共享 target 指纹权限冲突（独立目录规避）；交接文档提到的 `wasm-opt os error 5` 与 `cargo fmt` 子进程挂起在本机均未复现；preview 服务绑定 IPv6 `localhost`，故用 `http://localhost:4173/`。

---

## 11. 阶段 5 是否完成的明确结论

**结论：主方案 §15「阶段 5：F5 TMOS」在本交接包范围内完成并验收通过，可以关闭；但不等于 TMOS 全功能完成。**

依据：
1. 开工前补齐 **F5 TMSH Reference v17** 官方正文证据 F1–F6（模块层级、`exit`/`quit` 语义、`ltm pool` / `ltm virtual` 语法、`/Common/` 默认文件夹），14.1.0 资料只作辅助、不据此宣称 17.x 验证；无设备样本，未建立逐字输出黄金。
2. 新增独立 `Dialect::Tmos`、F5 TMOS 17.x Profile 与稳定 ID、独立 `tmos.rs` 命令树；`prompt` / 报错 / `config_render` / `show` / `ifname` 全部按新方言分叉；**同时从 IOS/VRP/Comware 三棵树移除全部抽象 `ltm` 命令**，逐方言接受/拒绝矩阵断言通过。
3. tmsh 模块层级（root / ltm / sys、`exit` 与 `quit` 区别）、pool / member / virtual 的创建/添加/删除/查询、全部负向路径、`NotSimulated` 分区、`list`/`show` 与唯一状态源一致、tmsh 风格配置渲染可逐行重放——均有 Rust 断言。
4. **Member 实际分配**：4 次真实 TCP 会话 + 主动判分 3 次会话，均断言落到至少 2 个不同 Member，不再是「只验证一两次连接成功」。
5. 课程、讲义、对照表与 F5 实验在同一变更集去抽象化；中英文课程原文各 12 条命令在 **Rust 集成 / WASM 契约 / 真实浏览器** 三层从原文解析并逐条执行 `0` 失败；纯读不推进时间、主动判分明示行为、只有完整主动判分标记完成。
6. 旧「所有方言均可解析」断言被替换而非共存；旧 F5 存档映射到 Tmos 并保留 Pool/VS 状态，加载提示与会话清空纳入测试，不静默改变判定。
7. 六项门禁全绿（300 Rust 测试 / 48 二进制、WASM 逐段全绿、Web check/build 全绿），真实浏览器消费最终 WASM，控制台 0 新增错误，两张截图留存；Stage 2/3/4 的 IOS/VRP/Comware 行为未回归。

**收口边界**：§10 列出的 TMOS 全功能、`net self`/VLAN 对象、其他 Partition、真机校准与逐字输出黄金仍属后续工作；本包未进入 C6b、阶段 6 防火墙或 Comware 非首批族扩展。本包在上表范围内停止。
