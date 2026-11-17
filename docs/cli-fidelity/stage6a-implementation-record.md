# 阶段 6A · 防火墙基线选择 / ASA·VRP 血统拆分 / 状态机与策略复查校准 · 实施记录

> 执行日期：2026-11-16
> 基线：`3d784e022c8e70ca3c4aafbce65edd1220064ed7`（detached HEAD，未提交、未推送）
> 上一记录：`docs/cli-fidelity/stage5a-implementation-record.md`（保留，**未回写**）
> 范围依据：`netemu-stage6a-implementation-handoff.md`；技术约束取主方案 §4.4 / §10.1 / §14 / §15
> 明确不做：C6b、完整 IOS ZBF 数据面与子模式提示符保真（保留 Stage 3B 的 recognized-only + 教学抽象，属 6B）、`run()` 前端事件动画、Comware 安全数据面
> 停止点：完成基线决策、血统拆分、校准与 §14.3 最低验收场景、lab10 课程闭环后停止

---

## 1. 现场保护与基线复跑

| 项 | 结果 |
|---|---|
| `git rev-parse HEAD` | `3d784e022c8e70ca3c4aafbce65edd1220064ed7`（未变） |
| 分支 | **detached HEAD**（`git branch --show-current` 为空） |
| 起点工作区 | **32 个已跟踪 M + 21 组未跟踪**（阶段 0/1、2A/2B/2C、3A/3B、4A 含补修、5A 全部保留） |
| 现场保护 | 全程未 `reset` / `clean` / 覆盖 / 删测 / 注释测试 / 放宽断言；未提交、未推送 |
| 基线 `cargo test --workspace` | **300 passed / 0 failed，48 个测试二进制**（与交接一致） |
| 基线课程表 | `TOTAL_ROWS=433 REJECTED=0` |
| 构建目录 | 独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-6a-target`；本机未出现并行编译挂起，**未使用 `-j 1`** |
| 阅读范围 | 交接文档、主方案 §4.4/§10.1/§14/§15、Stage 5A 记录全文、`security_golden.rs`、`show.rs`/`config_render.rs`/`specs.rs`/`profile.rs`/`device.rs`/`engine.rs` 防火墙相关段、`ios.rs`/`huawei.rs`/`comware.rs` 安全段、`lab-checks` 安全判分、`lab10`/`lab12`、`cmdmap.ts` security 段、Stage 3B ZBF 规格行 |

---

## 2. 工作项 A：基线证据评估与选择（先决策后实施）

### 2.1 官方证据表（全文核验，访问日期 2026-11-16）

全部为厂商官方站点正文，**无带版本设备 / eNSP 样本**，故统一 `DocumentationOnly`：只建立语法、模式、结构化状态与数据面断言，**不建立逐字输出黄金**。搜索摘要一律不作为裁定依据（正文缺失的条目已在下表明示）。

| # | 文档 | URL | 全文边界 |
|---|---|---|---|
| **V1** | 华为《安全策略的默认设置》(Default Settings for Security Policies) | `https://support.huawei.cn/hedex/api/pages/EDOC1100435583/AEP0109R/06/resources/security_secpolicy_cfg_0009.html` | 全文表 1：默认安全策略「源区域 local、目的区域 any → permit；其他策略匹配条件 any、动作 deny」；「控制同一安全区域内流量的默认安全策略 **禁用**」；「使用安全策略控制 LDP/BGP/DHCP 单播/OSPF 单播报文 启用」 |
| **V2** | 华为《什么是安全策略？》(IP 知识百科，更新 2026-07-13) | `https://info.support.huawei.com/info-finder/encyclopedia/zh/安全策略.html` | 全文「过滤机制」段：首包匹配安全策略 → 放行并**建立会话表**；后续包直接查会话表、作为该会话的回应报文直接放行，**不再匹配安全策略**；「只需在访问发起方向配置策略，反向流量无需配置」；匹配条件「与/或」关系、顺序即优先级、命中即停止 |
| **V3** | 华为《清除会话表信息》(Clearing a Session Table) | `https://support.huawei.cn/hedex/api/pages/EDOC1100386427/AZN0712J/09/resources/security_session_management_cfg_0009.html` | 全文：会话表项在一直有流量匹配时**不会老化**；典型清表场景之一即「更改了安全策略的配置，将某些用户的动作从允许改为禁止，并且希望该配置立即生效」；清表后依赖会话表转发的连接**被强行中断**；建议按条件小范围清除 |
| **V4** | 华为《检查安全策略》(Troubleshooting, V5R1) | `https://info.support.huawei.com/network/ptmngsys/Web/tsrev_security/cn/content/security/28_edesk_nat_failed_V5R1/edesk_nat_failed_edesk003.html` | 全文：「缺省情况下，**FW 系列防火墙域内的安全策略（如 trust 到 trust）都是禁止的**，需要手动打开域内访问策略」；`display security-policy` 输出列 `RULE ID / RULE NAME / STATE / ACTION / HITTED`，缺省策略行 `default enable deny` |
| **V5** | 华为《安全区域配置命令》(CLI Reference) | `https://support.huawei.cn/enterprise/zh/doc/EDOC1100460859/58b84bde`（同族 `EDOC1100516037/58b84bde`） | `firewall zone <name>` 进入安全区域视图（`[HUAWEI-zone-trust]`）；`add interface` / `undo add interface`；`description`；一个接口只属于一个区域；`display zone [ zone-name ] [ interface | priority ]` 视图=所有视图、缺省级别 1 监控级；输出 `trust priority is 85` / `interface of the zone is (1):` |
| **V6** | 华为《检查防火墙基本功能的配置结果》 | `https://support.huawei.com/enterprise/zh/doc/EDOC1100317283/cd376375` | 全文操作步骤：`display firewall zone [ zone-name ] [ interface | priority ]`、`display firewall interzone [ zone-name1 zone-name2 ]`、`display firewall-nat session aging-time` |
| **V7** | 华为《调整安全策略规则》+ `enable (安全策略规则视图)` | `https://support.huawei.com/enterprise/zh/doc/EDOC1100412543/284718e7`；`https://support.huawei.cn/enterprise/zh/doc/EDOC1100412535/2480e39b` | `rule move rule-name1 { after | before } rule-name2`；规则视图 `enable` / `disable`，「缺省情况下，安全策略规则的状态为启用」「只有当安全策略规则处于启用状态时才能发挥实际作用」 |
| **V8** | 华为《配置安全策略的会话老化时间》 | `https://support.huawei.cn/hedex/api/pages/EDOC1100435583/AEP0109R/06/resources/security_secpolicy_cfg_0018.html` | 全文：会话表项需持续被报文匹配，长时间无报文匹配即老化；`security-policy` → `rule name` → `session aging-time`、`long-link enable/aging-time`；老化时间优先级三级；被策略阻断会话缺省保留 120s |
| **V9** | 华为《Security Policy Configuration Commands》 | `https://support.huawei.com/enterprise/en/doc/EDOC1100262560/2480e39b/EN-US_CLIREF_0000001536916906` | `display security-policy rule name test` 输出 `(0 times matched) rule name test` + `source-zone` / `destination-zone` / `source-address range …` / `action permit`；表格视图列 `RULE ID / RULE NAME / STATE / ACTION / HITS`；`default action permit|deny` 带安全风险告警 |
| **C1** | Cisco《CLI Book 1: ASA Series General Operations CLI Configuration Guide, 9.18 — Transparent or Routed Firewall Mode》(Updated 2020-05-28) | `https://www.cisco.com/c/en/us/td/docs/security/asa/asa918/configuration/general/asa-918-general-config/intro-fw.html` | 全文相关句：「Unicast IPv4 and IPv6 traffic is allowed through the bridge group automatically **from a higher security interface to a lower security interface, without an access rule**」；「For Layer 3 traffic traveling **from a low to a high security interface, an access rule is required**」；`nameif` 用于给接口命名（只有被命名的接口才能用于基于接口的特性）；同安全级别需 `same-security` 才互通；已建立会话走快路径、新连接才做策略检查 |

**证据缺口（如实登记）**

1. 未取得 ASA `show conn` 输出、`show running-config` 逐字文本、ASA ACL 章节全文（本包只用到 C1 的安全级别方向性结论，不据此建任何黄金）。
2. 华为会话缺省老化值（TCP/UDP/ICMP）**未取得官方正文表格**；仅见社区样本 `tcp 1200s / icmp 20s`，因此只作 `CommunitySample` 参考，模拟器把老化时间标注为**模拟器参数**，不作为厂商承诺（见 §4 第 7 项）。
3「同域流量」两处官方表述存在版本差异，见 §2.4。

### 2.2 评估矩阵

评分 1（差）～5（好）。权重按交接文档四项要求。

| 维度 | 华为 VRP 安全策略（USG/FW） | Cisco ASA 9.x | 说明 |
|---|---|---|---|
| 官方证据可得性 | **4** | 4 | 两者都能取到正文；VRP 侧 `firewall zone` / `add interface` / `security-policy` / `rule move` / `enable·disable` / 会话老化 / 缺省策略均有官方正文（V1–V9）。ASA 侧本包只核验到 C1，`show conn` 与会话表章节未取得全文。VRP 侧证据**覆盖面更完整**（策略维护、会话、区域、查询四条线齐全） |
| 与既有模型/课程迁移成本 | **5** | 1 | 现有 `security_policies`（src_zone/dst_zone/permit，顺序首条）、`security_sessions`、`tcp_flows`、`Port::zone`、`show::security_zones/policies`、`cmdmap` security 段、`lab10` / `lab12` / 讲义 08 全部是 VRP 形状；选 VRP 只需补齐否定形式/会话表/规则维护。选 ASA 需重建接口（nameif/security-level）、策略（access-list/access-group/object/nat）、区域概念、全部课程与讲义，并重写 `lab12` 依赖 |
| 与既有数据面匹配度 | **5** | 2 | 既有数据面是「区域对 + 顺序策略 + 会话表」，正是 VRP 模型；ASA 的 `access-group` + 安全级别回退是另一套（接口级 ACL + 级别方向性），需要重建匹配流程，且既有 `tcp_flows` 与 ASA `conn` 表语义不同 |
| 教学价值 | **5** | 3 | 中文课程体系、`lab10`/`lab12`、讲义 08 均按华为体系讲授；VRP 会话表 + 策略禁用/清表的教学链条完整可演示。ASA 更适合作为对照概念（本包已在讲义 08 保留对照说明） |
| **合计** | **19** | **10** | — |

### 2.3 决策（先决策后实施）

> **基线 = 华为 VRP 防火墙安全策略（`Huawei VRP Firewall Security Policy Documentation Baseline (V1-V9)`）。**
> `DeviceKind::Firewall` 的 VRP Profile 由路由器 Profile 独立拆出为 `vrp-generic-firewall`。
> **落选血统 Cisco ASA（`nameif` / `security-level` / 安全级别回退）按「非基线命令移除」处置**：命令从 IOS/VRP/Comware 三棵树移除、数据面回退路径删除、`config_render` 停止渲染；`Port::nameif` / `Port::security_level` 字段**仅作旧存档兼容保留**（`#[serde(default)]`），不参与任何判定，也不再有 CLI 入口。ASA 语义只以「对照概念」形式留在讲义 08 正文中，并在 `cmdmap` 注释里说明其已从对照表移除。

**为什么不是「保留为独立 ASA Profile」**：ASA 需要独立的接口命名/安全级别模型、`access-group` 策略流程与 `conn` 表，与既有 `security_policies`（区域对）不是同一套数据面；本包范围内若强行保留，会形成两套并行语义而无课程与验收支撑，正是交接文档禁止的「无名共存」。因此选择显式移除 + 讲义对照，并在本记录留痕。

### 2.4 两处必须写明的语义分歧

1. **同区域转发默认动作**：V1（默认设置表）记录「控制同一安全区域内流量的默认安全策略 = 禁用」，即同域流量默认不由缺省策略控制（等价放行）；V4（FW 系列 V5R1 排障文档）明确「缺省情况下域内安全策略（如 trust→trust）都是禁止的，需要手动打开」。
   **决策**：取**更严格的一端**（同区域未命中策略 → 拒绝），理由是：(a) 交接文档要求「不得保留无证据的同区域无条件放行」，而「默认放行」只在某一版本族的默认表里成立；(b) 取拒绝时规则集统一为「首条启用命中，未命中缺省拒绝」，教学与测试无版本分支；(c) 分歧已登记为**版本差异待 eNSP 校准项**，实现上只由 `FirewallPolicy::intra_zone_default_permit` 一个布尔控制，校准后改一个值即可，不动共享逻辑与测试结构。
2. **未划分安全区域的接口**：本包取得的官方正文未给出结论（V5 只说「只支持将三层接口加入安全区域」，未说未加入时如何转发）。**决策**：显式建模为 `UnzonedTraffic::PassThrough`（沿用既有教学行为），在 `profile.rs` 注释与本记录标注**证据不足待验证**，不宣称保真；同时不因为「策略不生效」而放松任何已划入区域的正负用例。

---

## 3. 工作项 B：血统拆分清单

### 3.1 命令树（三树 + Tmos 不变）

| 位置 | 变更 | 依据 |
|---|---|---|
| `ios.rs` 接口视图 | **删除** `nameif <name>`、`security-level <0-100>` 两个节点（原 ASA 血统挂在 IOS 树上） | 落选血统移除；IOS XE 无这两个命令 |
| `ios.rs` | **保留** `zone security` / `zone-member security` / 简化 `zone-pair security` + `source/destination/action`（Stage 3B 教学抽象，`stage3b_courses` 门禁要求保留）；保留 5 条真实 ZBF recognized-only 节点 | Stage 3B 边界，属 6B |
| `huawei.rs` 安全区域视图 | **新增** `undo add interface <if>`（V5） | V5 |
| `huawei.rs` 安全策略视图 | **新增** `undo rule name <name>`、`rule move <n1> {before|after} <n2>`（V7/V9） | V7、V9 |
| `huawei.rs` 安全策略规则视图 | **新增** `enable` / `disable`（V7）、`undo source-zone` / `undo destination-zone` / `undo action`（否定形式） | V7、V9 |
| `huawei.rs` 用户视图 | **新增** `display firewall session table`（V3/V6）、`reset firewall session table`（V3） | V3、V6 |
| `huawei.rs` 用户视图 | **新增** `display security-policy rule name <n>` 明细（V9）；**移除** `display security-zone`（Comware 专属写法，改由 `display firewall zone` 承担） | V9、V5/V6 |
| `huawei.rs` `h_quit` | **修正**：安全策略规则视图 `quit` 只退一级到安全策略视图（原实现直接跳回系统视图） | V9/V7 视图层级 |
| `comware.rs` | **未改动**（`security-zone name` / `import interface` / `security-policy ip` 保持 Stage 4A 边界） | 交接 §5.3 |
| `tmos.rs` | **未改动** | 阶段 5A |

**双向矩阵断言**（`tests/stage6a_firewall.rs::stage6a_firewall_dialect_matrix_is_bidirectional`）：VRP 独占命令在 Comware / Tmos 上是 `invalid-command`（不是「仅识别」）；Comware 原生写法在 VRP 上被拒；ASA 的 `nameif` / `security-level` 在 IOS / VRP / Comware 三棵树全部不可解析；VRP 与 Comware 共有的 `security-policy` / `rule name` / `source-zone` / `action` 只主张 VRP 接受性（Comware 安全数据面不在本包，不主张其拒绝）。

### 3.2 设备模型（`sim-core`，唯一状态源不变）

| 项 | 变更 |
|---|---|
| `RouterState::security_zones: Vec<SecurityZone>` | **新增**（`name` + `priority: Option<u8>`）。区域集合与成员关系分离：集合在此，成员关系仍在 `Port::zone`（唯一状态，没有第二份成员表）。预定义区域缺省优先级 local 100 / trust 85 / dmz 50 / untrust 5 |
| `SecurityPolicy` | **新增** `enabled: bool`（`serde(default = true)`，V7 缺省启用）与 `hits: u64`（V9 HITTED 列）；`name/src_zone/dst_zone/permit` 不变 |
| `Port::zone` | 语义澄清为「安全区域成员」（VRP `firewall zone` + `add interface`；IOS 教学抽象 `zone-member security`），一个接口只能属于一个区域 |
| `Port::nameif` / `Port::security_level` | **降级为存档兼容字段**：字段保留 + 文档注释说明「ASA 落选血统，仅旧档往返，不参与任何判定/渲染」，无写入命令 |
| `engine::firewall_allow` / `firewall_tcp_allow` | 重写为「会话表命中 → 不复查策略（V2）→ 未命中 → 空策略判定 → 建表」；**删除 ASA 安全级别回退分支** |
| `engine::firewall_clear_sessions` | **新增** public API（`reset firewall session table` 的实现，返回清除条数） |
| `profile::FirewallPolicy` + `PlatformPolicy::firewall` | **新增**平台策略对象（域间/域内缺省动作、未划区域处置、TCP/ICMP 老化时间），参照 Trunk 平台策略先例**显式注入**；共享数据面不写 `if dialect` |
| `profile` 防火墙 Profile | **新增** `vrp-generic-firewall` / `ios-xe-17-zbf-teaching-firewall` / `comware-generic-firewall`，`profile_id_for` / `profile_by_id` / `os_baseline` 显式分支（F5/Comware 先例） |
| `summary` | `DevSummary::security_zones` 新增；`SecurityPolicySummary` 增加 `enabled` / `hits`，往返保留 |

### 3.3 `config_render` / `show` 按基线分叉

| 项 | 变更 |
|---|---|
| VRP 配置视图 | 新增 `security_lines()`：`firewall zone <n>` → `add interface <if>` → `quit`；`security-policy` → `rule name <n>` → `source-zone`/`destination-zone`/`action`/（禁用时）`disable` → `quit` → `quit`。**逐行可重执行**（`stage6a_display_reflects_configuration_and_config_view_is_replayable` 在干净设备上重放并比对 `display` 输出） |
| IOS 配置视图 | `zone security <n>` → `exit`；`zone-pair security <n>` → `source`/`destination`/`action` → `exit`（教学抽象） |
| Comware | 不渲染安全区域/策略（Stage 4A 边界，安全数据面不在本包），避免冒充未实现能力 |
| 删除的不可重执行行 | ① 接口视图内的 ` firewall zone <n>`（系统视图命令，放接口视图不可重放）；② 伪命令 `security-policy rule <n> <src> -> <dst> permit=<b>`（三方言共用、无任何方言语法）；③ ` nameif` / ` security-level`（命令已移除） |
| 顺带修正 | VLAN 段只在 `DeviceKind::Switch | AccessPoint` 上渲染（路由器/防火墙物理口 pvid 恒为 1，原实现会输出 `vlan batch 1`——在防火墙上不可重执行） |
| `show::security_zones` | VRP 输出官方结构：`<zone> priority is <n>` + ` interface of the zone is (n):` + 成员接口名；IOS 保留 `Zone:` / ` Interface:` |
| `show::security_policies` | VRP 输出 V9 列结构 `RULE ID / RULE NAME / STATE / ACTION / HITS`，并追加平台缺省策略行 `0 default enable deny`；IOS 教学抽象保留 zone-pair 列表并补 `state` |
| `show::security_policy_rule`（新增） | V9 明细：`(<hits> times matched) rule name <n>` + `source-zone` / `destination-zone` / `action` |
| `show::security_sessions`（新增） | VRP `display firewall session table`：ICMP 行（源/目的 IP）与 TCP 行（`ip:port`）+ 状态列；空表输出 ` Info: No session table entry.` |

### 3.4 规格分层（`specs.rs`）

- 新增 **27 条 `Dialect::Vrp` 防火墙基线规格行**，全部 `Implemented + DocumentationOnly + strict`，引用 V1–V9 官方 URL：
  `config/firewall/zone/@0word`、`zone/add/interface/@0if-name`、`zone/undo/add/interface/@0if-name`、`zone/quit`、`zone/return`、`config/security-policy`、`config/security-policy/ip`、`security_policy/rule/name/@0word`、`security_policy/rule/move/@0word/@0word/@0word`、`security_policy/undo/rule/name/@0word`、`security_policy/quit`、`security_policy/return`、`security_rule/source-zone/@0word`、`security_rule/destination-zone/@0word`、`security_rule/action/@0word`、`security_rule/enable`、`security_rule/disable`、`security_rule/undo/source-zone`、`security_rule/undo/destination-zone`、`security_rule/undo/action`、`security_rule/quit`、`security_rule/return`、`exec/display/firewall/zone`、`exec/display/firewall/session/table`、`exec/reset/firewall/session/table`、`exec/display/security-policy`、`exec/display/security-policy/rule/name/@0word`。
- 未模拟命令显式 `NotSimulated`：IOS 的 5 条真实 ZBF 行（Stage 3B 原样保留，`stage3b_courses` 门禁未改）。Comware 安全行仍留在 `lint_report().unassociated` 迁移清单（不隐藏、不冒充已实现）。
- `lint_report(Vrp/Comware/Tmos)`：`duplicates / dangling / drift / strict_missing` 全空（`strict_ok() == true`）。

---

## 4. 工作项 C：校准项逐项证据（§14.3 全表）

| 校准项 | 决策与实现 | 证据 | 测试 |
|---|---|---|---|
| **1. 既有流复查** | **不复查**。命中会话表即转发，策略禁用/删除/改序不影响既有流，直到空闲老化；「立即生效」的唯一路径是清会话表 | V2（后续包不再匹配安全策略）、V3（策略改为禁止后需清表才立即生效，清表即强行中断） | `stage6a_policy_disable_affects_new_flows_only_until_session_cleared`、`stage6a_policy_delete_and_reorder_change_only_future_matching` |
| **2. 同区域转发** | **未命中策略即拒绝**（不是无条件放行）；同区域显式 permit 才放行 | V1 与 V4 的版本分歧见 §2.4，取严格端；正/负用例齐全 | `stage6a_same_zone_traffic_requires_a_policy_positive_and_negative`（负：同区域无策略 0/5；正：`trust→trust permit` 后 5/5） |
| **3. 安全级别回退** | **删除**。ASA 落选，数据面无级别回退；`security_level` 字段仅存档兼容 | C1（ASA 高→低默认放行的原文）仅用于说明被移除的语义；本包不实现 | `security_golden::asa_lineage_commands_are_removed_and_have_no_data_plane`（写入 100/0 级别后仍双向拒绝，只有显式策略才放行）、方言矩阵 |
| **4. 流匹配键** | ICMP：源 IP + 目的 IP + echo id，**双向**；TCP：源 IP + 目的 IP + 源端口 + 目的端口（**双向五元组**，协议隐含 TCP），命中即刷新 `last_ns` | V2（会话表用于反向回包）、V9 | `stage6a_stateful_reply_passes_and_session_table_matches_state`（1 条 ICMP 会话支撑 5 个探测的回包） |
| **5. 状态化回包** | 已承诺协议（ICMP echo / TCP）回包靠会话表放行；**无会话的 echo reply 直接丢弃**；**非 SYN 且无会话的 TCP 直接丢弃** | V2、V3 | `stage6a_non_session_reply_and_unauthorized_new_session_are_dropped`（原始报文注入三条负向） |
| **6. 非会话拒绝** | 同上；另加「untrust→trust 无策略的 echo request 被拒」（未授权新会话） | V1（缺省 any/deny） | 同上 (c) |
| **7. ICMP 会话 / 老化** | ICMP 会话沿用 `security_sessions`，`icmp_idle_ns` / `tcp_idle_ns` 由平台策略对象给出（VRP 1.2e12 ns / 2e10 ns）。**标注为模拟器参数**：缺省老化值未取得官方正文表格，只参照社区样本，不宣称厂商承诺 | V8（老化机制与「持续有流量则不老化」）、社区样本（参数值） | `stage6a_stateful_reply_passes_and_session_table_matches_state`、§1 老化相关断言 |
| **8. 策略禁用/删除/改序** | 三者都有 CLI 入口（`disable`/`enable`、`undo rule name`、`rule move … before|after`），规则匹配只取 `enabled` 且按列表顺序首条命中；`display` 的 STATE 列同步 | V7、V9 | 上面 1、2 两项测试 + `stage6a_display_reflects_configuration_and_config_view_is_replayable` |
| **9. show/display 一致性** | `display firewall zone` / `display security-policy` / `display security-policy rule name <n>` / `display firewall session table` 全部读取唯一状态源；区域成员撤销、规则禁用、清表后输出立即反映 | V5、V6、V9 | 同 8 + `stage6a_summary_round_trip_keeps_zones_and_rule_state`（存档往返保留 `security_zones` 与 `enabled`） |
| **10. 会话表查询/清除命令** | `display firewall session table`（Query）、`reset firewall session table`（Session 效果，走独立模拟器提示通道，设备输出为空） | V3、V6 | `stage6a_policy_disable_…`、WASM `stage6a_dual_channel_and_device_inspection_are_pure` |

**明确否定的旧行为**：① 「首包后永久免检」不再作为未说明的默认——它现在是**有引证、有边界（空闲老化 + 清表）、有正负测试**的基线语义；② 「同区域无条件放行」已删除；③ ASA 安全级别回退已删除。

**L2 边界（如实登记）**：本模拟器的 VRP `display` 命令只挂在用户视图的树中，未实现官方「所有视图可用」。课程与测试用 `return` 显式回到用户视图再查询，并在讲义 08 与测试文件头注明。该边界不改动（属既有 L2 迁移清单）。

---

## 5. §14.3 最低验收场景 → 测试映射

| §14.3 条目 | 测试（`crates/cli-dialects/tests/stage6a_firewall.rs`） |
|---|---|
| 外到内未命中策略时拒绝 | `stage6a_interzone_default_deny_and_permit_are_both_exercised` |
| 内到外命中允许策略时通过 | 同上（5/5） |
| 已承诺协议的状态化回包通过 | `stage6a_stateful_reply_passes_and_session_table_matches_state` |
| 非会话回包 / 未授权新会话被拒 | `stage6a_non_session_reply_and_unauthorized_new_session_are_dropped` |
| 策略禁用后新旧流行为 | `stage6a_policy_disable_affects_new_flows_only_until_session_cleared` |
| 策略删除 / 改序后新旧流行为 | `stage6a_policy_delete_and_reorder_change_only_future_matching` |
| 同区域转发正、负用例 | `stage6a_same_zone_traffic_requires_a_policy_positive_and_negative` |
| `show/display` 与配置及会话状态一致 | `stage6a_display_reflects_configuration_and_config_view_is_replayable` |
| 课程使用明确标注的平台命令；ASA/VRP 拆分在命令树与设备模型完成 | `stage6a_firewall_dialect_matrix_is_bidirectional`、`stage6a_firewall_profiles_are_split_and_evidence_bound`、`stage6a_vrp_firewall_spec_lint_is_clean_and_first_batch_is_strict` |
| 会话表/存档一致性 | `stage6a_summary_round_trip_keeps_zones_and_rule_state` |

---

## 6. 课程与实验

### 6.1 lab10 原文（中英文同一序列，11 步）

`web/src/lib/labs.ts` / `web/src/content/labs-en.ts` 的 `firewall-zone` 任务改为 VRP 基线**严格可顺序执行**的箭头序列，覆盖「配置 → 查询 → 正/负 ping → 禁用规则 + 清表 → 重查询 → 重 ping」的完整闭环：

```
FW1：system-view → interface g0/0/0 → ip address 192.168.1.1 24 → undo shutdown → quit
FW1：interface g0/0/1 → ip address 203.0.113.1 24 → undo shutdown → quit
FW1：firewall zone trust → add interface g0/0/0 → quit
FW1：firewall zone untrust → add interface g0/0/1 → quit
FW1：security-policy → rule name allow-out → source-zone trust → destination-zone untrust → action permit → quit → quit
FW1：return → display firewall zone → display security-policy → display firewall session table
SRV：ping 192.168.1.10            （应先不通）
PC1：ping 203.0.113.10           （应通）
FW1：system-view → security-policy → rule name allow-out → disable → quit → quit → return → reset firewall session table
FW1：display security-policy → display firewall session table
PC1：ping 203.0.113.10           （现在应不通）
```

原文不再出现 `（IOS：zone security …）` 之类的跨方言括注；讲义 `lessons/08-firewall-zone.md` 与 `lessons-en/08-firewall-zone.md` 同步改为 VRP 基线，并明确写出「会话表不复查策略」「策略禁用只影响新流」「清表才立即生效」「同区域也要策略」「ASA 已移除」。Stage 3B 的 IOS 教学抽象说明段原样保留（`zone-pair security ZP` 简化路径仍可执行）。

### 6.2 三层测试（同一套原文解析规则）

| 层 | 位置 | 断言 |
|---|---|---|
| Rust 集成 | `tests/stage6a_firewall.rs::stage6a_lab10_course_original_sequence_runs_top_to_bottom` | 从 `labs.ts` / `labs-en.ts` 解析：中英序列相等、≥10 步；逐条执行 `failures == 0`；PC1→SRV 依次 `5 received` / `0 received`；SRV→PC1 全程 `0 received`；末态规则 `disable`、会话表清空、TCP 流表清空 |
| WASM 契约 | `crates/sim-wasm/src/stage6a_tests.rs`（3 测试） | 同一解析规则在真实 `Sim` 上逐条 `Executed`；`[FW1-zone-trust]` / `[FW1-security-policy-rule-allow-out]` 提示符出现；纯读 3 ✓ + 2 skipped 且 `dump()`/`stats()` 不变；主动 5/5；禁用+清表后策略项与 PC1→SRV 项转 ✗；双通道（查询只有设备输出、`reset` 只有模拟器提示）；非防火墙设备 `InvalidCommand` 且状态不变；IOS 树 `nameif` 已不可解析 |
| 真实浏览器 | 见 §8 | 从 DOM `li` 解析 11 条，逐条在真实终端执行 0 失败；提示符、查询输出、纯读、主动、重判分、双通道、控制台 0 错误 |

### 6.3 检查项（3 纯读 + 2 主动）

| # | 检查项 | 类型 | 说明 |
|---|---|---|---|
| 1 | FW1 G0/0/0 已划入 trust 区域 | 纯读 | 只读 `Port::zone` |
| 2 | FW1 G0/0/1 已划入 untrust 区域 | 纯读 | 同上 |
| 3 | 存在启用的 trust → untrust permit 策略 | 纯读 | **新增 `enabled` 判定**：规则被 `disable` 后不再满足（明细后缀 `（enable）/（disable）`） |
| 4 | SRV → PC1 被缺省策略拒绝 | 主动 | ICMP 负向探测 |
| 5 | PC1 → SRV 放行（回包走会话表） | 主动 | ICMP 正向探测 |

纯读路径不推进虚拟时间、不改变状态；「本实验含主动项：纯读结果全部通过也不会把实验标记为完成」的既有提示保留；只有完整主动判分才标记完成。**实测**：纯读后 `labDoneAfterReadOnly=false`；主动 5/5 后完成。

### 6.4 `cmdmap` security 段与零拒绝

- 新增 6 行（VRP 基线，IOS/Comware 列留空 = 无逐字对应）：`display firewall session table`、`reset firewall session table`、`disable`（规则视图）、`rule move allow-out before deny-all`、`undo rule name allow-out`、`undo add interface g0/0/0`。
- 既有 VRP 行（`firewall zone trust` / `add interface g0/0/0` / `security-policy` / `source-zone` / `destination-zone` / `action permit` / `display firewall zone` / `display security-policy`）随规格行落地自动升级为 `full-support`（**规格驱动**，不是 UI 一刀切）。
- Comware security 行**保持 Stage 4A 边界**：仍为 `pending-verification`（未关联规格 → 迁移清单），`stage3a_cmdmap` 的「security 不得完整支持、也不得变成命令错误」负向门禁未放宽。
- 零拒绝硬断言：`TOTAL_ROWS=433 → 439`，**`REJECTED=0`**（新增 6 个非空单元格全部参与检查）；Comware 单元格 `143 REJECTED=0`。

### 6.5 lab12 `sslvpn-remote` 回归

`security_golden::sslvpn_remote_access_tunnel` 未改动并继续通过：`PC1 → 192.168.1.10` 在未连接 VPN 时仍返回 `0/5`（防火墙缺省拒绝未被放松）；连接后 `5/5`，`display sslvpn session` 同时显示虚拟 IP 与客户端真实 IP。

---

## 7. 旧测试处置逐项（不删测、不放宽）

| 位置 | 处置 | 说明 |
|---|---|---|
| `security_golden::vrp_firewall_zone_policy_stateful` | **保留 + 强化断言** | `display security-policy` 由 `action permit` 自造行改为官方列结构断言（`RULE NAME` / `STATE` / `permit` / 缺省 `default` 行）；拓扑/数据面断言不变 |
| `security_golden::ios_zone_pair_policy` | **保留不变** | Stage 3B 教学抽象路径必须继续可执行 |
| `security_golden::firewall_tcp_denied_without_policy` | **保留不变** | TCP 无策略 → 「无响应」 |
| `security_golden::asa_security_level_default_policy` | **改写为矩阵**（`asa_lineage_commands_are_removed_and_have_no_data_plane`） | 原断言「高安全级别→低安全级别默认放行」与基线冲突。改写后覆盖**更强**：4 条 ASA 命令在 IOS/VRP 树上不可解析；把 `security_level=100/0` 直接写入旧档字段后，未划区域按平台策略直通、一旦划入区域双向仍拒绝、只有显式策略才放行 → 证明级别字段完全没有数据面 |
| `security_golden::firewall_commands_parse_in_all_dialects` | **保留 + 扩充** | 移除 `nameif` / `security-level` 两条（已不存在）；新增 12 条 VRP 基线命令（`undo add interface`、`rule move`、`undo rule name`、`enable`/`disable`、三条 `undo` 条件、会话表查询/清除） |
| `security_golden::sslvpn_remote_access_tunnel` | **保留不变** | lab12 回归（见 §6.5） |
| `security_golden::comware_security_zone_policy_parse_and_display` | **保留不变** | Comware 仍可解析/显示，数据面不在本包 |
| `security_golden::firewall_prompt_modes` | **保留不变** | `[FW1-zone-trust]` / `[FW1-security-policy]` / `[FW1-security-policy-rule-r1]` |
| `stage3b_courses::stage3b_legacy_simplified_zone_pair_path_is_preserved_as_teaching_abstraction` | **未改** | 教学抽象保留的强制门禁 |
| `stage3b_courses::stage3b_comware_security_rows_still_never_full_support` | **未改** | Comware 负向门禁 + 双向隔离 |
| `stage3b_courses::stage3b_ios_spec_lint_has_no_drift_and_zbf_rows_are_documented` | **未改** | 5 条 ZBF recognized-only 行数量与证据未变 |
| `stage3a_cmdmap::*` | **未改**（计入新增 6 行） | `REJECTED=0` 断言未放宽 |
| `config_render_slice.rs` | **未改** | 仍通过；VLAN 渲染范围收紧未影响 IOS 交换机用例 |
| `lab-checks` `SecurityPolicy` 判分 | **增强** | 除 `permit` 外还要求 `enabled`，明细带 `（enable/disable）`；仍为纯读 |
| `scenarios.rs` / `stage2b_flows` / `stage2c_flows` / `l4_scenarios` / `prototype` | **未改** | 全量复跑通过 |
| 全仓测试总数 | **300 → 316**（净 +16：`stage6a_firewall.rs` 13 + `stage6a_tests.rs` 3） | 二进制 48 → 49 |

---

## 8. 真实 WASM / 浏览器证据

方式：`playwright-cli` + `vite preview`（`http://127.0.0.1:4174/`），消费**本次** `wasm-pack build` + `vite build` 产物（`net_sim_bg-DJIKzRln.wasm`，1,661.09 kB / gzip 580.65 kB）。全部结论来自真实 UI 点击、真实 xterm 终端与页内真实 WASM；**未使用 TypeScript 假结果或桩**。每次运行先 `localStorage.clear()` 并重新加载实验，避免语言/存档持久化造成假绿。临时脚本 `s6a-verify.js` 与 `.playwright-cli/` 已删除（见 §10 逐项核对）。

| 验收点 | 实测结果 |
|---|---|
| 实验设备 | `PC1(Host/Ios)`、`FW1(Firewall/**Vrp**)`、`SRV(Host/Ios)` |
| 课程原文（中文） | 从 DOM `li` 解析 **11 条**，标签序列 `FW1×6,SRV,PC1,FW1,FW1,PC1`；逐条在真实终端执行，`prompt_errors=[]`（无 `Error:` / `% Invalid` / `% Unrecognized` / `% Wrong parameter` / `未知命令`） |
| 提示符 | `[FW1-zone-trust]` ✓、`[FW1-security-policy-rule-allow-out]` ✓ |
| 查询输出 | `display firewall zone` → `trust priority is 85` + `GigabitEthernet0/0/0` ✓；`display security-policy` → `RULE ID/RULE NAME/STATE/ACTION/HITS` 含 `allow-out … permit` 与 `default … deny` ✓ |
| 会话表 | ping 前 ` Info: No session table entry.`；ping 后出现 `icmp 192.168.1.10 → 203.0.113.10` ✓（与真实状态一致） |
| 纯读不变 | 3 ✓ 纯读项 + 2 skipped；状态栏 `虚拟时间 12.00s / 已处理事件 49 / 丢弃 5` 前后完全一致；`labDoneAfterReadOnly=false` |
| 主动判分 | **5/5 全绿**，明细 `SRV → PC1 … ping 不通`、`PC1 → SRV … ping 连通` |
| 改策略后重判分 | 执行 `disable` + `reset firewall session table` 后重判分：策略项 ✗（明细 `permit（disable）`）、`PC1 → SRV` ✗（`ping 不通`）；终端内 PC1 两次 ping 依次 `5 received` / `0 received` |
| 双通道 | `display firewall zone` → 只有设备输出、无模拟器提示；`reset firewall session table` → 只有 `[netemu] 已清除 0 条会话表项（策略变更对新建会话立即生效）`、设备输出为空 |
| 控制台 | `Total messages: 0 (Errors: 0, Warnings: 0)`，无 pageerror |

**截图**（`docs/cli-fidelity/`）：

| 文件 | 内容 |
|---|---|
| `stage6a-lab10-active.png` | 实验十：左侧 5 项全绿（3 纯读 + 2 主动，含 `ping 不通` / `ping 连通` 明细）；终端显示 `display security-policy` 官方列结构（`allow-out enable permit` / `default enable deny`）、会话表空→`icmp 192.168.1.10 → 203.0.113.10`；状态栏 `虚拟时间 24.88s / 事件 94 / 丢弃 18` |
| `stage6a-lab10-after-disable.png` | 禁用规则 + 清表后重判分：策略项与 PC1→SRV 项转红；PC1 终端两次 ping `5 received` → `0 received`；状态栏 `42.90s / 139 / 25` |
| `stage6a-lab10-session-table.png` | 双通道对照：`display firewall zone` 设备输出 vs `reset firewall session table` 的 `[netemu]` 模拟器提示 |

**覆盖边界（如实说明）**：截图为单页流程证据，判分结果与任务列表需在左侧面板滚动才完整可见；只在中文界面抓屏（**英文课程原文已在 Rust 与 WASM 两层逐条解析执行**，浏览器层只验证中文 DOM 原文与中英序列一致性由 Rust/WASM 断言承担）；终端 xterm 逐帧渲染未抓屏，改由结构化结果 + `TerminalPanel.svelte` 单一渲染静态门禁共同约束。

---

## 9. 六项门禁（最终真实结果）

| 门禁 | 退出码 | 结果 |
|---|---|---|
| `cargo fmt --all -- --check` | **0** | 先 `cargo fmt --all` 统一格式，复跑 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | **0** | 修掉 1 处 `needless_return`，无 `#[allow]` 规避 |
| `cargo test --workspace` | **0** | **316 passed / 0 failed / 0 ignored，49 个测试二进制**（基线 300 / 48）；课程表 `TOTAL_ROWS=439 REJECTED=0`，`COMWARE_CELLS=143 REJECTED=0` |
| `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | **0** | Rust→WASM `release` 25.68s；wasm-bindgen 声明重新生成；**wasm-opt 正常运行**（`Optimizing wasm binaries with wasm-opt`，本机未复现 `os error 5`，**未关闭优化**）；`:-) Done in 31.55s` |
| `npx --yes pnpm@10 --dir web run check` | **0** | `tsc --noEmit` 0 错误 |
| `npx --yes pnpm@10 --dir web run build` | **0** | `✓ built in 4.04s`，产物 `net_sim_bg-DJIKzRln.wasm`（1,661.09 kB / gzip 580.65 kB） |

---

## 10. HEAD、工作区、临时产物与未完成项

| 项 | 状态 |
|---|---|
| HEAD | `3d784e022c8e70ca3c4aafbce65edd1220064ed7`（未变） |
| 分支 | **detached HEAD** |
| 提交 / 推送 | **未提交、未推送** |
| 工作区 | **33 个已跟踪 M + 23 组未跟踪**（起点 32 M + 21 组；本包新增 `tests/stage6a_firewall.rs`、`src/stage6a_tests.rs` 2 组未跟踪 + `tests/security_golden.rs` 1 个 M；`docs/cli-fidelity/` 内新增本记录与 3 张截图，仍计入同一未跟踪条目） |
| 构建目录 | 独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-6a-target`；未使用 `-j 1`（本机无编译挂起） |
| 临时产物（**已按真实文件系统核对**） | `s6a-verify.js`：`Test-Path` = False（已删除）；`.playwright-cli/`：`Test-Path` = False（已删除）；`web/preview.log`：先停止占用 4174 端口的进程再删除，`Test-Path` = False；`s6a-home.yaml` / `s6a-lab10.yaml`（`snapshot --filename` 落在仓库根目录的残留）：`Test-Path` = False。preview 服务与浏览器均已关闭；仓库根 `git status` 中无任何 `*.yaml` 残留 |

**未完成项（明确后置，未开始）**

| 项 | 边界 |
|---|---|
| 完整 IOS ZBF 数据面与子模式提示符（`config-cmap` / `config-pmap` / `config-sec-zone-pair`） | 属 **6B**。本包保留 Stage 3B 的 5 条 recognized-only 语法行与 `zone`/`zone-pair` 教学抽象，**不冒充** ZBF 数据面 |
| Cisco ASA 作为独立方言/Profile | 本包**移除**其 CLI 与数据面回退，只留存档字段与讲义对照；若未来要支持 ASA，需按 §2.3 理由重新立项重建接口/策略/`conn` 表 |
| Comware 安全数据面（`security-zone` / `security-policy` 的转发与状态化） | 保持 Stage 4A 边界（可解析、可显示）；`cmdmap` Comware security 行仍为 `pending-verification` |
| 同区域默认动作的版本收敛 | §2.4 已登记 V1/V4 分歧，取严格端；待 eNSP（USG6000V）校准后改 `FirewallPolicy::intra_zone_default_permit` 一个值 |
| 未划分区域接口的真实处置 | 证据不足，显式 `UnzonedTraffic::PassThrough` + 标注待验证 |
| 华为会话缺省老化值 | 无官方正文表格；模拟器参数（社区样本级），已在 §4 第 7 项标注，不作保真承诺 |
| VRP `display` 的「所有视图可用」 | 既有 L2 边界；课程/测试用 `return` 显式回用户视图 |
| NAT / 服务匹配 / 策略属性（主方案 §14.2 末项） | 本包未展开：既有 NAT 走 `nat_egress`/`nat_entries`，与安全策略的先后顺序未按基线校准，留在后续防火墙包 |
| C6b、`run()` 前端事件动画 | 本包明确不做 |

---

## 11. 阶段 6 是否完成的明确结论

**结论：主方案 §15「阶段 6：防火墙」在交接包（Stage 6A）范围内完成并验收通过，可以关闭本包；但 IOS ZBF 完整数据面与子模式保真属 6B，本包不冒充完成。**

依据：

1. **先决策后实施**：先完成 V1–V9（华为）+ C1（Cisco ASA）官方正文核验与评估矩阵（19 : 10），再落实现。基线 = 华为 VRP 安全策略；ASA 落选并按「非基线命令移除」显式处置，无不名共存。
2. **血统拆分到位**：ASA `nameif`/`security-level` 从 IOS 树移除、数据面回退删除、`config_render` 停止渲染；VRP 基线命令按官方语法归位（含 `undo`/`enable`/`disable`/`rule move`/会话表查询与清除）；Comware 保持 Stage 4A 边界；双向接受/拒绝矩阵断言通过；`DeviceKind::Firewall` 拆出独立 Profile 与证据常量。
3. **校准逐项有据**：既有流不复查（V2/V3，配 `reset` 立即生效路径）、同区域需策略（V4 + 版本分歧留痕）、安全级别回退删除、流匹配键明确定义（双向 ICMP 三元组 / 双向 TCP 五元组）、非会话回包与非 SYN 报文拒绝、ICMP 会话与老化标注为模拟器参数、`display` 与真实状态一致 —— 每项都有正/负测试与出处。
4. **§14.3 全表闭环**：10 个最低验收场景逐条映射到具名测试，全部通过。
5. **课程与实验闭环**：lab10 中英文改为同一 VRP 严格序列（11 步，含禁用+清表闭环），三层测试（Rust 13 测试 / WASM 3 测试 / 真实浏览器）从课程原文解析并执行 0 失败；检查项 3 纯读 + 2 主动，纯读不推进、只有完整主动判分标记完成、改策略后重判分反映新旧流变化；`cmdmap` security 行按规格升级且 `REJECTED=0` 未放宽；`lab12` 的「未连接前不通」回归通过。
6. **门禁全绿**：316 Rust 测试 / 49 二进制、WASM 逐段（含 wasm-opt）通过、Web check/build 通过；真实浏览器消费最终 WASM，控制台 0 错误，3 张截图留存；Stage 2/3/4/5 行为未回归。

**收口边界**：§10 列出的 IOS ZBF 完整数据面（6B）、ASA 独立方言、Comware 安全数据面、NAT/服务匹配与安全策略的先后顺序校准、同区域默认动作的 eNSP 版本收敛、缺省老化值取证、`display` 全视图可用——均未在本包完成，也不得被读成已完成。本包在上述范围内停止，未进入 C6b 或 6B。
