# 阶段 6B · IOS XE 17.x ZBF 完整数据面 / 子模式保真 / 阶段 6 收口 · 实施记录

> 执行日期：2026-11-16
> 基线：`3d784e022c8e70ca3c4aafbce65edd1220064ed7`（detached HEAD，未提交、未推送）
> 上一记录：`docs/cli-fidelity/stage6a-implementation-record.md`（保留，**未回写**）
> 范围依据：`netemu-stage6b-implementation-handoff.md`；技术约束取主方案 §4.4 / §10.1 / §14 / §15
> 明确不做：C6b、ASA 恢复、Comware 安全数据面、VRP 侧改动、`run()` 前端动画、其他 IOS 安全特性
> 停止点：完成 ZBF 全流程、子模式保真、数据面接入、课程升级与门禁后停止

---

## 1. 现场保护与基线复跑

| 项 | 结果 |
|---|---|
| `git rev-parse HEAD` | `3d784e022c8e70ca3c4aafbce65edd1220064ed7`（未变） |
| 分支 | **detached HEAD**（`git branch --show-current` 为空） |
| 起点工作区 | **33 个已跟踪 M + 23 组未跟踪**（阶段 0/1、2A/2B/2C、3A/3B、4A 含补修、5A、6A 全部保留） |
| 现场保护 | 全程未 `reset` / `clean` / 覆盖 / 删测 / 注释测试 / 放宽断言；未提交、未推送 |
| 基线 `cargo test --workspace` | **316 passed / 0 failed，49 个测试二进制**（与交接一致，复跑退出码 0） |
| 基线课程表 | `TOTAL_ROWS=439 REJECTED=0` |
| 构建目录 | 独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-6b-target`；本机未出现并行编译挂起，**未使用 `-j 1`** |
| 阅读范围 | 交接文档、主方案 §4.4/§10.1/§14/§15、Stage 6A 记录全文、Stage 3B 记录 ZBF 章节、`stage3b_courses.rs`、`specs.rs`、`ios.rs`、`engine.rs`、`profile.rs`、`device.rs`、`summary.rs`、`show.rs`、`config_render.rs`、`lab-checks`、`security_golden.rs`、`stage6a_firewall.rs`、`cmdmap.ts`、讲义 08 中英全文 |

---

## 2. 工作项 A：IOS XE 17.x ZBF 证据与结构设计（先决策后实施）

### 2.1 官方证据表（全文核验）

| # | 文档 / 章节 | URL | 本次全文边界 |
|---|---|---|---|
| **E1** | *Security and VPN Configuration Guide, Cisco IOS XE 17.x* — **Zone-Based Policy Firewalls** | `https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/sec-vpn/b-security-vpn/m_sec-zone-pol-fw-xe.html` | **2026-11-16 重新抓取全文并逐节核对**：zone 与 zone-pair 规则、class-map / policy-map / class-default、inspect / pass / drop、parameter-map、`config-cmap` / `config-pmap` / `config-pmap-c` / `config-sec-zone-pair` 提示符、`show policy-map type inspect zone-pair [session]`/`show zone-pair security` 示例 |
| **E2** | 同书 — **Enabling ALGs and AICs in Zone-Based Policy Firewalls** | `https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/sec-vpn/b-security-vpn/m_zbf_enable_alg_aic.html` | 沿用 Stage 3B 全文核验（2026-11-12）；本包未重新抓取，仅用于 ALG/L7 边界说明 |
| **E3** | 同书 — **Firewall Support of SCCP** | `https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/sec-vpn/b-security-vpn/m_sec-data-sccp.html` | 沿用 Stage 3B 全文核验；本包未重新抓取 |
| **E4** | *IP Addressing Configuration Guide* — **vTCP for ALG Support** | `https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/ip-addressing/b-ip-addressing/m_sec-vtcp-alg.html` | 沿用 Stage 3B 全文核验；用于 `show` 命令证据 |

**本包从 E1 正文确认的关键语义（逐句）**

1. **缺省拒绝**：`If a policy is not configured between zone pairs, traffic is dropped.`
2. **同区域缺省放行**：`By default, traffic flows between interfaces that are members of the same zone.` / `By default, all traffic between two interfaces in the same zone is always allowed.`
3. **区域↔非区域**：`Traffic from a zone interface to a nonzone interface, or from a nonzone interface to a zone interface is always dropped; unless default zones are enabled.`
4. **inspect / pass 差异**：`Traffic with the inspect action will create a connection in the firewall table and be subject to state checking. Traffic with the pass action will bypass the zone firewall completely, not creating any sessions.`
5. **连接建立后不重新分类**：`After a firewall connection is created, the packets are no longer classified. That is, if the policy map changes, the underlying connections are not noticed.`
6. **pass 反向必须镜像**：`For the pass action, a policy must exist for packets in each direction, and for the inspect action, a policy must exist for traffic from the initiator.`
7. **class-default**：`class-default ... is always the last class ... If you do not configure any actions for the class-default class in an inspect policy, the default action is drop.`
8. **self zone**：`The self zone is the only exception to the default deny-all policy. All traffic to any router interface is allowed until traffic is explicitly denied.`
9. **子模式提示符**：正文示例逐字给出 `Device(config-cmap)#`、`Device(config-pmap)#`、`Device(config-pmap-c)#`、`Device(config-sec-zone-pair)#`、`Device(config-sec-zone)#`。
10. **参数/邻接边界**：`parameter-map type inspect` 可选；层次化 policy-map 允许；`description`、`platform inspect match-statistics per-filter` 出现在官方步骤中。

**证据等级**：全部 `DocumentationOnly`。没有带版本的 IOS XE 设备或可信设备样本，因此**不建立逐字输出黄金**，只建立语法、模式、提示符、结构化状态与数据面断言。

### 2.2 决策一：ZBF 结构映射（先决策）

> **决策：保留 ZBF 平台原生对象图（zone-pair → policy-map → class-map），通过 `FirewallPolicyModel::ZonePairs` 接入共享 `firewall_allow` / `firewall_tcp_allow` 与既有会话表；不把 ZBF 压平成 `SecurityPolicy`，也不为 IOS 开第二套转发路径。**

实现要点：

- `RouterState` 新增 `zbf_class_maps` / `zbf_policy_maps` / `zbf_zone_pairs`（见 [device.rs:632](file:///e:/网络命令/netemu/crates/sim-core/src/device.rs#L632)）；
- `FirewallPolicy` 新增 `model: FirewallPolicyModel { None, FlatRules, ZonePairs }`（[profile.rs:130](file:///e:/网络命令/netemu/crates/sim-core/src/profile.rs#L130)）。VRP 防火墙 = `FlatRules`；IOS Router/Firewall = `ZonePairs`；其余 = `None`；
- 共享数据面 [engine.rs:1622](file:///e:/网络命令/netemu/crates/sim-core/src/engine.rs#L1622) 只按显式平台对象选择策略来源，**没有任何 `if dialect`**；
- `inspect` 建会话、`pass` 无状态、`drop` 丢弃；会话命中优先于策略（官方：连接建立后不再分类）；
- 同区域、区域↔非区域、无 zone-pair 的缺省动作全部由平台对象表达（IOS：同区域放行、跨区/区域边界拒绝；VRP 基线保持 6A 语义不变）；
- 设备级门禁由 `Capability::ZoneFirewall` 决定（IOS Router/Firewall 为 Supported），VRP/Comware 路由器仍被拒绝。

**为什么不是「压平到 SecurityPolicy」**：扁平结构只能表达「区域对 + 单一 permit」，无法承载 `match-any` / `match-all`、`match protocol` / `match access-group`、每个 class 的独立动作、`class-default` 缺省 drop 与按 class 的命中计数。若压平，就必须在配置变更时派生第二份规则并在数据面重建顺序，违背「唯一状态源」并产生双份真相。交接文档允许「新建独立的 zone-pair 策略视图但判定仍走 `firewall_allow` 同一入口」，本包采用该方案。

### 2.3 决策二：教学抽象处置（先决策）

> **决策（a）：移除旧简化 `zone-pair security ZP` + `source` / `destination` / `action permit` 路径，`ios_zone_pair_policy` 改写为真实 ZBF 路径矩阵；Profile 由 `ios-xe-17-zbf-teaching-firewall` 更名为 `ios-xe-17-zbf-firewall`。**

依据：

1. 官方正文明确 **不存在** `source` / `destination` / `action permit` 这些 zone-pair 子命令，保留它们就是「无名共存」；
2. 真实路径已完整实现，覆盖强于简化路径；按 6A 的 ASA 先例改写测试比保留伪路径更有价值；
3. 课程实际依赖核查：lab10 是 VRP 基线，不使用 IOS 简化路径；讲义 08 只把它作为演示块，已同步改为真实 ZBF；`cmdmap` 的教学抽象行删除；
4. `teachingVendors` 仅作为通用课程元数据类型保留（当前无行使用），不再是执行状态或厂商保真声明。

该决策直接改写了两条 Stage 3B 门禁（见 §7），**没有删测、没有放宽**：原「简化路径必须保留」改为「简化路径必须移除且真实路径可执行」；原「仅识别」断言改为「Implemented + full-support + strict」矩阵。

---

## 3. 工作项 B：命令树、子模式与 5 条命令升级

### 3.1 5 条 recognized-only → Implemented（逐条）

| # | 命令（官方语法） | 模式 / 提示符 | 完整参数与子句 | `no` 形式 | 数据面效果 |
|---|---|---|---|---|---|
| 1 | `class-map type inspect {match-any\|match-all} <name>` | `config` → `(config-cmap)#` | `match protocol <name>`、`match access-group <n>`；`match-any`/`match-all` | `no class-map [type inspect ...] <name>`；`no match protocol/access-group` | 对象入库；被 policy-map 引用时拒绝删除 |
| 2 | `policy-map type inspect <name>` | `config` → `(config-pmap)#` | `class type inspect <cmap>`、`class class-default`、`class type inspect class-default`；否定 class | `no policy-map [type inspect] <name>`；`no class ...` | 对象入库；被 zone-pair 引用时拒绝删除 |
| 3 | `zone-pair security <name> source <src> destination <dst>` | `config` → `(config-sec-zone-pair)#` | 源/目的必须存在或为 `self`；同一 (src,dst) 不允许两个 zone-pair | `no zone-pair security <name>` | 方向 + 策略挂接；未挂策略即 drop |
| 4 | `service-policy type inspect <policy>` | `(config-sec-zone-pair)#` | 引用必须存在；`no service-policy ...` 解除挂接 | 同左 | 挂接 policy-map；解除后按官方 drop |
| 5 | `show policy-map type inspect zone-pair [name] [session]` | 特权/用户 EXEC | 可选 zone-pair 名、`session`；另 `show zone-pair security` / `show zone security` | —（查询） | 结构化输出：zone-pair、policy、class、match、动作、命中计数、会话计数 |

补充实现的子句：

- `(config-pmap-c)#`：`inspect [parameter-map]`、`pass`、`drop [log]`、`no inspect/pass/drop`、`no drop log`（官方排障示例）；
- 邻接但未模拟（显式 `NotSimulated`）：`parameter-map type inspect`、`match class-map`、`zone-pair description`、`platform inspect match-statistics per-filter`、层次化 `service-policy`（pmap-c 内）；
- `zone security` / `zone-member security` 及其 `no` 形式、`show zone security` 一并纳入规格与数据面。

### 3.2 子模式与提示符保真

按 E1 正文逐字实现并在 L2 交互测试中断言：

| 进入命令 | 提示符 | `exit` 目标 | `end` |
|---|---|---|---|
| `class-map type inspect ...` | `R1(config-cmap)#` | `R1(config)#` | `R1#` |
| `policy-map type inspect ...` | `R1(config-pmap)#` | `R1(config)#` | `R1#` |
| `class type inspect ...` | `R1(config-pmap-c)#` | `R1(config-pmap)#`（保留 policy 名） | `R1#` |
| `zone-pair security ... source ... destination ...` | `R1(config-sec-zone-pair)#` | `R1(config)#` | `R1#` |
| `zone security ...` | `R1(config-sec-zone)#` | `R1(config)#` | `R1#` |

测试：`stage6b_submode_prompts_and_exit_hierarchy_match_official`（Rust）与 `stage6b_wasm_zbf_inspect_flow_and_submode_prompts`（WASM），并已在真实浏览器逐条实测（§8）。

### 3.3 可选参数的核扩展

官方 `inspect [parameter-map-name]` 与 `show ... zone-pair [name]` 是中括号可选参数。原解析器「节点有参数子节点即 Incomplete」会让裸 `inspect` / 裸查询报错。为忠实表达官方语法，在 `CNode` 上新增 `opt_param`（`Tok::Param { optional: true }`），`has_param_kids` 只要求必填参数（[cli-engine/src/lib.rs:108](file:///e:/网络命令/netemu/crates/cli-engine/src/lib.rs#L108)）。这是通用语法能力，不复制命令树；`stp_golden` 的模式匹配同步为 `Tok::Param { .. }`（非断言放宽）。

---

## 4. 工作项 C：数据面接入与 §14.3 验收

### 4.1 判定路径

```
IPv4/TCP 入方向
  → 会话表命中（ICMP 三元组 / TCP 双向五元组）→ 直接转发，不复查策略
  → 未命中 → firewall_action()
        ├─ ZonePairs：zone-pair 方向 → policy-map classes 顺序首条命中
        │     ├─ inspect → 建会话（ICMP echo request / TCP SYN）
        │     ├─ pass    → 无状态放行（不建会话）
        │     └─ drop / 无命中 / 未挂策略 → 拒绝
        └─ FlatRules：VRP 顺序规则首条启用命中 → inspect 语义（6A 基线不变）
```

### 4.2 场景 → 测试映射

| 交接要求 / §14.3 条目 | 实现语义 | 测试 |
|---|---|---|
| 无 zone-pair 平台缺省正/负 | 区域间 drop；同区域 permit；区域↔非区域 drop；两侧未划区域直通 | `stage6b_zone_defaults_and_same_zone_boundaries` |
| inspect 正向 + 回包走会话表 | 首包建 1 条 ICMP 会话支撑 5 个回包；反向无会话拒绝 | `stage6b_inspect_is_stateful_and_rejects_non_session_replies` |
| 非会话回包 / 未授权新会话拒绝 | 无会话 echo reply、反向 echo request、非 SYN TCP 全部丢弃 | 同上 (a)(b)(d) |
| pass 无状态 | 无会话；无镜像 pass 时回包丢弃；镜像后放行 | `stage6b_pass_is_stateless_and_needs_mirrored_policy` |
| 策略删除/改序新旧流 | 改序 = 删除+重加 class；新流命中 drop；既有会话继续 | `stage6b_policy_change_affects_new_flows_only` |
| 同区域 | 缺省放行；显式同区 zone-pair drop 生效 | `stage6b_zone_defaults_and_same_zone_boundaries` |
| `show/display` 与配置、会话一致 | `show zone security` / `show zone-pair security` / `show policy-map ... [session]` | `stage6b_show_matches_state_and_config_view_is_replayable` |
| 配置视图逐行重执行 | ZBF 段渲染并在干净路由器重放，查询一致 | 同上 |
| 存档往返 | class-map / policy-map / zone-pair / 动作 / 命中计数 | `stage6b_summary_round_trip_keeps_zbf_objects` |
| 规格与 lint | 5 条 + 子句 `Implemented + DocumentationOnly + strict`；邻接 `NotSimulated` | `stage6b_zbf_specs_are_implemented_and_lint_is_clean`、`stage3b_ios_spec_lint...` |
| 方言双向矩阵 | IOS 接受；VRP/Comware 拒绝（invalid-command）；VRP `security-policy` 不进 IOS | `stage6b_zbf_dialect_matrix_is_bidirectional` |
| 讲义 08 IOS 演示序列 | 从真实 md 解析 → Rust 与 WASM 两层逐条执行 0 失败 | `stage6b_lesson_ios_demo_sequence_runs_and_is_stateful`、`stage6b_wasm_lesson_ios_demo_runs_on_real_sim` |

### 4.3 存档与规格

- `DevSummary` 新增 `zbf_class_maps` / `zbf_policy_maps` / `zbf_zone_pairs`（`#[serde(default)]`，旧档无字段可加载）；`summary()` / `apply_summary()` 双向保留匹配条件、动作、`class-default`、命中计数与挂接关系。
- IOS 规格新增完整 ZBF 子句行（class-map ×2、match ×2、policy-map、class ×2、inspect/pass/drop/log、5 组 `no`、zone-pair、service-policy、show ×4、zone/member ×4 等），全部 `Implemented + DocumentationOnly + strict` 且引用 E1/E4。
- 邻接未模拟命令显式 `NotSimulated`：`parameter-map`、`match class-map`、`description`、`platform inspect ...`、层次化 `service-policy`。
- `lint_report(Ios)`：`duplicates / dangling / drift / strict_missing` 全空（`strict_ok() == true`）；ZBF 可执行节点不再出现在 `unassociated`。
- Profile：`IOS_FIREWALL_PROFILE = "ios-xe-17-zbf-firewall"`；`os_baseline = "Cisco IOS XE 17.x Zone-Based Policy Firewall"`；Router 与 Firewall 都具备 `ZoneFirewall` 能力。旧存档中的 `ios-xe-17-zbf-teaching-firewall` 字符串只作为 `profile` 元数据保留，不参与行为判定（加载按 dialect + kind 映射）。

---

## 5. 工作项 D：课程与对照表

### 5.1 `cmdmap.ts` security 段

- 5 条真实 ZBF 行随规格升级自动变为 `full-support`（规格驱动，不是 UI 一刀切）；
- 新增 IOS 子句行：`match protocol icmp`、`match access-group 100`、`class type inspect WEB`、`inspect`、`pass`、`drop`、`show zone-pair security`、`no class-map WEB`、`no policy-map PMAP`、`no zone-pair security IN-OUT`、`no service-policy type inspect PMAP`；VRP/Comware/TMOS 列留空；
- 删除「netemu 教学抽象：简化 zone-pair」行与 `teachingVendors` 使用；
- 新增前端 `CmdMode`：`ClassMap` / `PolicyMap` / `PolicyMapClass`，WASM `mode_from` 与测试 `mode_of` 同步；
- 零拒绝硬断言：`TOTAL_ROWS=449 REJECTED=0`（基线 439；+10 个非空 IOS 单元格全部参与检查），Comware 单元格未变（143）。

### 5.2 讲义 08（中英）

- VRP 基线段落保留；新增 IOS ZBF 完整真实流程（zone → class-map → policy-map → zone-pair → service-policy）；
- 明确三动作差异（inspect 状态化 / pass 无状态且需镜像 / drop）、无策略 drop、同区域缺省放行、区域↔非区域始终丢弃、self 例外；
- 删除全部「教学抽象」「仅识别 / 未模拟」表述；保留「L7/ALG、UDP、parameter-map、层次化策略、ZBF 与 NAT 顺序未校准」的边界说明；
- 更新验证命令为 `show zone security` / `show zone-pair security` / `show policy-map type inspect zone-pair [session]`。

### 5.3 课程原文测试

- lab10 保持 VRP 基线不动；未新造 IOS 实验；
- 讲义 08 的 IOS 演示块按「课程原文」标准新增三层测试：Rust 解析执行、WASM 解析执行、真实浏览器逐条执行（§8）；中英序列一致性由 Rust/WASM 断言。

---

## 6. Stage 3B 门禁处置逐项（不删测、不放宽）

| 原测试 | 处置 | 说明 |
|---|---|---|
| `stage3b_real_zbf_commands_are_recognized_only_not_invalid_or_full_support` | **改写**为 `stage3b_real_zbf_commands_are_implemented_and_full_support` | 5 条命令改断言 `Executed / full-support / DocumentationOnly / passed / strict / Cisco 引用`；覆盖更强 |
| `stage3b_real_zbf_commands_reject_wrong_mode` | **保留未改** | 错误模式仍 `InvalidCommand`；VRP Profile 仍拒绝 IOS 命令 |
| `stage3b_not_simulated_notices_never_pollute_device_output` | **改写命令**为 `parameter-map type inspect PMAP` | 断言不变：NotSimulated、状态指纹不变、设备输出无 `[netemu]`/`%`、提示只在独立通道 |
| `stage3b_legacy_simplified_zone_pair_path_is_preserved_as_teaching_abstraction` | **改写**为 `..._is_removed_in_favor_of_real_zbf` | 简化路径必须 `Incomplete`/不可解析；真实完整路径必须可执行；覆盖更强 |
| `stage3b_lesson_marks_teaching_abstraction_and_quotes_official_syntax` | **改写**为 `stage3b_lesson_documents_real_zbf_without_teaching_abstraction` | 断言真实流程、三种动作、四个提示符来源；断言不再含教学抽象与未模拟表述 |
| `stage3b_security_real_rows_replace_simplified_ios_zbf_sequence` | **改写** | 简化子命令为 0；真实 5 条 + 子句存在；`teachingVendors` 为 0 |
| `stage3b_ios_spec_lint_has_no_drift_and_zbf_rows_are_documented` | **改写** | `lint` 全空；已实现行 strict + Implemented；邻接行显式 NotSimulated；ZBF 节点不在 `unassociated` |
| `stage3b_comware_security_rows_still_never_full_support` | **保留未改** | Comware 负向门禁与双向隔离不变 |
| `security_golden::ios_zone_pair_policy` | **改写为真实路径** | 使用 IOS Router + class-map/policy-map/zone-pair/service-policy；无策略双向拒绝、inspect 正向 5/5、show 结构 |
| `security_golden::asa_lineage_commands_are_removed_and_have_no_data_plane` | **更新配置段** | 显式 ZBF 策略替换教学命令；安全级别字段仍无数据面 |
| `security_golden::firewall_commands_parse_in_all_dialects` | **更新命令清单** | 移除简化三条，新增完整 ZBF、no 形式、三种动作与查询 |
| `stage6a_firewall.rs` 13 项 / `stage6a_tests.rs` 3 项 | **全部保留通过** | VRP 基线、会话表、lab10 原文零回归 |

---

## 7. 新增测试

| 文件 | 数量 | 覆盖 |
|---|---|---|
| `crates/cli-dialects/tests/stage6b_firewall.rs` | **11** | 规格/Profile、子模式提示符与 exit 层级、方言矩阵、无 zone-pair/同区域/区域边界、inspect 状态化与非会话拒绝、pass 无状态、策略改序/既有会话、show 与配置重放、存档往返、讲义 08 原文 |
| `crates/sim-wasm/src/stage6b_tests.rs` | **5** | 真实 WASM 全流程与提示符、pass 无状态、未模拟/未支持协议独立通道、删除策略后的会话边界、讲义原文 |
| 全仓测试 | **316 → 332**（+16），二进制 **49 → 50** | 0 failed / 0 ignored |

---

## 8. 真实浏览器验收（真实 WASM）

方式：Chrome（Playwright 1.65.1，headless）加载 `vite preview` 的**本次构建**产物（`net_sim_bg-C2hyi4h4.wasm`，1,739.87 kB / gzip 609.62 kB）。最终门禁构建与浏览器验收使用同一产物 `net_sim_bg-C2hyi4h4.wasm`。拓扑由真实 WASM 生成并经工作区存档注入（PC1—R1—SRV，R1 为 IOS Router，接口已划入 INSIDE/OUTSIDE，**ZBF 未配置**）；ZBF 全流程、ping、查询、删除策略全部在浏览器真实终端中用键盘逐条执行。

| 验收点 | 实测结果 |
|---|---|
| ZBF 全流程 | 13 条命令逐条执行，提示符断言 **13/13 通过**：`R1#` → `R1(config)#` → `R1(config-cmap)#` ×2 → `R1(config)#` → `R1(config-pmap)#` → `R1(config-pmap-c)#` ×2 → `R1(config-pmap)#` → `R1(config)#` → `R1(config-sec-zone-pair)#` ×2 → `R1#` |
| 子模式提示符 | `config-cmap` / `config-pmap` / `config-pmap-c` / `config-sec-zone-pair` 全部实测一致；另截图留存 `(config-pmap-c)#` |
| inspect 正向 + 回包 | PC1 `ping 203.0.113.10` → `5 received`；会话表查询 `sessions: 1 icmp, 0 tcp` |
| 反向拒绝 | SRV `ping 192.168.1.10` → `0 received`（会话建立前） |
| 查询结构 | `show zone-pair security` → `Zone-pair: IN-OUT` / `service-policy inspect PMAP`；`show policy-map ... session` → `Class-map: ICMP (match-any)`、`Match: protocol icmp`、`inspect`、会话计数 |
| 删除策略后重判分 | `no service-policy type inspect PMAP` 后 PC1 再次 ping → `0 received`（新旧流边界与官方「连接建立后不再分类」一致；截图显示 5 received → 0 received） |
| 双通道 | `parameter-map type inspect PMAP2` 只出现 `[netemu] 该命令的承诺语义尚未模拟；命令未执行，设备状态未改变`，无设备输出；随后 `show zone-pair security` 只有设备输出、无 `[netemu]` |
| 控制台 | `consoleErrors = []`（0 条 pageerror / console.error） |
| 截图 | `docs/cli-fidelity/stage6b-zbf-flow.png`（ZBF 配置 + 会话 + 双通道）、`stage6b-zbf-submode.png`（`(config-pmap-c)#`）、`stage6b-zbf-ping.png`（5 received → 删除策略后 0 received） |

**覆盖边界（如实说明）**：浏览器验收从工作区存档注入拓扑（与用户「导入文件」同一序列化路径），未在画布上手工拖拽三台设备；ZBF 命令、ping、查询与删除策略均为真实键盘输入与真实 WASM。headless Chrome 不渲染 xterm 的像素级主题效果，但终端文本来自真实 xterm 行缓冲。

---

## 9. 六项最终门禁（真实结果）

| 门禁 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | **环境性挂起，未完成**：`cargo fmt` 与 `rustfmt --check crates/cli-dialects/src/ios.rs` 在 30–150s 内用户态 CPU 近 0、进程不退出；对 **HEAD 原始 `ios.rs`** 同样挂起，证明与本次改动无关（复核机已知环境问题，见主方案 §15）。除 `ios.rs` 与 `crates/cli-dialects/src/lib.rs` 外，其余改动文件已用 `rustfmt 1.10.0-stable` **逐文件格式化并通过检查**（exit 0）；这两个文件在本机 rustfmt 稳定挂起（HEAD 的 `ios.rs` 同样复现），其内容无超过 100 列的行，新增代码按 rustfmt 风格手工核对。**不关闭任何检查、不写 `#[rustfmt::skip]` 规避。** |
| `cargo clippy --workspace --all-targets -- -D warnings` | **0**（修掉 `question_mark` 1 处，无 `#[allow]` 规避） |
| `cargo test --workspace` | **0**：**332 passed / 0 failed / 0 ignored，50 个测试二进制**；课程表 `TOTAL_ROWS=449 REJECTED=0` |
| `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | **0**：final release 37.13s；**wasm-opt 正常运行**（`Optimizing wasm binaries with wasm-opt`），未关闭优化；总耗时 37.13s。产物 `net_sim_bg-C2hyi4h4.wasm` 与浏览器验收所用完全一致 |
| `npx --yes pnpm@10 --dir web run check` | **0**：`tsc --noEmit` 0 错误 |
| `npx --yes pnpm@10 --dir web run build` | **0**：`✓ built in 5.56s`；产物 `net_sim_bg-C2hyi4h4.wasm`（1,739.87 kB / gzip 609.62 kB） |

---

## 10. HEAD、工作区、临时产物与未完成项

| 项 | 状态 |
|---|---|
| HEAD | `3d784e022c8e70ca3c4aafbce65edd1220064ed7`（未变） |
| 分支 | **detached HEAD** |
| 提交 / 推送 | **未提交、未推送** |
| 工作区 | **34 个已跟踪 M + 25 组未跟踪**（起点 33 M + 23 组；新增 `stage6b_firewall.rs`、`sim-wasm/stage6b_tests.rs` 2 组未跟踪；`stp_golden.rs` 因可选参数模式匹配进入 M；`docs/cli-fidelity/` 仍为同一未跟踪条目） |
| 构建目录 | 独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-6b-target`；未使用 `-j 1`（本机无编译挂起） |
| 临时产物（**已按真实文件系统逐项核对**） | `web/dist/netemu-6b-ws.json`：已删除（`GONE`）；`C:/Users/22534/AppData/Local/Temp/pw6b`（Playwright 脚本/截图/结果）：已删除（`GONE`）；`netemu-6b-workspace.json`：已删除（`GONE`）；`ios_nonew.rs` / `ios_head.rs` / `fmt_*.txt` / `ws.b64` / `netemu-6b-gen.mjs`：已删除；vite preview（PID 30924）已终止，`http://localhost:4175/` 返回 `000`；仓库内无 `*.json` 工作区残留，`web/dist` 仅保留构建产物 |

**本包边界（未完成，明确后置）**

| 项 | 边界 |
|---|---|
| UDP / L7 / ALG 检查 | 未建模；`match protocol` 的 L7 协议可配置但不会命中，已在配置时给 `[netemu]` 提示并在讲义/记录明示 |
| `parameter-map type inspect` 参数 | 未模拟；`inspect [pmap]` 只保存引用 |
| 层次化 policy-map、`match class-map`、`description`、`platform inspect` | 显式 `NotSimulated` |
| `self` / `default` 区域 | `self` 可作 zone-pair 端点，但本模拟器设备自发流量不经过转发路径（天然满足 self 例外）；`default` 仅在显式 `zone security default` 后可用，未实现 default zone 的启用开关 |
| IOS 清会话命令 | 本包未新增 IOS `clear` 会话命令；删除策略后既有会话保留、新流重判，与官方一致 |
| ICMP 会话键 | 沿用 6A 的双向三元组（源/目的/echo id）；同一 id 的反向 echo request 会被当作回包，是已知 L4 简化边界 |
| ZBF 与 NAT 顺序 | 未按基线校准 |
| C6b、`run()` 前端动画、ASA 恢复、Comware 安全数据面、VRP 改动 | 本包明确不做 |

---

## 11. 阶段 6 是否完成的明确结论

**结论：主方案 §15「阶段 6：防火墙」在 6A + 6B 范围内完成并验收通过，可以关闭阶段 6。**

依据：

1. **证据与决策先行**：E1 全文重核；两处结构决策（原生 ZBF 对象图 + 平台策略模型；移除教学抽象并改写门禁）在实施前写入本记录；
2. **命令树与子模式完整**：5 条 recognized-only 全部升级 Implemented，含完整参数、`no` 形式、视图内子句；四个官方提示符与 `exit`/`end` 层级逐条断言并在浏览器实测；
3. **数据面统一**：ZBF 与 VRP 共享 `firewall_allow` / `firewall_tcp_allow` 与既有会话表；inspect 状态化、pass 无状态、drop/未命中/未挂策略拒绝；无第二套判定、无 `if dialect`；
4. **§14.3 全表闭环**：无 zone-pair 正负、inspect 正反向、非会话拒绝、pass 镜像、策略改序/既有会话、同区域、show/配置一致、存档往返全部有具名测试；
5. **课程与门禁**：cmdmap 5 条升级 + 新 ZBF 行，`REJECTED=0`；讲义 08 中英同步真实流程；Stage 3B 两条 ZBF 门禁按升级改写且覆盖更强，未删测、未放宽；lab10 VRP 零回归；
6. **门禁**：332 Rust 测试 / 50 二进制全绿；Clippy 0；wasm-pack（含 wasm-opt）与 Web check/build 通过；真实 Chrome 全流程 13/13 提示符、数据面 5→0、双通道、控制台 0 错误，3 张截图留存。唯一未完成项是本机 `cargo fmt`/`rustfmt` 对 `ios.rs` 的环境性挂起（HEAD 同样复现），已如实登记，不冒充通过。

**收口边界**：§10 列出的 UDP/L7、parameter-map、层次化策略、self/default 完整语义、ICMP 同 id 边界、NAT 顺序、IOS 清会话命令均未在本包完成，也不得被读成已完成。阶段 6 在此收口；**不进入 C6b 或其他扩展**。
---

## 补修（2026-11-16）：讲义 08 IOS 接口命名闭环

> 依据：`netemu-stage6b-fix-implementation-handoff.md`。本节为**追加**，以上 6B 章节不回写。
> 阻塞项 B-1（课程原文陷阱）+ 观察项 O-1（show 组合形式）均已处置。

### 补修-1 B-1：讲义 IOS zone 块接口命名

**复核方证据（真实 WASM 浏览器，全新拓扑，按课程原文逐条）**：

```
zone security INSIDE          → Executed   R1(config-sec-zone)#
zone security OUTSIDE         → Executed   R1(config-sec-zone)#
interface g0/0/0              → InvalidCommand  "% Invalid interface type and number"
zone-member security INSIDE   → InvalidCommand（级联失败）
interface g0/0/1              → InvalidCommand（级联失败）
zone-member security OUTSIDE  → InvalidCommand（级联失败）
```

**根因**：`ifname.rs` 的 IOS 规范接口名为 `GigabitEthernet0/{idx}`，即 `g0/0`、`g0/1`；`g0/0/0` 是 VRP 命名。修复前 6B 的课程原文测试只以 `R1(config)# class-map type inspect` 为锚点解析第二个代码块，zone 块从未被执行。

**改动 diff 摘要**（仅 IOS zone 块；VRP 块 `add interface g0/0/0` 保持不动）：

| 文件 | 修改 |
|---|---|
| `web/src/content/lessons/08-firewall-zone.md` | IOS 块 `interface g0/0/0` → `interface g0/0`；`interface g0/0/1` → `interface g0/1` |
| `web/src/content/lessons-en/08-firewall-zone.md` | 同上（中英逐句对应） |
| 两课验证命令块 | 新增证据支持的组合形式 `show policy-map type inspect zone-pair IN-OUT session` |

通读确认：IOS 语境下已无 `g0/0/x` 三 slot 写法；VRP 块的两处 `add interface g0/0/0/1` 保留。

**测试扩展**（两处解析逻辑同源）：

- 从真实 md 解析**两个代码块**：zone 块锚点 `zone security INSIDE` 至代码块闭合；策略块沿用 `R1(config)# class-map type inspect`。zone 锚点位于 VRP 块之后，解析不会误入 VRP。
- 合并序列 = zone 块 + 一个**会话边界 `exit`**（zone 块最后停在 config-if；`exit` 是测试夹具边界，不是讲义命令）+ 策略块。
- 中英两个块分别断言完全一致；断言 zone 块包含 `interface g0/0` / `interface g0/1`、不含 `interface g0/0/0`。
- Rust `stage6b_lesson_ios_demo_sequence_runs_and_is_stateful`：测试夹具只预置接口地址；讲义序列逐条执行 0 失败；断言 `config-sec-zone` / `config-cmap` / `config-pmap` / `config-pmap-c` / `config-sec-zone-pair` / `config-if` 提示符与 `exit` 层级（`config-sec-zone→config`、`config-pmap-c→config-pmap→config`）；反向先探测 0，正向 5/5，会话 1 条 ICMP。
- WASM `stage6b_wasm_lesson_ios_demo_runs_on_real_sim`：同源解析 + 真实 `Sim` 逐条执行 + 相同提示符/数据面断言 + `show policy-map ... IN-OUT session` 含 `sessions: 1 icmp`。
- 既有策略块测试保留并改为覆盖完整序列；**未删测、未放宽**。

### 补修-2 O-1：`show policy-map type inspect zone-pair <name> session`

**核验结论：官方支持组合形式，已实现。**

- E1 正文：任务步骤给出 `show policy-map type inspect zone-pair session`（单数），示例给出 `show policy-map type inspect zone-pair ogacl_zp`（带区域对名），未出现互斥表述；
- Cisco 官方《Understand the Zone-Based Policy Firewall Design》明确语法为 `show policy-map type inspect zone-pair [zone-pair-name] [sessions]`，并说明 “When no zone-pair name is mentioned, policy-maps on all zone-pairs are displayed. The sessions option displays the inspection sessions created by the policy-map application on the specified zone-pair.” → **区域对名与 sessions 可组合**。
- 实现：`zone-pair [name] session` / `zone-pair [name] sessions`、裸 `session` / `sessions` 均可用；`session`（E1）与 `sessions`（设计指南）作为同义关键字接受。
- 规格：新增 `exec/show/policy-map/type/inspect/zone-pair/@0word/session`、`.../@0word/sessions`、`.../sessions`，全部 `Implemented + DocumentationOnly + strict`，引用 E1 与设计指南。
- 测试：Rust 与 WASM 均断言组合形式返回对应 zone-pair 且 `sessions: 1 icmp`；不再是 `InvalidCommand`。

### 补修-3 真实浏览器自验（补修后讲义原文）

方式：headless Chrome + `vite preview` 加载本次构建 `net_sim_bg-RET9hLbH.wasm`（1,740.79 kB / gzip 609.87 kB）；**PWA 清缓存**（注销 service worker + 删除 caches + `?v=6bfix` 导航）；拓扑由真实 WASM 生成并注入（PC1—R1—SRV，R1 仅预置接口地址，zone/ZBF 全部来自讲义原文）。

| 验收点 | 实测 |
|---|---|
| 讲义 zone 块 + 策略块 | 解析出的 20 条命令 + `enable`/`configure terminal`/边界 `exit`/`end` 逐条真实键盘执行，**0 失败**（无 `% Invalid` / `Error:` / `Unrecognized` / `Incomplete`） |
| 提示符 | **14 个关键断言全部一致**：`R1#`、`R1(config)#`、`R1(config-sec-zone)#`×2、`R1(config-if)#`×4、`R1(config-cmap)#`、`R1(config-pmap)#`、`R1(config-pmap-c)#`×2、`R1(config-sec-zone-pair)#`×2；`exit` 层级 `config-sec-zone→config`、`config-pmap-c→config-pmap→config` 实测一致 |
| 数据面 | SRV→PC1 `0 received`；PC1→SRV `5 received`；`show policy-map ... session` 与 `... IN-OUT session` 均 `sessions: 1 icmp` |
| show | `show zone security` 含 INSIDE/OUTSIDE；`show zone-pair security` 含 `Zone-pair: IN-OUT` + `service-policy inspect PMAP`；policy-map 视图含 `Class-map` / `inspect` |
| 双通道 | `parameter-map type inspect PMAP2` 仅出现 `[netemu]` 提示、无设备输出；随后 `show zone-pair security` 仅设备输出、无 `[netemu]` |
| 控制台 | `consoleErrors = []` |
| 截图 | `docs/cli-fidelity/stage6bfix-lesson-flow.png`（zone+策略全流程与 `config-sec-zone-pair` 提示符）、`stage6bfix-submode.png`（`config-pmap-c`）、`stage6bfix-reverse.png`（反向 0 received）、`stage6bfix-ping.png`（正向 5/5）、`stage6bfix-show.png`（`sessions: 1 icmp` 与组合查询） |

### 补修-4 最终门禁（真实结果）

| 门禁 | 结果 |
|---|---|
| `cargo clippy --workspace --all-targets -j 1 -- -D warnings` | **0** |
| `cargo test --workspace -j 1` | **0**：**332 passed / 0 failed / 0 ignored，50 个测试二进制**（补修未新增测试函数：扩展 2 个既有课程原文测试、新增 3 条 show 规格；总数与 6B 一致） |
| `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | **0**：release 17.10s；**wasm-opt 正常运行**，未关闭优化；产物 `net_sim_bg-RET9hLbH.wasm`（1,740.79 kB / gzip 609.87 kB） |
| `npx --yes pnpm@10 --dir web run check` | **0** |
| `npx --yes pnpm@10 --dir web run build` | **0**：`✓ built in 2.49s`，产物同上 |
| `cargo fmt` | **环境性挂起（已知）**：`cargo fmt --all -- --check` 与 `rustfmt --check ios.rs` 在本机不退出（HEAD 原文件同样复现）；本次改动的 `specs.rs`、`stage6b_firewall.rs`、`stage6b_tests.rs` 已逐文件 `rustfmt` 通过；`ios.rs` 无超过 100 列的行，新增代码按 rustfmt 风格手工核对 |

### 补修-5 工作区与临时产物

| 项 | 状态 |
|---|---|
| HEAD | `3d784e022c8e70ca3c4aafbce65edd1220064ed7`（未变） |
| 分支 | detached HEAD；未提交、未推送 |
| 工作区 | **34 个已跟踪 M + 25 组未跟踪**（补修修改已在既有 M 中；新增 5 张补修截图仍属 `docs/cli-fidelity/` 未跟踪条目） |
| 临时产物（真实文件系统核对） | `web/dist/netemu-6bfix-ws.json`：GONE；`C:/Users/22534/AppData/Local/Temp/pw6bfix`：GONE；`netemu-6bfix-workspace.json` / `netemu-6bfix-gen.mjs`：GONE；vite preview（端口 4176）已终止；仓库内无工作区 JSON 残留 |

**补修结论：B-1 与 O-1 均已闭环；本包在此停止，不进入 C6b，不做范围外扩展。**
