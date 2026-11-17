# 阶段 3B · 剩余课程错位 / IOS ZBF 真实对照 / 代表闭环 / 阶段 3 收口 · 实施记录

> 执行日期：2026-11-12
> 基线：`3d784e022c8e70ca3c4aafbce65edd1220064ed7`（detached HEAD，未提交、未推送）
> 上一记录：`docs/cli-fidelity/stage3a-implementation-record.md`（保留，**未回写**）
> 范围依据：`netemu-stage3b-implementation-handoff.md`；技术约束取主方案 §4.4 / §5.4 / §5.5 / §10 / §14 / §15
> 明确不做：C6b、Comware 拆树与原生课程整改、F5 TMOS、完整防火墙 / ASA-VRP 血统拆分、完整 IOS ZBF 数据面、`run()` 前端事件动画
> 停止点：完成课程修正、ZBF 对照边界与两个代表闭环后停止

---

## 1. 现场保护与基线复跑

| 项 | 结果 |
|---|---|
| `git rev-parse HEAD` | `3d784e022c8e70ca3c4aafbce65edd1220064ed7` |
| `git symbolic-ref -q HEAD` | 空 → **detached HEAD（no branch）** |
| 起点工作区 | **25 个已跟踪 M + 15 组未跟踪**（阶段 0/1、2A/2B/2C、3A 改动全部保留） |
| 现场保护 | 全程未 `reset` / `clean` / 覆盖 / 删测 / 注释测试 / 放宽断言；未提交、未推送 |
| 基线 `cargo test --workspace --quiet` | 全部测试二进制 `ok`，`EXIT=0`；与交接方 265 项一致（最终 276 − 本包新增 11 = 265） |
| 构建目录 | 共享 target 指纹权限冲突 → 全程使用独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-3b-target` |
| 阅读范围 | 交接文档、主方案 §4.4/§5.4/§5.5/§10/§14/§15、Stage 3A 记录、`labs.ts` / `labs-en.ts` / `cmdmap.ts` / 两份防火墙课程 md、`CourseView.svelte` / `LeftPanel.svelte`、`specs.rs` / `spec.rs`、`ios.rs` / `huawei.rs` / `show.rs`、`sim-wasm/src/lib.rs` / `stage3a_tests.rs` / `stage3a_cmdmap.rs`、`security_golden.rs` / `campus_golden.rs` |

**环境事项**：交接文档提到本机 `wasm-opt` 可能报 `拒绝访问 (os error 5)`；本包两次 `wasm-pack build` 均**未复现**，未关闭优化、未改配置换绿（见 §10）。

---

## 2. 实际代码、课程、证据与测试改动

### 2.1 Rust 生产代码（演进现有内核，无平行语法树）

| 文件 | 改动 |
|---|---|
| `crates/cli-dialects/src/ios.rs` | 在现有 `CNode` 树上增量登记 5 条真实 IOS XE 17.x ZBF 命令：`config/class-map/type/inspect/match-any/@0word`、`config/policy-map/type/inspect/@0word`、`config/zone-pair/security/@0word/source/@0word/destination/@0word`（在旧 `zone-pair security <name>` 参数节点下新增 kids，**旧 run 不变**）、`security_rule/service-policy/type/inspect/@0word`、`exec/show/policy-map/type/inspect/zone-pair`。 |
| `crates/cli-dialects/src/lib.rs` | `cmd::zbf_recognized_only` 防御性 handler：只写 `Ctx.notices`（`simulator_notices` 通道），不改 `sim-core` 状态、不返回厂商成功文本。规格门禁为 `NotSimulated` 时 `exec_line_result` 在门禁处即拦截，正常路径不会调用它。 |
| `crates/cli-dialects/src/specs.rs` | 新增 4 个官方证据常量 + 5 条 `SpecRow`：`Dialect::Ios`、`Effect::Unimplemented`、`SemanticSupport::NotSimulated`、`EvidenceLevel::DocumentationOnly`、`capability: None`、`strict: false`，`references` 指向 Cisco 官方 URL。沿用现有稳定 ID、旁表、四态与双通道，未建第二套语法树。 |

### 2.2 前端课程与 UI

| 文件 | 改动 |
|---|---|
| `web/src/lib/labs.ts` | `campus-capstone` 验证行按 IOS / VRP 设备归属分开；`enterprise` NAT 任务改为由 `show ip nat translations` 核验，去掉 Ping 单独证明 NAT 的强宣称。 |
| `web/src/content/labs-en.ts` | 同步上述两处英文修正。 |
| `web/src/content/cmdmap.ts` | `CmdRow` 新增 `teachingVendors?: CmdVendor[]`（**课程内容元数据**）与「空列 = 该平台无逐字对应命令」语义；security 段重构为真实 IOS ZBF 行 + VRP/Comware 真实策略行 + 一行明确隔离的教学抽象行。`rowMode()` / `profileForRow()` / Stage 3A Profile 选择契约不变。 |
| `web/src/components/CourseView.svelte` | 空列不调用 `inspect_command`，显示「— 本平台无逐字对应命令」；教学抽象单元格增加**独立徽标**，结构化 `status / support / evidence / specId / detail` 照常展示，不被徽标替代。 |
| `web/src/lib/i18n-dict.ts` | 中英文各新增 `course.teaching` / `course.notApplicable`。 |
| `web/src/content/lessons/08-firewall-zone.md`、`lessons-en/08-firewall-zone.md` | 把原「思科：zone-pair security ZP → source/destination/action」代码块改为：官方 IOS XE 17.x ZBF 语法骨架 + 独立 `netemu 教学抽象` 引用块 + 「仅识别 / 未模拟」边界说明 + 官方依据；区域命令标注官方语法来源。 |

### 2.3 测试

| 文件 | 数量 | 覆盖 |
|---|---|---|
| `crates/cli-dialects/tests/stage3a_cmdmap.rs` | 5（保留） | 新增 `table_blocks()`；`rows()` 跳过平台专属空列（不是过滤厂商）；结构断言改为 `rows >= blocks` + 三厂商覆盖；**`stage3a_cmdmap_probe_rejected_rows` 从只打印改为 `assert_eq!(rejected, 0)`**，仍打印 `REJECTED` 明细与 `TOTAL_ROWS`。 |
| `crates/cli-dialects/tests/stage3b_courses.rs` | **11（新增）** | `include_str!` 直接读真实 `labs.ts` / `labs-en.ts` / `cmdmap.ts` / 两份课程 md：campus 中英文 `show vrrp` / `display eth-trunk 1` 归属；enterprise 中英文不再由 Ping 单独证明 NAT；security 真实行与简化序列隔离、课程 md 独立教学抽象标识；5 条真实 ZBF 命令的 `status/support/evidence/specId/detail`、正确/错误模式、跨方言隔离；`NotSimulated` 的 `device_output` / `simulator_notices` 双通道与状态指纹不变；旧简化路径仍可解析且不重标厂商保真；Comware 阶段 4 门禁；IOS `lint_report().strict_ok()` 与 5 条新规格的证据/效果分类。 |

**未改动**：`sim-core` 唯一状态事实来源、四态枚举、`judge()` 映射矩阵、`exec_command` 旧文本 API、`verify_command` 兼容 API、同步 WASM 契约、`TerminalPanel.svelte` 双通道渲染、`lab-checks` 纯读/主动双入口、`run()` 前端接线、`ios_zone_pair_policy` 与全部 VRP 防火墙数据面测试。

### 2.4 新增文档产物

`docs/cli-fidelity/stage3b-implementation-record.md`（本文）、`stage3b-security-table.png`、`stage3b-security-teaching.png`、`stage3b-firewall-active.png`。

---

## 3. 中英文课程修正前后对照

| 位置 | 修正前 | 修正后 |
|---|---|---|
| `labs.ts` campus 验证 | `验证：display vrrp（CORE1 Master）、show ip ospf neighbor（FULL）、display eth-trunk 1（2 Up）…` | `验证（IOS CORE1/CORE2）：show vrrp（CORE1 Master）、show ip ospf neighbor（FULL）；验证（VRP SW1/SW2）：display eth-trunk 1（2 个 Selected 成员）…` |
| `labs-en.ts` campus 验证 | `Verify: display vrrp (CORE1 Master), show ip ospf neighbor (FULL), display eth-trunk 1 (2 Up)…` | `Verify (IOS CORE1/CORE2): show vrrp (CORE1 Master), show ip ospf neighbor (FULL); verify (VRP SW1/SW2): display eth-trunk 1 (2 Selected members)…` |
| `labs.ts` NAT 核验行 | `… show ip nat translations 看转换` | `… 用 show ip nat translations 核验转换表项` |
| `labs.ts` enterprise 末行 | `PC1 ping 202.100.1.100 —— 通了！你的流量已经 NAT 成公网地址` | `PC1 ping 202.100.1.100 —— 连通性检查；Ping 通过不能单独证明流量已 NAT 成公网地址，转换是否生效以 show ip nat translations 表项为准` |
| `labs-en.ts` NAT 核验行 | `… check translations with show ip nat translations` | `… verify the translation entries with show ip nat translations` |
| `labs-en.ts` enterprise 末行 | `PC1 ping 202.100.1.100 — it works! Your traffic has been NATed to a public address` | `PC1 ping 202.100.1.100 — a reachability check only; a successful ping alone does not prove the traffic was NATed, judge the translation from show ip nat translations entries` |
| 防火墙课程 md（中/英）「思科」代码块 | `zone-pair security ZP` + `source trust` + `destination untrust` + `action permit`（作为真实思科 CLI 展示） | 官方 `class-map / policy-map / zone-pair / service-policy` 语法骨架 + 独立 `netemu 教学抽象` 引用块（明确「不是真实 IOS ZBF CLI」「仅识别、未模拟」）；验证行改用 `show policy-map type inspect zone-pair` |

NAT 配置 `ip nat inside/outside`、真实 Ping、ACL 选做项、8 项检查定义与标签、Stage 3A 纯读/主动契约均未改动。

---

## 4. IOS ZBF 官方证据、真实语法与模拟边界

### 4.1 官方证据（访问日期均为 2026-11-12；**官方网页正文全文核验**，非搜索摘要）

| # | 文档标题 / 章节 | URL | 版本 / 更新 |
|---|---|---|---|
| E1 | *Security and VPN Configuration Guide, Cisco IOS XE 17.x* — **Zone-Based Policy Firewalls** | `https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/sec-vpn/b-security-vpn/m_sec-zone-pol-fw-xe.html` | IOS XE 17.x，Updated: January 11, 2021 |
| E2 | 同书 — **Enabling ALGs and AICs in Zone-Based Policy Firewalls** | `https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/sec-vpn/b-security-vpn/m_zbf_enable_alg_aic.html` | IOS XE 17.x，Updated: January 11, 2021 |
| E3 | 同书 — **Firewall Support of Skinny Client Control Protocol** | `https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/sec-vpn/b-security-vpn/m_sec-data-sccp.html` | IOS XE 17.x，Updated: January 11, 2021 |
| E4 | *IP Addressing Configuration Guide, Cisco IOS XE 17.x* — **vTCP for ALG Support**（`show` 命令清单） | `https://www.cisco.com/c/en/us/td/docs/routers/ios/config/17-x/ip-addressing/b-ip-addressing/m_sec-vtcp-alg.html` | IOS XE 17.x，Updated: November 15, 2022 |

**证据边界**：以上均为 `DocumentationOnly`。没有带版本的 IOS XE 设备、eNSP 或可采信设备样本，因此**不建立任何逐字厂商输出黄金、不升级证据等级**；本包只建立语法、CLI 模式、结构化状态与纯读性测试。项目既有的 `show zone security` / `show zone-pair security` 查询仍是项目简化 zone 状态视图，本次未在这些页面正文内取得其逐字输出证据，保持「输出待验证」。`show policy-map type inspect zone-pair` 有 E1/E4 正文依据；`show zone-pair security` 有 E4 正文依据。

### 4.2 官方真实语法与 CLI 模式（E1～E4 正文）

| 命令 | 官方模式 / 提示符 | 依据 |
|---|---|---|
| `zone security {zone-name | default}` | `Device(config)#` → `config-sec-zone` | E1/E2/E3 |
| `zone-member security zone-name` | `Device(config-if)#` | E1/E2/E3 |
| `class-map type inspect {match-all | match-any} class-map-name` | `Device(config)#` → `config-cmap` | E1/E2/E3 |
| `match protocol protocol-name` / `match access-group …` | `config-cmap` | E1/E3 |
| `policy-map type inspect policy-map-name` | `Device(config)#` → `config-pmap` | E1/E2/E3 |
| `class type inspect {class-map-name | class-default}` | `config-pmap` → `config-pmap-c` | E1/E2/E3 |
| `inspect [parameter-map-name]` / `drop [log]` / `pass` | `config-pmap-c` | E1/E2/E3 |
| `zone-pair security zone-pair-name source source-zone destination destination-zone` | `Device(config)#` → `config-sec-zone-pair` | E1/E2/E3 |
| `service-policy type inspect policy-map-name` | `config-sec-zone-pair` | E1/E2/E3 |
| `show policy-map type inspect zone-pair [session]`、`show zone-pair security` | privileged EXEC | E1/E4 |

官方正文明确：**没有 `source` / `destination` / `action permit` 这类 zone-pair 子命令**，策略动作来自 `class-map → policy-map → service-policy` 的 inspect/pass/drop；`If a policy is not configured between a pair of zones, traffic is dropped by default.` 因此原课程把 `zone-pair security ZP` + `source` + `destination` + `action permit` 当成真实 IOS ZBF 是**明确错误**。

### 4.3 本包登记的最小真实语法子集（只覆盖对照所需命令）

| 课程命令 | 模式 | 稳定 ID | 结构化结果 |
|---|---|---|---|
| `class-map type inspect match-any WEB` | Config | `config/class-map/type/inspect/match-any/@0word` | `NotSimulated` / `recognized-only` / `DocumentationOnly` |
| `policy-map type inspect PMAP` | Config | `config/policy-map/type/inspect/@0word` | 同上 |
| `zone-pair security IN-OUT source INSIDE destination OUTSIDE` | Config | `config/zone-pair/security/@0word/source/@0word/destination/@0word` | 同上 |
| `service-policy type inspect PMAP` | SecurityRule | `security_rule/service-policy/type/inspect/@0word` | 同上 |
| `show policy-map type inspect zone-pair` | Exec | `exec/show/policy-map/type/inspect/zone-pair` | 同上 |

- `service-policy` 由现有 `Mode::SecurityRule`（项目 `config-zone-pair`）承载官方 `config-sec-zone-pair` 模式语义；**提示符逐字差异属于阶段 6**，本包不宣称 L2 逐字保真。
- 未登记 `match protocol` / `class type inspect` / `inspect` 等子模式命令，避免为凑表绿而造第二套模式层；它们与完整 ZBF 数据面同属阶段 6 范围。
- 真实但未模拟的命令**不伪报 `InvalidCommand`**：正确模式 + 正确语法 → `NotSimulated`；错误模式 → `InvalidCommand`（真实例：`Mode::Exec` 下输入 `class-map …`）；`judge()` 能力检查仍先于语义检查，四态不变。

### 4.4 教学抽象与模拟边界

- 旧简化路径 `zone-pair security ZP` → `source trust` → `destination untrust` → `action permit` 是**独立稳定 ID**，代码与 `security_golden.rs::ios_zone_pair_policy` 测试全部保留，不作为厂商保真重新包装。
- 课程侧标识是 `CmdRow.teachingVendors: ['ios']` + `CourseView` 的独立徽标「netemu 教学抽象（非真实厂商 CLI）」/ 英文「netemu teaching abstraction (not real vendor CLI)」。它是**课程内容元数据，不是第五种执行状态**，也不替代 `InspectionResult.status/support/evidence`（实测该行仍显示 `Executed` / `pending-verification` / `Unverified`）。
- 未模拟限制只进入 `simulator_notices`：`exec_line_result` 对 `NotSimulated` 返回 `device_output = ""`、`state_changed = false`、一条 `[netemu]` 提示；`stage3b_courses.rs` 与浏览器双通道测试均断言 `device_output` 不含 `netemu` / `%`。
- Comware 列继续受阶段 4 门禁约束，UI 不显示「完整支持」；本包未新增任何空 handler 伪装「完整支持」或伪造状态变化。
- **未宣称**完整 IOS ZBF 数据面、ASA/VRP 血统拆分或防火墙平台基线完成。

---

## 5. security 对照表修改前后结构化状态

| 行 | 修改前（IOS 列） | 修改前结构化状态 | 修改后（IOS 列） | 修改后结构化状态 |
|---|---|---|---|---|
| 创建安全区域 | `zone security trust` | `Executed` / `pending-verification` / `Unverified`（迁移期） | 不变（官方语法，E1～E3） | 不变 |
| 接口划入区域 | `zone-member security trust` | 同上 | 不变（官方语法，E1～E3） | 不变 |
| 创建区域间策略 | `zone-pair security ZP` | 同上，**却冒充真实 IOS ZBF 对照** | VRP/Comware 真实行；IOS 列为空「无逐字对应」 | VRP/Comware 照常 inspection；IOS 不调用 |
| 策略源 / 目的 / 动作 | `source trust` / `destination untrust` / `action permit` | 同上（项目自有子命令） | 从真实对照移除；VRP/Comware 真实子命令保留 | VRP/Comware 照常 inspection；IOS 列为空 |
| 定义检查类映射 | — | — | `class-map type inspect match-any WEB` | `NotSimulated` / `recognized-only` / `DocumentationOnly` |
| 创建检查策略映射 | — | — | `policy-map type inspect PMAP` | 同上 |
| 创建区域对并指定方向 | — | — | `zone-pair security IN-OUT source INSIDE destination OUTSIDE` | 同上 |
| 区域对挂接检查策略 | — | — | `service-policy type inspect PMAP` | 同上 |
| 查看检查策略 / 安全策略 | `show zone-pair security` | 迁移期 + 简化输出 | `show policy-map type inspect zone-pair` | `NotSimulated` / `recognized-only` / `DocumentationOnly` |
| netemu 教学抽象（新增隔离行） | — | — | `zone-pair security ZP` + 独立徽标 | `Executed` / `pending-verification` / `Unverified`（实测），**不标完整支持** |

浏览器实测（最终重建 WASM，reload 后）：security 段 **13 行**、`recognized-only` **5** 个单元格、教学抽象徽标 **1** 个、平台专属空列 **14** 个、Comware `full-support` **0**；直接调用 `sim.inspect_command('ios-xe-17-router','Config','class-map type inspect match-any WEB')` 返回 `NotSimulated / recognized-only / DocumentationOnly`。

---

## 6. 课程表零拒绝变成真实门禁

- `stage3a_cmdmap_probe_rejected_rows` 现在断言 `rejected == 0`，并保留 `REJECTED` 明细打印与 `TOTAL_ROWS`。
- **未过滤任何 IOS security 行**：所有非空单元格（含 5 条真实 ZBF 行与教学抽象行）都在选定 Profile/模式下逐格检查；空列是「该平台无逐字对应命令」，不是「把错误行藏起来」。
- 结构断言从 `len % 3 == 0` 适配为 `rows >= table_blocks().len()` + 三厂商整体覆盖 + 总数 ≥ 300；这是课程表支持平台专属行所需的结构变化，**没有放宽状态判断**（反而新增了 `rejected == 0` 硬断言）。
- 新增 `stage3b_courses.rs` 直接 `include_str!` 真实课程文件，防止 `display vrrp` 错位与简化 IOS ZBF 序列回归。

---

## 7. `campus-capstone` 真实 WASM / CLI 验收映射

浏览器脚本经 `window.__netemu.sim` 驱动本次真实编译的 WASM，通过 `netemu:open-lab` 调用真实 `applyLab(lab9)` 构建拓扑，再执行 **129 条真实 CLI 配置命令，`commandFailures = 0`**（SW1 33 / SW2 28 / DSW 30 / CORE1 23 / CORE2 15，序列复用 `campus_golden.rs`，未维护第二份课程事实来源）。

| 验收点 | 真实结果 |
|---|---|
| IOS `show vrrp` Master | `GigabitEthernet0/0    1     200     Master      10.0.0.1`（CORE1） |
| VRP `display eth-trunk 1` Selected | `WorkingMode: LACP` / `Operate status: up` / `Number Of Up Port In Trunk: 2`；两个成员行 `GigabitEthernet0/0/3`、`GigabitEthernet0/0/4` 均为 `Up`（对应内部 `lag.selected` 集合） |
| 中英文 IOS 任务 | 已无 `display vrrp`（Rust `include_str!` 断言 + 课程文件实测） |
| 实验检查 | 课程实际定义 **8 项**（交接文档写「7 项」，按代码事实如实报告）：5 纯读 + 3 主动 Ping |
| 纯读入口 | 5 项通过，3 项 `skipped=true`，detail「主动实验项：纯读检查不执行，请运行实验判分」 |
| 主动入口 | **8/8 全部 `passed && !skipped`**（含 PC1→PC2、PC1→PC3、PC1→外网 3 个真实 Ping） |

未重写 LACP / OSPF / VRRP / NAT 数据面；`show vrrp` 与 `display eth-trunk 1` 的旧文本 API 输出尾部仍带 `[netemu]` 待验证提示（迁移期文本契约），结构化 `exec_command_result` 双通道另见 §9。

---

## 8. `firewall-zone` 真实 WASM / CLI 验收映射

真实 VRP 防火墙拓扑 + **22 条真实 VRP CLI 配置命令，`commandFailures = 0`**（序列复用 `security_golden.rs::vrp_firewall_zone_policy_stateful` 的配置段）：

```
system-view → interface g0/0/0 / g0/0/1（IP + undo shutdown）
→ firewall zone trust / untrust（add interface）
→ security-policy → rule name allow-out → source-zone trust
→ destination-zone untrust → action permit
```

| 验收点 | 真实结果 |
|---|---|
| `display firewall zone` | `Zone: trust / GigabitEthernet0/0/0`、`Zone: untrust / GigabitEthernet0/0/1` |
| `display security-policy` | `rule name allow-out` / `source-zone trust` / `destination-zone untrust` / `action permit` |
| 3 个纯读项 | 全部 `passed=true`、`behavior=readOnly`、`skipped=false` |
| 2 个主动项 | 纯读路径 `skipped=true`；主动路径 `SRV → PC1 被默认拒绝 = ping 不通`、`PC1 → SRV 放行 = ping 连通` |
| 纯读前后不变 | `dump()` 完全一致；`stats()` 前后均 `{virtualNs:0, events:0, inFlight:0, dropped:0}`，`statsUnchanged=true` |
| 主动判分 | **5/5**（`passed && !skipped`），`app.labPassed['firewall-zone'] === true` |
| 完成门禁 | 纯读后 `labPassed` 仍非 true（`readOnlyDidNotMarkPassed=true`），只有完整主动判分才标记完成 |

本包**未**宣称完整 IOS ZBF 数据面或 ASA/VRP 血统拆分完成。

---

## 9. 浏览器流程、截图边界与控制台

- 方式：`playwright-cli`（`@playwright/cli`）+ `vite preview @ 127.0.0.1:4173`，消费 `vite build` 产物中的真实 `net_sim_bg-CmCCz-jw.wasm`；**不使用 TypeScript 假结果或声明桩**。临时验收脚本 `web/dist/stage3b-verify.js` 用后已由最终 `vite build` 清空 `dist` 删除。
- 课程表逐单元格：首屏 vlan 7 行 → 安全课程 13 行；`cmdmapInspections()` 暴露真实缓存 key 与 `status/support/evidence/specId`；6 个不同 Profile 互不串用。
- 中英文切换：切换前后 key 集合 **138 → 138 完全一致**；英文徽标文案 `netemu teaching abstraction (not real vendor CLI)` 正常显示；中文徽标 `netemu 教学抽象（非真实厂商 CLI）` 正常显示。
- 教学抽象行实测：`badgeText` 独立显示，同一单元格仍显示 `支持: 待验证` / `状态: Executed`（结构化结果未被徽标替代）。
- Stage 3A 契约回归：campus 纯读跳过 3 个主动项、主动入口 8 项；firewall 纯读跳过 2 个主动项、主动 5/5；纯读不标记实验完成。
- 终端双通道：`sim.exec_command_result` 对 `class-map type inspect match-any WEB` → `status=NotSimulated`、`deviceOutput=""`、`simulatorNotices=["[netemu] 该命令的承诺语义尚未模拟；命令未执行，设备状态未改变"]`、`stateChanged=false`；旧 `exec_command` 文本仍含 `[netemu]` 提示（迁移期契约），`TerminalPanel` 未改动。
- 最终门禁重新 `wasm-pack build` + `vite build` 后 `reload` 复核：`inspect_command` 直接返回 `NotSimulated / recognized-only / DocumentationOnly`，13 行 / 1 徽标 / 5 recognized / Comware full-support 0；控制台 **Total messages: 0 (Errors: 0, Warnings: 0)**。

**截图**（`docs/cli-fidelity/`）：

| 文件 | 内容 |
|---|---|
| `stage3b-security-table.png` | 安全课程表：真实 IOS ZBF 5 行中的前 4 行「仅识别 / 未模拟」、VRP/Comware「待验证」、平台专属空列「— 本平台无逐字对应命令」 |
| `stage3b-security-teaching.png` | 滚动到表格底部：教学抽象独立徽标 + `show policy-map type inspect zone-pair` + Comware 阶段 4 说明 |
| `stage3b-firewall-active.png` | 防火墙实验：行为清单、纯读/主动双按钮、5/5 逐项结果与「纯读 / 主动实验」来源标签 |

**截图边界（如实说明）**：课程表需滚动 `.reader`、判分结果需滚动 `.left-panel` 才完整可见，故分两张课程截图；这是**单页流程证据，不是全量视觉回归**，未逐课程、逐终端窗口或逐语言抓屏（语言切换不污染由页内真实 WASM API 的结构化结果证明）。实质验证由页内真实 WASM API 结果完成。

---

## 10. 最终六项门禁与测试数量（真实结果）

| 门禁 | 退出码 | 结果 |
|---|---|---|
| `cargo fmt --all -- --check` | **0** | 先 `cargo fmt --all` 统一新增文件，复跑 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | **0** | 修掉一处 `field_reassign_with_default`（测试文件 Session 构造），无 `#[allow]` 规避 |
| `cargo test --workspace` | **0** | **276 passed / 0 failed / 0 ignored，47 个测试二进制**（基线 265 + 本包新增 11；`sim-wasm --lib` 16 项保持） |
| `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | **0** | 见分段 |
| `npx --yes pnpm@10 --dir web run check` | **0** | `tsc --noEmit` 0 错误 |
| `npx --yes pnpm@10 --dir web run build` | **0** | Vite 消费重新生成的 `net_sim_bg-CmCCz-jw.wasm`（1,549.15 kB，gzip 542.61 kB） |

### 10.1 WASM 总门禁逐段（分别报告）

| 段 | 结果 |
|---|---|
| Rust → WASM 编译 | ✅ `release` profile，基于当前源码（首次 43.46s；最终增量 0.19s） |
| wasm-bindgen 声明 | ✅ `net_sim.js` / `net_sim.d.ts` / `net_sim_bg.wasm.d.ts` / `package.json` 重新生成，含 `inspect_command` / `read_only_lab_checks` / `run_lab_checks` |
| wasm-opt | ✅ `[INFO]: Optimizing wasm binaries with wasm-opt`，本机**未复现** `拒绝访问 (os error 5)`；未关闭优化、未改配置换绿 |
| WASM 总门禁 | ✅ `:-) Done`，退出 0 |
| Web 结果 | ✅ `check` / `build` 均 0；浏览器 reload 后消费该 WASM 得到上述结构化结果，0 控制台消息 |

| 层 | 数量 / 结果 |
|---|---|
| Rust 单元 + 集成 + doctest | 276 passed / 0 failed（新增 11：全部在 `stage3b_courses.rs`） |
| 课程表零拒绝 | `TOTAL_ROWS` 全部非空单元格逐格检查，`REJECTED=0` 且为断言 |
| 浏览器真实 CLI | campus 129 条 + firewall 22 条 + 双通道/复核命令，`commandFailures = 0` |
| 浏览器真实检查 | campus 8/8 主动、firewall 5/5 主动、firewall 3/3 纯读且状态/时间/统计不变 |
| Web | `tsc --noEmit` 0 错误；`vite build` 0 错误；控制台 0 消息 |

---

## 11. HEAD、分支、真实工作区计数、未完成项与阻塞

| 项 | 状态 |
|---|---|
| HEAD | `3d784e022c8e70ca3c4aafbce65edd1220064ed7` |
| 分支 | **detached HEAD（no branch）**，`git symbolic-ref -q HEAD` 为空 |
| 提交 / 推送 | **未提交、未推送** |
| 工作区 | **27 个已跟踪 M + 16 组未跟踪**（起点 25 M + 15 组；本包新增 2 个已跟踪 M 为两份防火墙课程 md，1 组未跟踪为 `stage3b_courses.rs`；`docs/cli-fidelity/` 内新增 3 张截图与本记录）。起点改动全部保留。 |
| 构建目录 | 独立 `CARGO_TARGET_DIR` |
| 临时文件 | `web/dist/stage3b-verify.js` 已由最终 `vite build` 清空 `dist` 删除；`web/.playwright-cli/page-*.yml` 两个快照已删除；playwright-cli 创建的空目录不影响仓库（Git 不跟踪空目录）。无无入口诊断残留。 |

**未完成项（明确后置，未开始）**

| 项 | 所属阶段 |
|---|---|
| C6b 跨设备 SSH / 远程 VTY 会话仿真 | 阶段 2 后置的独立里程碑 |
| Comware 独立命令树、原生 VLAN/SVI 闭环与实验二课程整改 | 阶段 4 |
| F5 TMOS 独立方言、17.x 证据、Pool/Member 分配验收 | 阶段 5 |
| 完整防火墙平台基线、ASA/VRP 血统拆分、既有流/同区域/安全级别校准、完整 IOS ZBF 数据面与子模式 | 阶段 6 |
| `run()` 前端事件驱动动画接线 | 单独评审，无 Web 调用方 |
| 厂商逐字输出黄金、eNSP / 带版本设备输出校准 | 证据工作，无样本前保持「输出待验证」 |
| class-map / policy-map / zone-pair 子模式提示符逐字保真（如 `config-sec-zone-pair`） | 阶段 6 的 L2 交互范围 |

**阻塞**：无功能阻塞。环境事项：共享 target 指纹权限冲突（独立目录规避）；交接文档的 `wasm-opt os error 5` 本机未复现（如实记录，不以 Web 成功掩盖该差异）；preview 服务需后台常驻，验收后已停止。

---

## 12. 阶段 3 是否收口：明确结论

**结论：阶段 3「课程与 UI 对齐」在本包范围内收口。**

收口依据：

1. Stage 3A 已交付 Profile 级结构化 `inspect_command`、纯读/主动判分分离与 5 个代表实验闭环；本包完整保留并未回写其记录。
2. 本包关闭了主方案 §4.4 / §10.1 登记的剩余跨厂商课程错位（IOS `show vrrp`、VRP `display eth-trunk 1` 归属、enterprise NAT 证据表述），中英文与课程正文同步。
3. IOS ZBF 对照经 Cisco IOS XE 17.x 官方全文核验：简化序列从真实对照中隔离并加独立教学抽象标识；真实语法按官方模式登记为「仅识别 / 未模拟」，既不伪报命令错误也不显示完整支持；双通道、四态、Comware 阶段 4 门禁不回归。
4. 课程表 `REJECTED=0` 成为带断言的真实门禁，并新增直接读取真实课程文件的回归测试。
5. `campus-capstone` / `firewall-zone` / security 课程表均在**本次真实生成的 WASM** 与浏览器中完成闭环，纯读不变与主动副作用证据齐全，控制台无新增错误。
6. 六项最终门禁全绿（276 Rust 测试、WASM 逐段全绿、Web check/build 全绿）。

收口边界：阶段 3 完成**不等于**厂商逐字输出全部验证；没有带版本设备/eNSP 样本的输出继续保持待验证。上表「未完成项」全部属于阶段 4 / 5 / 6、C6b、`run()` 动画或证据采集工作，**本包未进入、未顺带重建**。

---

## 13. 停止点

完成课程修正、ZBF 对照边界、零拒绝门禁和两个代表闭环后停止；未进入 C6b、阶段 4、F5、阶段 6 防火墙扩展或 `run()` 前端动画。

---

## 14. 2026-11-13 隔夜复核（无代码改动）

> 触发：用户询问「还有没做完的吗？继续吧」。本节只复核现场与门禁，未新增实现、未修改课程/测试、未提交、未进入阶段 4 / 5 / 6。

### 14.1 现场

| 项 | 复核结果 |
|---|---|
| HEAD | `3d784e022c8e70ca3c4aafbce65edd1220064ed7`（与 §11 一致） |
| `git symbolic-ref -q HEAD` | 空 → 仍为 detached HEAD |
| 工作区 | 27 个已跟踪 M + 16 组未跟踪；起点 25 M + 15 组改动全部保留 |
| 提交 / 推送 | 未提交、未推送 |
| 交付物 | 本文与 3 张 `stage3b-*.png` 均在 |
| 临时产物 | `web/dist/stage3b-verify.js` 不存在（已由 `vite build` 清空 `dist` 删除）；`web/.playwright-cli/` 复核为空目录，本次已清除（Git 不跟踪空目录） |

### 14.2 六项门禁复跑（独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-3b-target`）

| 门禁 | 退出码 | 结果 |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | 无格式差异 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 无 warning、无 `#[allow]` 规避 |
| `cargo test --workspace --quiet` | 0 | **276 passed / 0 failed / 47 个测试二进制** |
| `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | 0 | `wasm-opt` 正常，未复现 `os error 5` |
| `npx --yes pnpm@10 --dir web run check` | 0 | `tsc --noEmit` 0 错误 |
| `npx --yes pnpm@10 --dir web run build` | 0 | 消费 `net_sim_bg-CmCCz-jw.wasm`（1,549.15 kB）构建成功 |

`vite build` 期间 `vite-plugin-svelte` 输出既有的 a11y / chunk-size 编译提示，均为**非错误**、退出码 0；与 §9 的浏览器控制台 0 消息不是同一层，不影响门禁结论。复跑后 `git status --short` 仍为 27 个已跟踪 M + 16 组未跟踪，构建未污染工作区。

### 14.3 结论

- **Stage 3B / 阶段 3 范围内无未完成项**：无失败门禁、无未修复测试、无缺失文档或截图、无临时脚本残留。
- §12 的收口结论与 §13 的停止点不变。
- §11「未完成项」清单全部属于 C6b、阶段 4 / 5 / 6、`run()` 动画与逐字输出证据采集，**不在本包授权范围内**；如需继续，需明确指定新阶段或任务。
