# 阶段 4A · Comware V7 官方证据 / 独立命令树 / 原生 L2·L3 闭环 / 阶段 4 验收 · 实施记录

> 执行日期：2026-11-13
> 基线：`3d784e022c8e70ca3c4aafbce65edd1220064ed7`（detached HEAD，未提交、未推送）
> 上一记录：`docs/cli-fidelity/stage3b-implementation-record.md`（保留，**未回写**）
> 范围依据：`netemu-stage4a-implementation-handoff.md`；技术约束取主方案 §4.3 / §4.4 / §5 / §6 / §10 / §12 / §15
> 明确不做：C6b、F5 TMOS、完整防火墙 / ASA / VRP 拆分、完整 IOS ZBF、Comware 全功能扩展、`run()` 前端事件动画
> 停止点：完成独立拆树、首批原生闭环、实验二与课程表、OSPF 区域视图退出层级、阶段 4 验收后停止
> 复核补修：2026-11-13 同日，见 §0；补修通过后才可关闭阶段 4

---

## 0. 复核补修（2026-11-13，同日）

### 0.1 复核发现的阻塞问题（用户复核，属实）

`labs.ts` / `labs-en.ts` 实验二第 1 步要求 `system-view → vlan 10 → quit → vlan 20 → quit`。补修前 `cmd::create_vlans` 只创建 VLAN、不切换 CLI 模式，`quit` 从系统视图直接退回用户视图 `<SW1>`，于是 `vlan 20` 得到 `InvalidCommand`；第 2、3 步在两个物理接口、两个 SVI 之间也没有写 `quit`，按箭头顺序执行会停在上一个接口视图。旧测试与浏览器脚本使用的是另行手写的 **17 条「验收命令」**（`vlan 10` 后直接 `vlan 20`、接口间无 quit），**并非课程原文序列**，因此「17 条全 Executed」没有覆盖该缺口。

### 0.2 补修方式（不改测试绕过，按官方证据实现真实层级）

- **E11 官方正文**：H3C `02-Layer 2—LAN Switching Command Reference / 09-VLAN configuration commands`（WX5540E，Comware V7，R2607P26-6W101，2019-06），`https://wwwsg.h3c.com/en/d_201906/1198196_294551_0.htm`，全文核验：`vlan` 词条「**Use `vlan vlan-id` to create a VLAN and enter its view or enter the view of an existing VLAN.**」，Views=**System view**，示例 `<Sysname> system-view / [Sysname] vlan 2 / [Sysname-vlan2]`；`name` / `description` / `port` 词条 Views=**VLAN view**；`display vlan` Views=**Any view**；`port access vlan` 官方示例即为 `[Sysname] vlan 3 / [Sysname-vlan3] quit / [Sysname] interface gigabitethernet 1/0/1 / [Sysname-GigabitEthernet1/0/1] port access vlan 3`。配合 E6（`quit` 退回上一级视图、`return` 从任意非用户视图直达用户视图）成立。**不建立逐字输出黄金**（仍无设备样本）。
- 新增共享 `Mode::Vlan`（tag `vlan`）与 `SubCtx.vlan`；Comware `vlan <id>` 复用共享 `cmd::create_vlans` 写状态后进入 VLAN 视图，提示符 `[SW1-vlan10]`；`quit` 回 `[SW1]`、`return` 回 `<SW1>`；VLAN 视图内 `display` / `name` 可用，接口视图命令被拒且状态不变。VRP / IOS 的 `vlan` 行为不变（只补最小退出集合兜底，不承诺 VRP 保真，也不进入「VRP 拆分」范围）。
- `Trees` 增加 `vlan` 根列表并纳入 `roots_for` 稳定 ID 派生；`prompt` / `mode_from` / `h_quit` / `h_exit` 全部补齐新模式的穷尽分支；新增 `vlan/quit`、`vlan/return` 两条 Comware 规格行，使层级退出属于首批闭环而非迁移清单。
- `config_render` 的 Comware VLAN 段每输出一条 `vlan <n>` 后补 ` quit`，使 `display current-configuration` 的输出**仍可逐行重执行还原同一状态**（§5 契约不变）；这是「配置呈现 + 可重放序列」的有意设计，不冒充厂商逐字文本。
- `labs.ts` / `labs-en.ts` 第 2、3 步补齐接口与 SVI 之间的 `quit`，前三步写成严格可执行的箭头序列；中英文解析结果完全一致（**19 条**）。
- 新增 **按课程原文逐条执行** 的 Rust 集成测试、真实 WASM `Sim` 契约测试与真实浏览器测试：三者都从 `labs.ts` / `labs-en.ts` / DOM `ol.task li` **解析**命令，不再使用另写的验收清单。6 处既有 Comware 执行序列按官方层级补 `quit`（详见 §7），`config_render` 重执行与 `vlan_golden` 输出索引同步修正。
- **计数修正（用户复核，属实）**：VLAN 课程表 VRP 列实测 **2 行完整支持 + 5 行待验证**（此前误记为「VRP 同 7 行为待验证」）；安全课程表 Comware 实测 **8 行待验证、0 行完整支持**（此前误记为 7 行）。偏差来源：旧脚本按 `innerText.includes('完整支持')` 统计，命中了说明文字「**不得标为完整支持**」，且 security key 正则漏行；已改为按表格单元格**徽标前缀**精确统计（`✓ 完整支持` / `? 待验证`），并用中英文界面识别断言排除语言持久化干扰。

### 0.3 补修后结论

阻塞已消除：中英文课程原文各 19 条命令在真实 WASM 上 **0 失败**，出现官方 VLAN 视图提示符 `[SW1-vlan10]` / `[SW1-vlan20]`，末态 `[SW1]`，主动判分 3/3；六项门禁与浏览器复核全绿，见 §8 / §9 / §11。

---

## 1. 现场保护与基线复跑

| 项 | 结果 |
|---|---|
| `git rev-parse HEAD` | `3d784e022c8e70ca3c4aafbce65edd1220064ed7` |
| `git branch --show-current` | 空 → **detached HEAD（no branch）** |
| 起点工作区 | **27 个已跟踪 M + 16 组未跟踪**（阶段 0/1、2A/2B/2C、3A/3B 改动全部保留） |
| 现场保护 | 全程未 `reset` / `clean` / 覆盖 / 删测 / 注释测试 / 放宽断言；未提交、未推送 |
| 基线 `cargo test --workspace --quiet` | 全部测试二进制 `ok`，`EXIT=0`（交接方 276 项 / 47 个测试二进制） |
| 构建目录 | 独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-4a-target` |
| 阅读范围 | 交接文档、主方案 §4.3/§4.4/§5/§6/§10/§12/§15、Stage 3B 记录、`lib.rs`/`huawei.rs`/`specs.rs`/`config_render.rs`/`show.rs`/`ifname.rs`、`profile.rs`/`device.rs`/`summary.rs`/`engine.rs`、`sim-wasm/src/lib.rs`/`stage3a_tests.rs`/`lab_defs.rs`、`lab-checks/src/lib.rs`、`cmdmap.ts`/`labs.ts`/`labs-en.ts`/`CourseView.svelte`/`LeftPanel.svelte`/`i18n-dict.ts`、全部 25 个含 `Dialect::Comware` 的测试文件 |

---

## 2. Comware V7 官方证据（访问日期均为 2026-11-13）

### 2.1 证据表

全部为 H3C 官方网页**正文全文核验**（非搜索快照摘要）。证据等级统一为 `DocumentationOnly`：没有带版本设备、HCL 或可采信设备样本，**不建立任何逐字厂商输出黄金、不升级证据等级**；本包只建立语法、CLI 模式、结构化状态、配置视图与数据面断言。

| # | 文档标题 / 章节 | URL | 版本 / 更新 | 全文边界 |
|---|---|---|---|---|
| E1 | *02-Layer 2—LAN Switching Command Reference* — **09-VLAN configuration commands**（H3C WX5540E Access Controller Switching Engine Command References (R2607P26)-6W101） | `https://wwwsg.h3c.com/en/d_201906/1198196_294551_0.htm` | Comware V7，R2607P26-6W101，2019-06 | 全文：`vlan`/`undo vlan`、`interface vlan-interface`/`undo interface vlan-interface`、`ip address`/`undo ip address`、`shutdown`/`undo shutdown`、`display vlan`、`port access vlan`/`undo port access vlan`、`port link-type`、`port trunk permit vlan`/`undo port trunk permit vlan`、`port trunk pvid` 命令词条 |
| E2 | **04-二层技术-以太网交换配置指导 / 07-VLAN映射配置**（H3C S5130S-EI-G 系列交换机 配置指导-R7748Pxx-6W100） | `https://www.h3c.com/cn/d_202212/1745029_30005_0.htm` | Comware V7，R7748Pxx-6W100，2022-12 | 全文 §1.5–§1.11：默认链路类型、Trunk 默认放行列表、`port trunk permit vlan` 多处配置步骤 |
| E3 | **04-Comware 7 CLI-based configuration examples (AC+fit AP deployment) / 019-Remote Portal Authentication Configuration Examples**（H3C Wireless Products All-in-One-6W102） | `https://www.h3c.com/en/d_202408/2257333_294551_0.htm` | Comware 7，6W102，2024-08 | 全文：AC/Switch 配置过程与 *Configuration files* 段；含提示符、接口名、VLAN/SVI/Trunk/Access/静态路由 |
| E4 | **04-Comware 7 CLI-based configuration examples / 006-Auto AP Configuration Examples** | `https://www.h3c.com/en/d_202408/2257320_294551_0.htm` | Comware 7，6W102，2024-08 | 全文：`undo port trunk permit vlan 1` 与 VLAN/SVI 配置 |
| E5 | **01-基础配置命令参考 / 3.3 display current-configuration**；**06-配置文件管理 / 1.11 配置文件管理显示和维护** | `https://www.h3c.com/cn/d_201706/1001779_30005_0.htm`；`https://www6.h3c.com/cn/d_201903/1156317_30005_0.htm` | Comware V7 | 全文：`display current-configuration` 显示设备当前生效配置 |
| E6 | **01-基础配置指导 / 1.1.3 返回上一级视图 / 1.1.4 返回用户视图**；**ComwareV7 常用命令行视图介绍 / 1.3 退出视图** | `https://www.h3c.com/cn/d_202009/1339568_30005_0.htm`；`https://wwwsg.h3c.com/cn/Service/Document_Software/Document_Center/Home/Switches/00-Public/Command/Command_Manual/ComwareV7_Command/?CHID=561055` | Comware V7，2025-12 更新 | 全文：`quit` 从当前视图返回**上一级视图**；`return` 从任意非用户视图直接返回用户视图 |
| E7 | **07-三层技术-IP路由命令参考**（`ip route-static` / `undo ip route-static`）；**04-三层技术-IP路由配置指导 / 09-静态路由配置**（显示与维护） | `https://www.h3c.com/cn/d_201803/1071543_30005_0.htm`；`https://www.h3c.com/cn/d_202511/2702674_30005_0.htm` | Comware V7 | 全文：静态路由写入/撤销、`display ip routing-table` |
| E8 | **05-三层技术-IP路由配置指导 / 07-OSPF配置 / 1.4 配置 OSPF 基本功能** | `https://www.h3c.com/cn/d_202504/2394061_30005_0.htm` | Comware V7，R7752-6W101，2025-04 | 全文：`ospf [process-id]` → `area area-id` → `network ip-address wildcard-mask`；明确 `network` **仅支持在 OSPF 区域视图下配置** |
| E9 | **Packet filter commands / 07-Packet filter commands**；**10-ACL and QoS Command Reference / display packet-filter** | `https://www.h3c.com/en/d_202506/2515724_294551_0.htm`；`https://wwwsg.h3c.com/en/d_201408/839455_294551_0.htm` | Comware V7 | 全文：Comware 用 `packet-filter`（不是 VRP 的 `traffic-filter`）在接口应用 ACL |
| E10 | **03-二层技术-以太网交换命令参考**（H3C S12500-F CR(R1109)-6W100） | `https://www.h3c.com/cn/Service/Document_Software/Document_Center/Switches/Catalog/S12500/S12500-F/Command/Command_Manual/H3C_S12500-F_CR(R1109)-6W100/03/201906/1203212_30005_0.htm` | Comware V7，R1109-6W100 | 全文：`port trunk permit vlan` / `undo port trunk permit vlan` / `port link-type` 词条与使用指导 |
| E11 | **02-Layer 2—LAN Switching Command Reference / 09-VLAN configuration commands**（H3C WX5540E，Comware V7，R2607P26-6W101） | `https://wwwsg.h3c.com/en/d_201906/1198196_294551_0.htm` | Comware V7，R2607P26-6W101，2019-06 | 全文复核（2026-11-13 补修）：`vlan` 词条「Use `vlan vlan-id` to create a VLAN and **enter its view**」、Views=System view、示例 `[Sysname-vlan2]`；`name`/`description`/`port` Views=VLAN view；`display vlan` Views=Any view；`port access vlan` 官方示例含 `[Sysname-vlan3] quit` |

### 2.2 官方语法与状态语义（正文要点，不复制整篇文档）

| 主题 | 官方结论 | 依据 |
|---|---|---|
| VLAN 创建 / 删除 | `vlan { vlan-id1 [ to vlan-id2 ] \| all }`、`undo vlan { … }`；系统视图；VLAN ID 范围 **1–4094**；VLAN 1 不可创建/删除；默认只有 VLAN 1 | E1 (`vlan` 词条) |
| Access 口 | `port link-type { access \| hybrid \| trunk }`；**缺省所有端口均为 Access、PVID=1、属于 VLAN 1**；`port access vlan vlan-id`、`undo port access vlan` 恢复 VLAN 1 | E1 (`port link-type`/`port access vlan` 词条) |
| Trunk 默认列表 | **「A trunk port allows only packets from VLAN 1 to pass through.」** / 「缺省情况下，Trunk 端口只允许 VLAN 1 的报文通过」 | E1 (`port trunk permit vlan`)、E2 (§1.5/§1.6/§1.7/§1.8/§1.9)、E10 |
| permit 替换还是累加 | **累加**：「If you use the `port trunk permit vlan` command multiple times, the trunk port carries the set of VLANs specified by `vlan-list` in each execution.」；`undo port trunk permit vlan { vlan-list \| all }` 移除指定 VLAN；`port trunk permit vlan all` 放行全部 | E1 (`port trunk permit vlan`)、E10 |
| permit 是否保留 VLAN1 | E3 的 *Configuration files* 中，只执行了 `port trunk permit vlan 100 200`，最终配置为 **`port trunk permit vlan 1 100 200`** → VLAN 1 保留；E4 需显式 `undo port trunk permit vlan 1` 才移除 | E3、E4 |
| 未先 `port link-type trunk` | E1 中 `port trunk permit vlan` 视图为「二层以太网接口视图」，官方示例一律先 `port link-type trunk`；本包按主方案 §12 只做**结构/状态断言**（拒绝且端口模式不变），不编造逐字报错 | E1、E2、E10、主方案 §12 |
| SVI | `interface vlan-interface vlan-interface-id`（系统视图，1–4094，须先创建 VLAN）；`undo interface vlan-interface`；VLAN 接口视图 `ip address ip-address { mask \| mask-length }`；`shutdown`/`undo shutdown`，**缺省未被手工关闭** | E1、E3 |
| 接口默认状态 | 「A VLAN interface is not manually shut down」；`port` 缺省均属 VLAN 1 且为 Access（E1）。项目侧 Comware 交换机物理口默认 up、SVI 创建后 admin_up=true，与官方一致；路由器物理口默认状态无 Comware V7 正文证据，保持项目既有策略并标注待验证 | E1、E3 |
| 接口名 / 提示符 | `GigabitEthernet1/0/1`、`[Switch-GigabitEthernet1/0/1]`、`[Switch-vlan100]`、`[Switch-Vlan-interface200]`、`<Switch>` / `[Switch]` | E3 |
| 静态路由 | `ip route-static dest-address { mask-length \| mask } next-hop-address`、`undo ip route-static …`、`display ip routing-table` | E3、E7 |
| 查询 / 配置视图 | `display vlan [vlan-id …]`（任意视图）、`display current-configuration`（显示当前生效配置） | E1、E5 |
| OSPF 层级 | `system-view` → `ospf [process-id]` → `area area-id`（进入 OSPF 区域视图）→ `network ip-address wildcard-mask`（**仅区域视图**）；`quit` 退回上一级视图，`return` 直达用户视图 → 区域视图两级退出成立 | E6、E8 |
| ACL 应用 | Comware 为 `packet-filter`，VRP 为 `traffic-filter` | E9、华为 VRP 文档基线 |
| VLAN 视图层级 | `vlan <id>` 创建 VLAN **并进入 VLAN 视图** `[Sysname-vlan<id>]`；`quit` 退回上一级（系统视图），`return` 直达用户视图；`name` / `description` / `port` 属 VLAN 视图，`display` 属任意视图；官方 `port access vlan` 示例在 `vlan <id>` 之后显式写 `quit` 再进接口视图 | E1/E11、E6 |

### 2.3 证据边界

- 无带版本 Comware V7 真机 / HCL / 设备输出样本：`display vlan`、`display ip routing-table`、`display current-configuration` 的**逐字输出**与报错文本继续列为「输出待验证」；本包只用结构/状态断言，不新增逐字厂商黄金。
- E1 为 WX5540E 交换引擎命令参考（Comware V7 家族），E2/E7/E8 为 S5130S-EI-G / RA5300 等 Comware V7 配置指导，E3/E4 明确标注 Comware 7。版本族差异以「通用 Comware V7 文档基线」表述，**不冒充某个精确设备版本**。
- E6 的 `quit` 是通用 CLI 视图规则，E8 给出 OSPF 进程/区域/子视图层级；两者组合证明「区域视图 quit → 进程视图 → quit → 系统视图」，不是靠项目自身代码推断。

---

## 3. VRP / Comware 双向差异矩阵

「不得互用」= 在该方言树中不存在该字面量，跨方言输入应得 `InvalidCommand`；本包只对首批闭环命令承诺状态语义，其余按「未迁移 / 未模拟 / 待验证」处理，不伪造完整支持。

| 命令族 | VRP（保留在 `huawei::build()`） | Comware（保留在 `comware::build()`） | 隔离要求 / 本包处置 |
|---|---|---|---|
| VLAN 批量创建 | `vlan batch <vlan-list>` | 无（`batch` 不进入 Comware 树） | 不得互用；Comware 用 `vlan <id>` |
| VLAN 单个创建 | `vlan <id>`（通用，官方 VRP 亦有） | `vlan <id>` | 有证据的通用命令，双向保留 |
| VLAN 视图层级 | 本包 VRP `vlan <id>` **仍不切换视图**（迁移期共享语法，不承诺 VRP 保真；不进入「VRP 拆分」范围） | `vlan <id>` 进入 `[SW1-vlan<n>]`；`quit`→`[SW1]`；`return`→`<SW1>` | E1/E11、E6；阶段 4A 只对 Comware 实现，见 §0 |
| VLAN 删除 | `undo vlan <id>`（通用） | `undo vlan <id>` | 新增；官方 E1 |
| SVI | `interface Vlanif<n>` | `interface Vlan-interface<n>` | 不得互用；E1/E3 |
| Access 划 VLAN | `port default vlan <n>` | `port access vlan <n>` | 不得互用；E1/E3 |
| 撤销 Access | `undo port default vlan` | `undo port access vlan` | 不得互用；E1 |
| Trunk 模式 | `port link-type trunk` | `port link-type trunk` | 通用，双向保留；E1 |
| Trunk 放行列表 | `port trunk allow-pass vlan <list>` | `port trunk permit vlan <list>` | 不得互用；E1/E2/E10 |
| Trunk 移除放行 | （VRP `undo port trunk allow-pass vlan`） | `undo port trunk permit vlan <list>` / `… all` | Comware 原生否定形式；E1/E10 |
| 聚合接口 | `interface Eth-Trunk<n>` | `interface Bridge-Aggregation<n>` | 不得互用；本包不扩展聚合功能，仅保留既有语法 |
| 加入聚合组 | `eth-trunk <n>` | `port link-aggregation group <n>` | 不得互用 |
| 聚合查询 | `display eth-trunk [n]` | `display link-aggregation [summary\|verbose]` | 不得互用 |
| VTY 视图 | `user-interface vty <a> <b>` | `line vty <a> <b>` | 不得互用 |
| 本地用户口令 | `password irreversible-cipher` | `password simple` / `class manage` / `authorization-attribute user-role` | 不得互用 |
| 安全区域 | `firewall zone <z>` / `add interface` | `security-zone name <z>` / `import interface` | 不得互用；security 主题仍为 Stage 4 门禁范围（不得完整支持） |
| ACL 应用到接口 | `traffic-filter <dir> acl <n>` | `packet-filter`（recognized-only，本包未模拟数据面） | E9；不作为 VRP 命令混入 Comware |
| 提示符 / 报错 | `Error: …` | `% …` | 已在 `lib.rs::prompt` / `format_error` 按方言分叉 |
| 接口名 | `GigabitEthernet0/0/x` | `GigabitEthernet1/0/x` | 已在 `ifname.rs` 按方言分叉 |
| 通用命令（保留） | `sysname`、`system-view`、`quit`/`return`、`ip address`、`undo shutdown`、`ip route-static`/`undo ip route-static`、`display current-configuration`/`display vlan`/`display ip routing-table`、`ospf <pid>`/`area <a>`/`network …`、`ping` | 同左 | 均为 Comware V7 官方语法（E1/E3/E5/E6/E7/E8），双向保留 |

**非首批的迁移期共享语法**（BGP / IS-IS / STP / VRRP / VXLAN / LTM / route-policy / IPsec / mgmt 等）：本包不逐条重写厂商语法；它们在 Comware 树上继续以「未关联规格 → 待验证 / 仅识别」呈现，绝不显示 `full-support`，且不作为 Comware 官方语法证据。该项列入 §11 未完成项。

---

---

## 4. 独立命令树与共享边界（实际代码）

| 文件 | 改动 |
|---|---|
| `crates/cli-dialects/src/comware.rs`（新增） | 独立 `build() -> Trees`。只含 Comware V7 字面量：`vlan <id>` / `undo vlan`、`interface Vlan-interface` / `Bridge-Aggregation`、`port access vlan` / `undo port access vlan`、`port trunk permit vlan` / `undo … {list\|all}`、`line vty`、`security-zone name` / `import interface`、`display link-aggregation` / `display interface brief`、`packet-filter`、`password simple` / `class manage` / `user-role`。`undo_branch(interface_view: bool)` 按系统视图 / 接口视图分别挂官方否定形式。 |
| `crates/cli-dialects/src/huawei.rs` | 删除全部 Comware 专属字面量（`Vlan-interface`、`Bridge-Aggregation`、`port access vlan`、`permit`、`link-aggregation mode`、`line vty`、`security-zone name`、`import interface`、`display link-aggregation`、`class manage`、`password simple`、`user-role`、`undo port link-aggregation group`），只保留 VRP 语法。 |
| `crates/cli-dialects/src/lib.rs` | 顶部 `pub mod comware;`；`trees()` 中 `Dialect::Comware` 从 `huawei::build()` 改为 **`comware::build()`**。新增共享 `cmd::no_vlan` / `cmd::no_trunk_permit` / `cmd::recognized_only`；三者只读写 `sim-core` 状态或 `Ctx.notices`，不含 `if dialect`。 |
| `crates/sim-core/src/summary.rs` | 新增 `Net::remove_vlan(dev, vlan)`：VLAN1 不可删，删除时同步清理 SVI / ARP / MAC / 待发帧 / 端口 PVID 与 Trunk 放行列表。 |
| `crates/sim-core/src/profile.rs` | Comware Profile 从「阶段 0 通用模型（待验证）」改为 `H3C Comware V7 Documentation Baseline (E1-E10)`，`EvidenceLevel::DocumentationOnly`；`trunk_initial / keeps_vlan1 / requires_trunk_mode / additive` 拆为独立 `Dialect::Comware` 分支并逐项附 E1/E2/E3/E10 证据；交换设备 `L2SwitchPort` 由 E1/E2 判为 `Supported`；路由物理口默认状态无正文证据，保守保留 `false` 并标注待验证。新增 `COMWARE_V7_BASELINE` 等证据常量。 |
| `crates/cli-dialects/src/config_render.rs` | `render()` 由 `Ios / _` 两支拆成 `Ios / Vrp / Comware` 三支：Comware 逐条 `vlan <id>`、`port access vlan`、`port trunk permit vlan`、`interface Vlan-interface<n>`（由 `ifname::svi_canonical` 生成）、OSPF 块两次 `quit`、`interface Bridge-Aggregation<n>` + `link-aggregation mode`；不再输出 `vlan batch` / `port default vlan` / `allow-pass` / `Vlanif` / `Eth-Trunk` / `firewall zone`。 |
| `crates/cli-dialects/src/ifname.rs` | `svi_short(Comware)` 由 `Vlanif<n>` 修正为 `Vlan-interface<n>`。 |
| `crates/cli-dialects/src/specs.rs` | 新增 10 个 H3C 官方证据常量与 **30 条 `Dialect::Comware` `SpecRow`**：首批闭环命令挂 `Effect` / `Capability` / `SemanticSupport::Implemented` / `DocumentationOnly` / 承诺契约 / `strict`；`packet-filter` 挂 `NotSimulated` + `Effect::Unimplemented`（仅识别）。 |
| `crates/lab-checks/src/lib.rs` | `SviIp` 判分文案由 `Vlanif{n} = …` 改为方言中性 `VLAN{n} SVI = …`，避免 Comware 实验二显示 VRP 名称。 |
| §0 补修 `crates/cli-engine/src/lib.rs` | 新增 `Mode::Vlan`（tag `vlan`）与 `SubCtx.vlan`：模式编号三方言共用，仍不改数据面。 |
| §0 补修 `cli-dialects/src/{lib,comware,huawei,ios}.rs` | `Trees.vlan` 根列表 + `root_for` / `roots_for` / `prompt` 穷尽分支；Comware 新增 `h_vlan_enter`（复用 `cmd::create_vlans` 写状态，**只追加会话层级**）；`h_quit` / `h_exit` 把 `Mode::Vlan` 归入「退回 Config」；VRP / IOS 只给最小退出集合兜底，不改其 `vlan` 行为。 |
| §0 补修 `crates/sim-wasm/src/lib.rs` | `mode_from` 支持 `"Vlan"`，会话级纯读 inspection 可查 VLAN 视图命令。 |
| §0 补修 `crates/cli-dialects/src/specs.rs` | 新增 `vlan/quit`、`vlan/return` 两条 Comware 规格行（`ModeSwitch` / `Implemented` / `DocumentationOnly` / strict）；`config/vlan` 契约描述同步为「创建并进入 VLAN 视图」。 |
| §0 补修 课程文本 / 配置渲染 / 测试 | `labs.ts`、`labs-en.ts` 任务 2/3 补 `quit` 形成 19 条严格序列；`config_render` Comware VLAN 段每条后补 ` quit` 保证可重放；新增课程原文解析 + 顺序执行的 Rust / WASM / 浏览器测试。 |

**共享 / 独立边界**：`CNode` 仍是语法、帮助、补全和稳定 ID 的唯一来源；两棵树共享 `cmd` handler、`ParamKind` 参数工具、`specs` 规格机制、`judge()` 四态映射、`sim-core` 唯一状态与 `ifname/show/prompt/format_error` 的方言分支。**没有第二套数据面、后端或语法定义**；`comware::build()` 不调用 `huawei::build()`（测试用 `std::ptr::eq(trees(Comware), trees(Vrp))` 直接证伪）。

`lint_report(Dialect::Comware)`：`strict_ok() = true`，`duplicates / dangling / drift / strict_missing` 全空，`strict_total > 10`，`rows(Comware).len() = 30`（其中 `vlan/quit` / `vlan/return` 为 §0 补修新增）。

---

## 5. 首批原生功能与配置渲染验收

| 闭环项 | 实现与断言 | 证据 |
|---|---|---|
| VLAN 创建 / 删除 / 非法 ID | `vlan 10/20` 写 `SwitchState.vlans`；`undo vlan 30` 清理派生状态；`vlan 0 / 4095 / 99999`、`undo vlan 1`、`undo vlan 99` 全部 `InvalidCommand` 且状态指纹不变 | E1 |
| VLAN 视图与 `quit` / `return` 层级 | `vlan 10` → `Mode::Vlan` / `[SW1-vlan10]`；VLAN 视图内 `display vlan` 与 `name` 可执行、`port access vlan` 被拒且状态不变；`quit` → `[SW1]`（可继续 `vlan 20` / `interface …`）；`return` → `<SW1>`，`vlan 30` 随后被拒；`SubCtx.vlan` 记录当前 VLAN | E1/E11、E6 |
| 课程任务原文顺序执行 | 中英文课程前三步解析出的 **19 条**命令逐条执行 `0` 失败，出现 `[SW1-vlan10]` / `[SW1-vlan20]`，末态 `[SW1]`；状态（VLAN 10/20、pvid 10/20、两个 SVI）、`display current-configuration` 与 PC1→PC2 `5/5` 全部成立；中英文序列完全相等 | E1/E11、E6、§0 |
| Access / PVID / 撤销 | `port link-type access` + `port access vlan 10` 写 `Port.mode/pvid`；`undo port access vlan` 恢复 PVID 1 | E1/E3 |
| Trunk 默认与 permit | 进入 trunk 后 `trunk_allowed == Some([1])`；`permit vlan 10` → `[1,10]`，再 `permit vlan 20` → `[1,10,20]`（累加）；`undo … vlan 20` → `[1,10]`；`undo … all` → `[1]`；Access 口直接 `permit` 被拒绝且模式不变 | E1/E2/E3/E10 |
| Trunk 正/负流量 | Rust 集成拓扑（两台 Comware 交换机经 Trunk 互联、同 VLAN10 双主机）：默认列表 → `5 transmitted, 0 received`；两端 permit VLAN10 → `5 transmitted, 5 received`；撤销一端 → 再次 `0 received` | E1/E2 |
| SVI 与跨 VLAN 路由 | `interface Vlan-interface 10/20` + `ip address` 写 `SviState`；PC1→PC2 `5/5`；`shutdown` SVI10 → `0 received`；`undo shutdown` → 恢复 `5/5` | E1/E3 |
| 静态路由 | `ip route-static` 写入、`display ip routing-table` 含 `192.168.2.0/24 / Static / 10.0.0.2`、`undo ip route-static` 撤销；双 Comware 路由器集成拓扑中：无路由 `0 received` → 双向路由 `5 received` → 撤销 `0 received` → 恢复 `5 received` | E3/E7 |
| `display current-configuration` | 原生输出只包含 `vlan <n>` / `port access vlan` / `port trunk permit vlan` / `interface Vlan-interface<n>` / `ip route-static`；不含 VRP 字面量；§0 补修后每条 `vlan <n>` 后补一行 ` quit`（E11 视图层级），保证**逐行重执行**后 `fs.mode == Config` 且确定性配置指纹（VLAN/SVI/静态路由/端口）与原状态完全一致 | E5/E11/ADR D12 |
| 查询纯读 | `display vlan` / `display ip routing-table` / `display interface brief` 从 `DevSummary` 实时生成；设备输出不含 `netemu` 措辞 | E1/E7 |
| OSPF 数据面 | 本包**不扩展**；只保证既有邻居/路由能力不回归，区域视图退出层级见 §6 | 主方案 §12 |

---

## 6. 实验二、课程表与 OSPF 退出层级

### 6.1 `office-l3` 中英文（与拆树同一变更集）

`web/src/lib/labs.ts` 与 `web/src/content/labs-en.ts` 同步改为**严格可顺序执行的箭头序列**（§0 补修后，前三步共 19 条）：

1. `system-view → vlan 10 → quit → vlan 20 → quit`（E1/E11：VLAN 视图必须 `quit` 才能再建 VLAN / 进接口）；
2. `interface g1/0/1 → port link-type access → port access vlan 10 → quit → interface g1/0/2 → port link-type access → port access vlan 20 → quit`（补齐两个物理接口之间的 `quit`）；
3. `interface Vlan-interface 10 → ip address 192.168.10.1 24 → quit → interface Vlan-interface 20 → ip address 192.168.20.1 24 → quit`（补齐两个 SVI 之间的 `quit`）。

第 4 步仍在属性面板填网关，第 5 步仍为 `display vlan` / `display current-configuration` 与 PC1→PC2 Ping。中文用 `→`、英文用 `->`，两种写法的解析结果**完全一致**。
Rust `include_str!` 门禁断言两语言均**不含** `G0/0/1` / `G0/0/2` / `vlan batch` / `Vlanif` / `port default vlan` / `allow-pass` / `Eth-Trunk`，且**含有**全部原生写法；实验二设备仍是 `add_device('Switch','Comware')`，检查项仍是 2 个 `sviIp` 纯读 + 1 个 `ping` 主动。新增断言：从两语言原文解析出的序列相等、序列中三处 `quit`（VLAN 视图 / 物理接口间 / SVI 间）存在、并逐条在真实引擎与真实 WASM `Sim` 上执行。

### 6.2 课程表分层矩阵

- `web/src/content/cmdmap.ts`：Comware 安全区域行改为 `security-zone name trust` / `import interface g1/0/1`；ACL 应用行改为 `packet-filter 3000 inbound`；VLAN/Trunk/SVI/静态路由行本就是 Comware 原生写法，未改。平台专属空列语义不变。
- `web/src/components/CourseView.svelte`：删除 `supportKey` / `markOf` / `stateClass` 三处「Comware 一律降级」；支持状态完全由结构化 `support` 决定。`i18n-dict.ts` 中英文提示同步改为分层说明。
- `stage3a_cmdmap_probe_rejected_rows`：`TOTAL_ROWS=439 REJECTED=0` 硬断言继续有效；新增 `stage4a_cmdmap_is_spec_driven_and_security_gate_remains`：`COMWARE_CELLS=146 REJECTED=0`，且同时存在 `full-support` 与非 `full-support` 的 Comware 单元格。
- 分层结果（真实浏览器，§0 补修后按表格**徽标前缀**精确统计，不再用 `innerText.includes`）：
  - VLAN 课程表 **7 行 × 3 厂商**：Comware 列 **7 行 `完整支持 / Executed / DocumentationOnly`**；**VRP 列 2 行完整支持 + 5 行待验证**（此前误记为「VRP 同 7 行待验证」，已修正）；IOS 列 7 行完整支持。
  - 安全课程表 **13 行**：Comware 列 **8 行待验证、0 行完整支持**（此前误记为 7 行，已修正），其余为「本平台无该字面对应命令」空列；`packet-filter` 为 `recognized-only / NotSimulated`。
  - 缓存 key 含 Profile + 模式 + 命令，VRP / Comware 不串用；`inspect_command('comware-generic-l3-switch','Vlan','quit')` 在补修后为 `full-support`（`vlan/quit` 已有规格行）。

### 6.3 OSPF 区域视图两级退出

真实 Comware 路由器 CLI：`ospf 7` → `area 0.0.0.1` → `network 10.1.0.0 0.0.0.3` → `quit` → `quit`。
提示符序列（浏览器实测）：`[CWOSPF] → [CWOSPF-ospf-7] → [CWOSPF-ospf-7-area-0.0.0.1] → [CWOSPF-ospf-7-area-0.0.0.1] → [CWOSPF-ospf-7] → [CWOSPF]`。
Rust 断言：进程号 `Some(7)` 不丢失、第一次 quit 后 `Mode::Ospf` 且 `sub.ospf == Some((7,0))`、第二次 quit 后 `Mode::Config`；区域视图内 `vlan 99` / `port access vlan 10` 返回 `InvalidCommand` 且状态指纹不变；`display current-configuration` 含 `ospf 7` / `area 0.0.0.1` / `network …`，OSPF 块含两次 `quit`（E6/E8）。未扩展完整 Comware OSPF 数据面。

---

## 7. 旧测试逐项调整说明（25 个含 `Dialect::Comware` 的既有文件）

全量 `cargo test --workspace` 在拆树后复跑，25 个既有文件**无一删除、无一注释、无一放宽断言**。逐文件处置：

> **§0 复核补修追加**：实现官方 VLAN 视图后，`vlan <id>` 会进入 VLAN 视图，6 处「`vlan <id>` 后直接跟接口 / 视图 / `undo vlan`」的既有执行序列会断。这些序列本身**不符合官方写法**（E1/E11 的 `port access vlan` 示例在 `vlan 3` 后显式 `quit`），因此按真实行为补 `quit`，**不是改测试绕过、也不是放宽断言**；所有状态 / 数据面 / 输出断言保持不变。

| 位置 | 补修 | 断言影响 |
|---|---|---|
| `stage4a_comware.rs::stage4a_vlan_access_svi_cross_vlan_routing_and_config_render_reexecution` | `vlan 10` / `vlan 20` 后各补 `quit` | 无；后续 `prompt == "[SW1]"`、pvid / VLAN / SVI / 路由断言均不变 |
| 同上（VLAN 创建 / 删除段） | `vlan 30` 后补 `quit` 再 `undo vlan 30` | 无；`undo vlan 1/99` 仍被拒 |
| 同上（渲染重执行） | `config_render` 输出每条 `vlan <n>` 后补 ` quit`，测试仍逐行 `trim()` 重执行 | 无；`fs.mode == Config`、配置指纹一致、`assert_eq!(cfg, render(...))` 不变 |
| `stage4a_comware.rs::stage4a_trunk_default_permit_semantics_and_positive_negative_traffic` | 两台交换机各在 `vlan 10` 后补 `quit` | 无；默认 `[1]`、累加 `[1,10,20]`、正/负 Ping 不变 |
| `stage4a_comware.rs::stage4a_profile_inspection_is_layered_pure_and_dual_channel` | `vlan 10` 后补 `quit` | 无；`display vlan` 仍 `Executed`，`packet-filter` 仍 `NotSimulated` |
| `vlan_golden.rs::comware_vlan_flow` | `vlan 10` 后补 `quit`，`display vlan` 输出索引 `outs[6]` → `outs[7]` | 无；`trunk_allowed == Some([1,10,20])` 与 `display vlan` 逐字快照不变 |
| `sim-wasm/src/stage4a_tests.rs` | 删除手写的 17 条常量，改为从 `labs.ts` / `labs-en.ts` 解析 19 条课程原文 | 新增断言：中英文序列相等、三条 `quit` 存在、逐条 `Executed`、VLAN 视图提示符 |

| # | 文件 | 处置 | 依据 / 说明 |
|---|---|---|---|
| 1 | `abbreviation_slice.rs` | 原生保留 | `display i` 歧义缩写仍返回 `InvalidCommand`，新 display 树仍有多个 `i*` 分支 |
| 2 | `bgp_golden.rs` | 原生保留 | `bgp 65001` / `peer … as-number` 属有证据的通用命令 |
| 3 | `dc_golden.rs` | 原生保留 | `stack member 2` / `display mlag` 通用 |
| 4 | `golden.rs` | 原生保留 | Comware `<H3C>`/`<SW1>` 提示符、`sysname` / `quit` / `return`、`display ip interface brief` 黄金；`svi_short` 修正未涉及该用例（无 SVI） |
| 5 | `ipsec_golden.rs` | 原生保留 | IPsec 隧道通用 |
| 6 | `ipv6_golden.rs` | 原生保留 | `ipv6 address` / `ipv6 route-static` 通用 |
| 7 | `isis_golden.rs` | 原生保留 | `isis 1` 通用 |
| 8 | `ltm_golden.rs` | 原生保留 | 项目教学抽象，双向保留，不作为保真证据 |
| 9 | `lacp_golden.rs` | 原生保留 | `interface Bridge-Aggregation 1` / `port link-aggregation group 1` / `undo port link-aggregation group` / `display link-aggregation summary` 全部保留在 Comware 树；`selected=[1,2]` 与 Ping 不变 |
| 10 | `mgmt_golden.rs` | 原生保留 | `COMWARE_CONFIG` 的 `class manage` / `password simple` / `authorization-attribute user-role` / `line vty` 保留；`password simple` 在 Aaa 视图节点未改 |
| 11 | `mpls_golden.rs` | 原生保留 | `static-lsp …` 通用 |
| 12 | `mstp_golden.rs` | 原生保留 | `stp mode mstp` / `instance 2 vlan 20` 通用 |
| 13 | `ospf_golden.rs` | 原生保留 | `ospf 1` / `Mode::OspfArea` 下 `network …` 双向可解析；本包另增 Comware 两级退出断言 |
| 14 | `route_policy_golden.rs` | 原生保留 | `route-policy` / `peer … route-policy` 通用 |
| 15 | `rstp_golden.rs` | 原生保留 | `stp mode rstp` 通用 |
| 16 | `security_golden.rs` | 原生保留 | `security-zone name` / `import interface` 保留在 Comware；`display security-zone` / `display security-policy` 不变；security 主题仍不标完整支持 |
| 17 | `security_qos_golden.rs` | 原生保留 | `dhcp snooping` / `port-security` 通用 |
| 18 | `stage2b_flows.rs` | 原生保留 | Comware 规格 lint 断言 `strict_ok()` 继续通过（新增 30 条规格且无悬空/漂移） |
| 19 | `stage2c_flows.rs` | 同上 | 同上 |
| 20 | `stp_golden.rs` | 原生保留 | `stp cost` / `display stp brief` 通用 |
| 21 | `vlan_golden.rs` | 原生保留 + §0 补 `quit` | `comware_vlan_flow` 在 `vlan 10` 后补官方必需的 `quit`（输出索引同步 6→7）；`port trunk permit vlan 10 20` → `Some([1,10,20])` 与 `display vlan` 逐字黄金完全不变（E1/E3/E11） |
| 22 | `vrrp_golden.rs` | 原生保留 | `vrrp vrid` 通用 |
| 23 | `vrf_golden.rs` | 原生保留 | `ip vpn-instance` 通用 |
| 24 | `vxlan_golden.rs` | 原生保留 | `vxlan peer` / `display vxlan` 通用 |
| 25 | `wlan_golden.rs` | 原生保留 | `wlan vlan 10` / `display wlan` 通用 |

**另有 3 处门禁断言本身要求「两套语法混用 / 全量降级」，改为逐方言接受-拒绝矩阵（覆盖保留或增强，不是删除）：**

| 位置 | 旧断言 | 新断言 |
|---|---|---|
| `tests/stage3a_cmdmap.rs::stage3a_cmdmap_uses_structured_platform_status_not_boolean` | 所有 Comware 行 `support != full-support` | ① VLAN 首批行必须 `full-support`；② 任何 Comware `full-support` 必须来自 `specs::find(Comware, specId)` 的 `Implemented + can_decide_capability` 规格；③ security 行仍不得 `full-support` 且不得 `invalid-command`；④ 同时统计 `comware_full` 与 `comware_not_full` 均 > 0。并新增 `REJECTED=0` 硬断言不变 |
| `tests/stage3b_courses.rs::stage3b_comware_security_rows_still_never_full_support` | 8 条（含 VRP 的 `firewall zone trust` / `add interface`）在 Comware 上不得是命令错误 | 拆成三层：`comware_native` 8 条不得完整支持且不得命令错误；`security-zone name` / `import interface` 在 VRP 上必须 `invalid-command`；`vrp_only` 2 条在 Comware 上必须 `invalid-command`。保留原有 security 负向门禁并新增双向拒绝覆盖 |
| `sim-wasm/src/stage3a_tests.rs::stage3a_profile_selection_changes_structured_support` | Comware `port access vlan 10` `!= full-support` | 改为 `== full-support`；另增 security `security-zone name trust` 不得完整支持/不得命令错误、`vlan batch 10 20` 在 Comware 上必须 `invalid-command`；IOS/VRP 原断言不变 |

新增 `tests/stage4a_comware.rs`（9 项）与 `sim-wasm/src/stage4a_tests.rs`（3 项），见 §5/§6/§8。

---

## 8. 真实 WASM / 浏览器证据（§0 补修后重跑）

方式：`playwright-cli` + `vite preview @ localhost:4173`，消费 **本次** `wasm-pack build` + `vite build` 生成的 `net_sim_bg-DU6RWIQS.wasm`（浏览器 `performance` 资源条目实测）。全部结论来自页内真实 WASM API / 真实 UI 点击，**未使用 TypeScript 假结果或声明桩**。临时脚本 `web/dist/stage4a-recheck.js` 在验收后删除；快照 yaml / `.playwright-cli/` 已清理。语言偏好会持久化，故每轮验收先清空 `localStorage`，并用 `ol.task li` 首行断言中文（`→`）或英文（`->`）界面，避免语言漂移造成假绿。最终六项门禁复跑只改了两处源码注释，WASM 产物名与字节大小不变（同为 `net_sim_bg-DU6RWIQS.wasm`，1,594.70 kB），因此下列浏览器结论对最终交付产物有效。

| 验收点 | 真实结果 |
|---|---|
| **按课程原文逐条执行（中文）** | 从 DOM `ol.task li` 解析前三步得到 **19 条**命令，逐条送进真实 WASM `exec_command_result`：`failures=0`；提示符出现 `[SW1-vlan10]` / `[SW1-vlan20]`，末态 `[SW1]`；`display current-configuration` 输出无 `vlan batch` / `port default vlan` / `allow-pass` / `Vlanif` |
| **按课程原文逐条执行（英文）** | 切换界面语言后重新加载 `office-l3`，从 DOM 解析英文任务得到 **19 条**、`failures=0`，同样出现两个 VLAN 视图提示符；`sequenceEqual=true`（中英文序列完全一致） |
| 纯读 2 项 + 状态/时间/统计不变 | UI「纯读检查」→ 2 项 `good` + 1 项 `skip`；`dump()` / `stats()` 前后完全一致；`labPassed['office-l3']=false` |
| 主动判分 3/3 + 完成门禁 | UI「运行实验判分」→ 3 项全 `✓`（含 `PC1 → PC2 跨 VLAN 连通 / 主动实验 / ping 连通`），`good=3`，`labPassed=true`；WASM 契约测试另断言真实 Ping `5 packets transmitted, 5 received` |
| 修改后重判分不缓存 | 改 SVI10 地址（`display current-configuration` 实际反映 `192.168.10.9`）→ `good=2`；恢复 → `good=3` |
| VLAN 课程表分层（精确徽标统计） | 表格 **7 行 × 4 列**：`vrpFull=2 vrpPending=5 cwFull=7 cwPending=0`，即 **VRP 2 行完整支持 + 5 行待验证**、**Comware 7 行完整支持**（§0 计数修正）；缓存 key 含 Profile + 模式 + 命令，VRP 用 `vlan batch 10` 另占 key，不串用 |
| security 负向门禁 | 安全课程表 **13 行**：Comware 列 `cwFull=0 cwPending=8`，即 **8 行待验证、0 行完整支持**（§0 计数修正）；跨方言 `inspect_command`：`port access vlan 10` Comware `full-support` / VRP `invalid-command`，`port trunk permit vlan` 同理，`vlan batch` Comware `invalid-command` / VRP `pending-verification`，`packet-filter` `recognized-only` |
| VLAN 视图规格 | `inspect_command('comware-generic-l3-switch','Vlan','quit')` = `full-support`，证明 VLAN 视图退出已关联规格、不是迁移清单里的空壳 |
| 一次会话级纯读 inspection | `session_id(SW1)` → `verify_device_command(..., "port access vlan 10")` 返回 `full-support / Executed / DocumentationOnly`，`detail` 含 `[profile=comware-generic-l3-switch mode=if authenticated=true remote=false]`；错误令牌 `session id mismatch`；前后 `dump()` / `stats()` 不变 |
| OSPF 两次 quit | 真实提示符 `[CWOSPF-ospf-7-area-0.0.0.1] → [CWOSPF-ospf-7] → [CWOSPF]`；区域视图错误模式 `vlan 99` → `InvalidCommand / % Unrecognized command found at '^' position.` 且 `stateChanged=false`；配置视图 OSPF 块含两次 `quit` |
| 双通道 | `display vlan`：`deviceOutputLen=340`、`simulatorNotices=[]`；`packet-filter 3000 inbound`：`NotSimulated`、`deviceOutput=""`、1 条 `[netemu]` 提示；旧文本 API 中 `[netemu]` 恰好出现 1 次；`TerminalPanel.svelte` 静态门禁断言 `deviceOutput` 与 `simulatorNotices` 各渲染一次 |
| 控制台 | 全流程新增错误 **0**（`consoleErrors=0`，无 pageerror） |

**截图**（`docs/cli-fidelity/`，§0 补修后重拍）：

| 文件 | 内容 |
|---|---|
| `stage4a-office-l3-active.png` | 实验二纯读/主动两个按钮 + 3/3 全绿判分结果 + 拓扑（课程序列执行后） |
| `stage4a-vlan-course.png` | VLAN 课程表：Comware 列 7 行「完整支持」，VRP 列 **2 行「完整支持」+ 5 行「待验证」**，逐格结构化 evidence/specId |
| `stage4a-security-course.png` | 安全课程表（宽视口）：Comware `security-zone name trust` / `import interface g1/0/1` / `security-policy` 等 **8 行**仍为「待验证」，平台专属空列照常 |

**覆盖边界（如实说明）**：截图为单页流程证据，课程表与判分结果需滚动内部容器 `.reader` / `.left-panel` 才完整可见；只在中文界面抓屏，但**英文任务序列已在真实浏览器 DOM 上解析并逐条执行**（不只依赖 Rust `include_str!`）；未逐课程、逐终端窗口或逐语言抓屏。实质验证由页内真实 WASM API 结构化结果完成；终端 xterm 的逐行 DOM 渲染未逐帧抓屏，改由 `TerminalPanel.svelte` 单一渲染静态门禁 + 浏览器结构化双通道结果共同约束。

---

## 9. 六项门禁（最终真实结果）

| 门禁 | 退出码 | 结果 |
|---|---|---|
| `cargo fmt --all -- --check` | **0** | 先 `cargo fmt --all` 统一新增文件，复跑 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | **0** | 首轮修掉两处 `field_reassign_with_default`（新测试 Session 构造），§0 补修后复跑仍 0；无 `#[allow]` 规避 |
| `cargo test --workspace` | **0** | **291 passed / 0 failed / 0 ignored，48 个测试二进制**（基线 276 + 阶段 4A 首轮 12 + §0 补修 3：课程原文顺序执行、VLAN 视图层级、WASM 课程原文契约）；课程表 `TOTAL_ROWS=439 REJECTED=0`，`COMWARE_CELLS=146 REJECTED=0` |
| `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | **0** | 见 9.1（§0 补修后重建） |
| `npx --yes pnpm@10 --dir web run check` | **0** | `tsc --noEmit` 0 错误（补修后复跑） |
| `npx --yes pnpm@10 --dir web run build` | **0** | 消费 `net_sim_bg-DU6RWIQS.wasm`（1,594.70 kB，gzip 554.63 kB）；最终门禁 `✓ built in 4.59s` |

### 9.1 WASM 总门禁逐段

| 段 | 结果 |
|---|---|
| Rust → WASM 编译 | ✅ `release` profile，基于 §0 补修后的当前源码（最终门禁 12.14s） |
| wasm-bindgen 声明 | ✅ `net_sim.js` / `net_sim.d.ts` / `net_sim_bg.wasm.d.ts` / `package.json` 重新生成 |
| wasm-opt | ✅ `[INFO]: Optimizing wasm binaries with wasm-opt`，本机**未复现** `拒绝访问 (os error 5)`；**未关闭优化、未改配置换绿** |
| WASM 总门禁 | ✅ `:-) Done in 18.49s`（最终门禁），退出 0 |
| Web 结果 | ✅ `check` / `build` 均 0；浏览器 reload 后实测资源名为 `net_sim_bg-DU6RWIQS.wasm`，控制台 0 消息 |

| 层 | 数量 / 结果 |
|---|---|
| Rust 单元 + 集成 + doctest | 291 passed / 0 failed（阶段 4A 共新增 15） |
| 浏览器真实 CLI | **课程原文中文 19 条 + 英文 19 条**逐条执行（不再是手写 17 条），加 OSPF / 双通道 / 会话命令，`failures=0`；两级 VLAN 视图提示符实测出现 |
| 浏览器真实检查 | 纯读 2 通过 + 1 跳过且状态/时间/统计不变；主动 3/3；改 SVI 后重判分 `2→3` 反映变化；Access VLAN 改/恢复另由 Rust / WASM 契约测试覆盖 |
| 课程表 | 439 非空单元格、146 个非空 Comware 单元格，`REJECTED=0` 且为断言；浏览器按徽标精确统计 VRP `2+5`、安全课程 Comware `8 待验证` |

---

## 10. HEAD、分支、真实工作区计数、未完成项与阻塞

| 项 | 状态 |
|---|---|
| HEAD | `3d784e022c8e70ca3c4aafbce65edd1220064ed7`（未变） |
| 分支 | **detached HEAD（no branch）**，`git branch --show-current` 为空 |
| 提交 / 推送 | **未提交、未推送** |
| 工作区 | **29 个已跟踪 M + 19 组未跟踪**（起点 27 M + 16 组；本包新增 `ifname.rs` / `vlan_golden.rs` 2 个 M，`comware.rs` / `stage4a_comware.rs` / `stage4a_tests.rs` 3 组未跟踪，以及 `docs/cli-fidelity/` 内本记录与 3 张截图）。起点改动全部保留；`vlan_golden.rs` 是 §0 补修按官方 VLAN 视图补 `quit` 的既有测试。 |
| 构建目录 | 独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-4a-target` |
| 临时产物 | `web/dist/stage4a-verify.js`、`web/dist/stage4a-recheck.js` 均已删除；`stage4a-*.yaml` 快照与 `.playwright-cli/` 已清理；preview 服务与浏览器已关闭 |

**未完成项（明确后置，未开始）**

| 项 | 边界 |
|---|---|
| Comware 非首批族群（BGP / IS-IS / STP / VRRP / VXLAN / EVPN / LTM / route-policy / IPsec / DHCP / ACL / NAT / OSPFv3 / SRv6 / mgmt 等） | 仍是「迁移期共享语法」：命令可解析但无 Comware 官方规格行，课程表按 `待验证 / 仅识别` 显示，绝不 `full-support`；`config_render` 这些段落的逐厂商修正未做 |
| `packet-filter` 数据面 | 官方语法已登记，语义 `NotSimulated`，仅识别、不改状态 |
| Comware 路由器物理口默认管理状态 | 本包取得的官方正文无直接结论，保守保留 `false` 并标注待验证；未机械继承 VRP 的 `true` |
| 厂商逐字输出黄金 / 真机或 HCL 样本 | 仍缺，所有 `display` 只做结构/状态断言 |
| `undo interface Vlan-interface` / `port trunk pvid` / `vlan <a> to <b>` 等 Comware 官方变体 | 未纳入首批 |
| **VLAN 视图内的 `port <interface-list>` / `description` 成员分配**（§0 补修后新增的显式边界） | 视图层级与 `display` / `name` 已实现，`port` 成员分配、`description` 落库未实现；这些节点按 `unassociated` 显式列入迁移清单，不标 `full-support` |
| **VRP / IOS 的 VLAN 视图**（§0 补修后新增的显式边界） | 本包只给最小退出集合兜底，`vlan <id>` 仍不切换视图；不承诺 VRP 保真，不进入「VRP 拆分」范围 |
| C6b、F5 TMOS、完整防火墙 / ASA / VRP 拆分、完整 IOS ZBF、`run()` 前端事件动画 | 本包明确不做 |

**阻塞**：无功能阻塞。环境事项：共享 target 指纹权限冲突（独立目录规避）；交接文档提到的 `wasm-opt os error 5` 本机未复现；preview 服务绑定 IPv6 `localhost`，故用 `http://localhost:4173/`。

---

## 11. 阶段 4 是否完成的明确结论

**结论：§0 复核发现的阻塞问题已按官方证据补修并通过全量复核；主方案 §15「阶段 4：Comware」在本交接包范围内完成并验收通过，可以关闭。**

依据：
1. `trees(Dialect::Comware)` 已改调独立 `comware::build()`，两棵树双向隔离了 §4 矩阵列出的全部专属命令；共享的只有 handler / 参数工具 / 规格机制 / `sim-core` 状态，`CNode` 仍是唯一语法来源，未建第二套后端。
2. VLAN / Access / Trunk / SVI / 静态路由 / `display current-configuration` 六组原生闭环均有官方证据（E1–E11）、规格行、状态断言、错误模式/非法参数断言与正/负数据面验收；配置渲染重执行可还原同一状态。
3. 实验二 Comware 化**与拆树在同一变更集完成**，中英文同步，真实 WASM 纯读 2 项不变、主动 3/3、完成门禁与重判分均实测通过。
4. 课程表改为规格驱动的分层矩阵：首批闭环行显示完整支持，未迁移 / 未模拟 / security 行仍不得完整支持；`REJECTED=0` 硬断言与双语言门禁均通过。
5. OSPF 进程 → 区域两级 `quit`、提示符、进程号、配置渲染与错误模式状态不变全部有浏览器与 Rust 断言；未顺带扩展数据面。
6. 六项门禁全绿（291 Rust 测试 / 48 二进制、WASM 逐段全绿、Web check/build 全绿），真实浏览器消费本次 WASM，控制台 0 新增错误，三张截图留存。
7. **§0 补修的阻塞已消除**：E11 官方正文证明 `vlan <id>` 必须进入 VLAN 视图、`quit` 退回上一级；已实现 `Mode::Vlan` / `[SW1-vlan<n>]` / `quit`→系统视图 / `return`→用户视图，课程第二、三步补齐接口与 SVI 之间的 `quit`。**中英文课程原文各 19 条命令在真实 WASM / 真实浏览器 DOM 上逐条执行 `0` 失败**，不再依赖另写的 17 条验收清单；Rust 集成测试、WASM 契约测试、浏览器测试三者共用同一套原文解析规则。计数偏差（VRP `2 完整支持 + 5 待验证`、安全课程 Comware `8 行待验证`）已按表格徽标精确统计并在 §6.2 / §8 更正。

**收口边界**：阶段 4 完成**不等于 Comware 全功能完成**。§10 列出的非首批族群、逐字输出黄金、真机校准与 `packet-filter` 数据面仍属后续工作；本包未进入 F5 TMOS、C6b、完整防火墙 / ASA / VRP 拆分、完整 IOS ZBF 或 `run()` 前端动画。本包在上表范围内停止。
