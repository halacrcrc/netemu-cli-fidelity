# 阶段 2C · H4 VRP BGP + H5 VRP ACL/Route-Policy/BGP 引入策略 + 阶段 2 全量收口 · 实施记录

> 执行日期：2026-11-12
> 基线：`3d784e022c8e70ca3c4aafbce65edd1220064ed7`（detached HEAD，未提交、未推送）
> 上一记录：`docs/cli-fidelity/stage2b-implementation-record.md`（保留，不回写）
> 本包范围：**H4（VRP BGP）→ H5（VRP ACL / Route-Policy / BGP 引入策略）→ 阶段 2 全量收口回归**
> 停止门禁：阶段 2 总回归完成后停止；**未进入 C6b / 阶段 3 / Comware / F5 / 防火墙扩展**。

---

## 1. 现场保护与基线复跑

| 项 | 结果 |
|---|---|
| `git symbolic-ref -q HEAD` | 失败 → **detached HEAD** |
| `git rev-parse HEAD` | `3d784e022c8e70ca3c4aafbce65edd1220064ed7`（与交接文档固定基线一致） |
| 未提交改动 | 起点 30 项（18 M + 12 组未跟踪），全程未 `reset`/`clean`/覆盖/删测/放宽断言 |
| 构建目录 | 共享 target 在本环境复现指纹文件权限冲突 → 全程使用独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-2c-target`（环境问题，非断言失败） |
| `cargo test --workspace`（基线复跑） | Stage 2B 基线 **44 binaries / 233 passed / 0 failed** 吻合，无本次回归 |

复用既有实现，未另建平行内核：`tests/bgp_golden.rs`、`tests/route_policy_golden.rs`、
`sim-core/tests/scenarios.rs` 的 eBGP 学习/ping、现有 BGP/Route-Policy 命令树与 handler 全部保留并继续运行。
本包在其上补齐 H4/H5 的 strict 规格、否定形式、故障恢复与正负 L4 验收。

---

## 2. H4 / H5 支持矩阵

图例：**完成** = 语义/状态/数据面闭环且测试全绿；**待验证** = 语义已实现但缺带版本 VRP/eNSP 样本，
不写逐字输出黄金；**未实现** = 明确不在本包。

| 流程 | 支持状态 | strict 规格 | 否定形式 | 正/负 L4 | 代表测试 |
|---|---|---|---|---|---|
| **H4** VRP BGP | 完成（输出待验证） | 12 | `undo bgp <as>`、`undo peer <ip>`、`undo network <ip> <mask>`（mask 关键字 + 点分掩码两形态） | 4（P1/N1/N2/R1） | `h4_*`（stage2c_flows.rs） |
| **H5** VRP ACL/Route-Policy/BGP 策略 | 完成（输出待验证） | 24（ACL 10 + Route-Policy 12 + BGP 绑定 2） | `undo acl <n>`、`undo route-policy <name> [node <seq>]`、`undo if-match [acl]`、`undo apply [local-preference]`、`undo peer <ip> route-policy <name> <dir>` | 5（P1/N1/N2/R1/A1） | `h5_*`（stage2c_flows.rs） |

H4/H5 合计新增 **36 行 strict 规格**（`specs.rs` 2660–3095），全部 `PENDING_OUTPUT`、
references `&[VRP_CFG, ADR_D12]`；ID/shape 由 `cli-engine::spec::derive_commands` 从 `CNode` 派生，不复制语法树。

### 2.1 H4 证据 / 配置 / 查询 / 否定 / L4

| 维度 | 落实 |
|---|---|
| 能力/证据 | `StaticRouting`/`RouteTableView`/`ConfigView` Supported（DocumentationOnly）；证据不足 → `Executed / 待验证`，不伪判 `UnsupportedByDevice` |
| 会话/路由 | eBGP 双向 Established；跨 AS 路由进真实 RIB（`display ip routing-table` 与 `display bgp routing-table` 同源） |
| 配置视图 | `bgp <真实本端 AS>`、`peer <ip> as-number <真实对端 AS>`、`network <真实前缀> <掩码>`；用 AS 65010/65020 + 额外前缀 10.9.0.0 证明无硬编码 |
| 查询 | `display bgp peer`（邻居/对端 AS/Established）、`display bgp routing-table`（前缀/AS_PATH/下一跳/LocPrf）纯读 |
| 否定 | `undo network` 经 Update 全表刷新撤销对端前缀；`undo peer` 删邻居+策略绑定；`undo bgp` 校验 AS 一致后清进程；重复撤销/不匹配掩码/错误 AS 拒绝且 `state_fingerprint` 不变 |
| 故障恢复 | 接口 `shutdown` → hold-timer 老化撤会话/路由；`undo shutdown` → 重建（H4-R1，真实计时，非伪造 hook） |
| L4 | H4-P1 双向跨 AS ping 5/5；H4-N1 错误 AS/无邻居/未宣告 → 不建邻或无路由 + 0/5；H4-N2 撤销后路由消失 + 0/5；H4-R1 故障撤销 + 恢复 5/5 |

### 2.2 H5 证据 / 配置 / 查询 / 否定 / L4

| 维度 | 落实 |
|---|---|
| ACL 语义 | 复用 C5 首条命中 + 隐式拒绝；VRP ACL 通配符参与路由前缀匹配（`match_acl`：ACL permit=命中，缺 ACL=false） |
| 节点顺序 | seq 升序首命中（`nodes.sort_by_key(seq)`），非 HashMap/排序偶然性；permit/deny、`if-match acl`、`apply local-preference` |
| 绑定 | `peer <ip> route-policy <name> import/export`：允许前缀存在、被拒前缀不存在、未命中按隐式拒绝默认 |
| 修改对照 | 无策略→deny→permit 的 RIB + ping 前后对照，证明过滤来自策略而非静态拓扑 |
| 配置视图 | 真实 ACL 规则、route-policy 节点（seq 升序、if-match/apply）、BGP 引用；无占位行 |
| 选路 | `apply local-preference` 验证**实际选路**：lp200 使下一跳指向 R3，改绑 R1 后下一跳翻转，`undo apply` 回 lp100（H5-A1，非仅渲染） |
| 否定 | `undo route-policy node`/`undo route-policy`/解绑/`undo acl`/`undo if-match`/`undo apply` → 路由重算 + 业务恢复；非法 seq(0/70000)、不存在 ACL/策略、错误模式、重复定义拒绝且状态不变、不静默覆盖整策略 |
| L4 | H5-P1 允许前缀路由+ping 5/5；H5-N1 拒绝前缀无路由+0/5；H5-N2 仅 permit 节点无兜底→未命中隐式拒绝+负向断言；H5-R1 撤节点/解绑→重算+恢复 5/5；H5-A1 Local-Preference 选路对照 |

**L4 场景登记表**：`stage2c_flows.rs::STAGE2C_L4` 共 **9 条**（H4 四条 + H5 五条），每条映射到具体执行断言；
`stage2c_l4_registry_is_explicit_and_nonempty` 校验 9 个最低 ID 齐全、正负兼备、ID 不重复。

---

## 3. 阶段 2 canonical 总矩阵（15 条流程）

编号更正沿用 Stage 2B：**C3 = IOS DHCP**、**C6a = 管理认证**（不再沿用 Stage 2A 误标）。
strict 数为当前 `specs.rs` 逐段实测；IOS 合计 104、VRP 合计 136，与 §4 lint 一致。

| 流程 | 名称 | strict | 否定形式 | L4/管理面反例 | 代表测试 |
|---|---|---|---|---|---|
| C1 | IOS 编址/静态路由/CLI | 21 | `no ip route`、`no ip address` | 4（正1负3） | `prototype.rs`、`l4_scenarios.rs` |
| C2 | IOS VLAN/Access/Trunk/SVI | 8 | `no switchport access vlan` | 2（正1负1） | `c2_h1_vlan_slice.rs` |
| C3 | IOS DHCP（+VRP ip pool） | 26 IOS + 8 VRP | `no ip dhcp pool`、`no default-router`、`no network`、`no dns-server` | 3（正1负2） | `stage2b_flows.rs c3_dhcp_*` |
| C4 | IOS NAT | 13 | `no ip nat inside/outside`、`no ip nat inside source static`、`no ip nat inside source list <acl> interface <if> [overload]` | 2（正1负1） | `c4_nat_*` |
| C5 | IOS ACL | 15 | `no access-list <n> [rule <i>]`、`no ip access-group` | 3（TCP正+2负） | `c5_acl_*` |
| C6a | IOS 管理认证 | 4 | `no login`、`no enable secret`、`no username` | 管理面负向（错误口令不提权+指纹不变；非数据面 L4） | `c6a_*`、`c3_mgmt_slice.rs` |
| C7 | IOS OSPF | 10 | `no router ospf <pid>`、`no network <ip> <wc> area <a>` | 2（正1负1） | `c7_ospf_*` |
| C8 | IOS VRRP | 7 | `no vrrp <vrid>`、`no vrrp <vrid> preempt` | 2（正1负1） | `c8_vrrp_*` |
| H0 | VRP 编址/静态路由/CLI | 25 | `undo ip route-static`、`undo ip address` | 2（正1负1） | `prototype.rs` |
| H1 | VRP VLAN/Trunk/SVI | 11 | `undo port default vlan` | 2（正1负1） | `c2_h1_vlan_slice.rs` |
| H2 | VRP STP | 13 | `undo stp priority`、`undo stp root`（config+if）、`stp disable` | 2（正1负1） | `h2_stp_*` |
| H3 | VRP Eth-Trunk/LACP | 12 | `undo eth-trunk`（if+config） | 2（正1负1） | `h3_eth_trunk_*` |
| H4 | VRP BGP | 12 | `undo bgp`、`undo peer`、`undo network` | 4（P1/N1/N2/R1） | `stage2c_flows.rs h4_*` |
| H5 | VRP ACL/Route-Policy/BGP 策略 | 24 | `undo acl`、`undo route-policy [node]`、`undo if-match [acl]`、`undo apply [local-preference]`、`undo peer route-policy` | 5（P1/N1/N2/R1/A1） | `stage2c_flows.rs h5_*` |
| H6 | VRP OSPF | 31 | `undo network`、`undo area`、`undo ospf <pid>` | 2（正1负1） | `h6_vrp_ospf_*` |

校验：IOS 21+8+26+13+15+4+10+7 = **104**；VRP 25+11+8(C3)+13+12+12+24+31 = **136**。
L4 登记合计：Stage 2A `l4_scenarios.rs` 10 + Stage 2B `STAGE2B_L4` 18 + Stage 2C `STAGE2C_L4` 9。

---

## 4. 最新 lint 与迁移统计（本次新鲜提取）

由 `cli_engine::spec::lint_report` 从当前代码现场导出：

| 方言 | 派生总数 | 已关联 | 显式未关联 | strict 总数 | strict 覆盖 | 重复 | 悬空 | 漂移 | strict 缺失 |
|---|---|---|---|---|---|---|---|---|---|
| IOS | 287 | 104 | 183 | 104 | 104 | 0 | 0 | 0 | 0 |
| VRP | 480 | 136 | 344 | 136 | 136 | 0 | 0 | 0 | 0 |
| Comware | 480 | 0 | 480 | 0 | 0 | 0 | 0 | 0 | 0 |

`已关联 + 显式未关联 = 派生总数` 三方言均成立（104+183=287、136+344=480、0+480=480）。
相较 Stage 2B（VRP 464/100/364/100）：派生 +16（H4/H5 新增可执行 undo 节点）、已关联 +36（H4/H5 规格行）、
显式未关联 −20（原有 BGP/ACL/policy 节点本次关联）。
`stage2c_lint_has_no_duplicates_dangling_drift_or_strict_gaps` 与
`stage2b_lint_*`、`strict_ids_are_derived_from_the_side_table_not_duplicated` 多重把关；
20 个 H4/H5 needle 全部 strict + 有契约 + 有证据引用。显式未关联清单作为迁移清单保留，**不设隐藏豁免**。

横切约束保持：`CNode` 仍是语法/模式/帮助/补全唯一输入源，旁表只存派生 ID/shape/元数据；
平台差异走 Profile/PlatformPolicy 或独立方言树，**未新增共享 handler 的 `if dialect` 隐式语义分支**；
四态、双通道、结构化 API（`exec_command_result`）、旧文本 API（`exec_command`/`exec_and_run`）、WASM 契约兼容；
`Unknown` 不伪判 `UnsupportedByDevice`，待验证是证据元数据而非第五态；inspection 纯读（不推进时间/不探测/不调 `run_checks`）。

---

## 5. 最终六项门禁真实结果

| 门禁 | 真实退出码 | 说明 |
|---|---|---|
| `cargo fmt --all -- --check` | **0** | 起点 huawei.rs undo 子树有格式差，`cargo fmt --all` 修复后复跑 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | **0** | 首轮报 `single_element_loop`（stage2c_flows.rs:1223），**直接改为单条直调修复，未加任何 `#[allow]`**；复跑 0 |
| `cargo test --workspace` | **0** | **45 binaries / 250 passed / 0 failed / 0 ignored**（基线 44/233，净增 1 binary/17 tests = stage2c_flows 全量）；含 BGP/Route-Policy/OSPF/DHCP/ACL/NAT/VRRP/STP/LACP 与 Stage 2A/2B 全部既有测试 |
| `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | **0** | 见 §5.1 逐段 |
| `npx --yes pnpm@10 --dir web run check` | **0** | `tsc --noEmit` |
| `npx --yes pnpm@10 --dir web run build` | **0** | Vite 消费重新生成的真实 `net_sim_bg-BITG-Rhw.wasm`（1,493.83 kB，gzip 525.30 kB）；chunk>500kB 为既有 advisory，非错误 |

### 5.1 WASM 总门禁逐段

- **Rust→WASM 编译**：✅ release profile，基于当前源码（21.08s）。
- **wasm-bindgen 声明**：✅ 生成 `net_sim.js`/`net_sim.d.ts`/`net_sim_bg.wasm.d.ts`/`package.json`。
- **wasm-opt**：✅ 本环境使用缓存 binaryen v130，**未复现交接文档的 `拒绝访问 (os error 5)`**；未关闭优化、未反复重试、未改配置换绿。
- **WASM 总门禁**：✅ 退出 0；产物 `net_sim_bg.wasm` 1,493,831 B（优化后）。
- **Web check / build**：✅ / ✅，消费上述真实 WASM（非声明桩）。

---

## 6. 浏览器真实验证（真实 WASM，`http://localhost:4173/` preview）

通过页内 `window.__netemu.sim`（真实 WASM）驱动，结构化 `exec_command_result` 与旧文本 `exec_command` 双 API 均验证。

**H4 代表命令与状态**
- 拓扑 PC1—R1—R2—PC2（VRP），接口编址 + eBGP 65001/65002 全部 `Executed`。
- `display bgp peer`：`Local AS 65001`、`Peer 10.0.0.2 AS 65002 State Established`（真实会话），并带双通道提示。
- `display ip routing-table`：`192.168.2.0/24 BGP Pre 20 NextHop 10.0.0.2`（跨 AS 学习进真实 RIB）。
- `display bgp routing-table`：`192.168.2.0/24 NextHop 10.0.0.2 LocPrf 100 AS_PATH 65002`。
- `ping 192.168.2.10`（PC1）：**5 transmitted, 5 received, 0% loss**（H4-P1 浏览器复现）。

**H5 代表命令与状态**
- R2 绑定 `acl 2000` + `route-policy FILTER deny node 10 / if-match acl 2000` + `permit node 20` + `peer 10.0.0.1 route-policy FILTER import` 全 `Executed`。
- 绑定后 R2 RIB **失去 192.168.1.0/24**，`ping 192.168.1.10`（PC2）**0 received, 100% loss**（H5-N1）。
- `display route-policy`（node 10 deny/if-match acl 2000、node 20 permit）、`display acl all`（rule 5 permit source 192.168.1.0 0.0.0.255）、`display current-configuration`（含 route-policy/if-match/peer 引用）均取真实状态。
- `undo peer 10.0.0.1 route-policy FILTER import` → RIB 恢复 192.168.1.0/24 + `ping` **5/5**（H5-R1）。

**双通道 / 旧文本 API / 控制台**
- 结构化 `simulatorNotices` 含 `[netemu] 平台能力或输出证据不足：行为待验证…`，`deviceOutput` 无模拟器措辞。
- 旧文本 `exec_command` 同时含设备输出（`Established`）与 `[netemu]` 提示行，不丢设备输出。
- 控制台 **0 messages（0 errors / 0 warnings）**。

**Stage 2B 收口否定形式浏览器补验**
- NAT EasyIP `no ip nat inside source list 100 interface GigabitEthernet0/1 overload`：`Executed`，`nat_easyip` 置 null、配置移除；不匹配接口（g0/0）→ `InvalidCommand "% NAT EasyIP rule not found"` 状态不变。
- IOS OSPF `no network 10.0.0.0 0.0.0.255 area 3`：`Executed`，running-config 移除该语句。
- VRP `undo stp root`：`Executed`（`stp root primary` 后撤销），`display stp brief` 正常 + 双通道提示。

**视觉抓屏**：视口可用，已抓 `docs/cli-fidelity/stage2c-browser.png`（加载态应用 + 实验七 BGP route-policy 拓扑的真实设备图标/连线 + 引擎统计 设备6/链路5）。
这是**单张冒烟抓屏，不宣称完整视觉回归**；命令/路由/策略/双通道/旧 API 的实质验证经页内真实 WASM API 完成（见上），抓屏未逐一打开每个终端窗口。

---

## 7. 实际代码与测试清单

**新增**
- `crates/cli-dialects/tests/stage2c_flows.rs`：**17 条测试**（9 条 L4 登记 + H4 六条 + H5 七条 + lint/能力/双通道横切），全绿。
- `docs/cli-fidelity/stage2c-implementation-record.md`（本文件）、`docs/cli-fidelity/stage2c-browser.png`。

**修改（本包 H4/H5 生产代码，复用既有内核）**
- `crates/sim-core/src/engine.rs`：BGP Update 全表刷新撤销（`on_bgp_packet`）、`bgp_reset`；hold-timer 会话老化支撑链路故障撤销。
- `crates/cli-dialects/src/lib.rs`：`route_policy_match_acl`/`bgp_peer_policy` 完整预检；`bgp_no_process`/`bgp_no_neighbor`/`bgp_no_network`/`route_policy_no`/`bgp_no_peer_policy`/`route_policy_no_match`/`route_policy_no_apply`/`acl_no_all` 的 `bgp_policy_changed` 重算钩子。
- `crates/cli-dialects/src/huawei.rs`：config/bgp/route-policy 三视图 `undo` 子树（CNode 唯一语法源）。
- `crates/cli-dialects/src/config_render.rs`：BGP + route-policy 真实状态渲染（seq 升序、真实 AS/邻居/前缀/引用，去占位）。
- `crates/cli-dialects/src/specs.rs`：H4/H5 共 36 行 strict 规格。

**本会话直接改动**
- `stage2c_flows.rs`：修复 clippy `single_element_loop`（单元素 for 改直调）。
- `huawei.rs`：`cargo fmt` 格式化 undo 子树。
- 临时 lint 统计测试 `zz_lint_stats.rs` 用完即删，**无残留**。

**测试映射（stage2c_flows.rs 17 条）**
`stage2c_l4_registry_is_explicit_and_nonempty`、`h4_ebgp_cross_as_adjacency_route_and_ping`(P1)、
`h4_wrong_as_no_neighbor_unannounced_are_unreachable`(N1)、`h4_undo_network_peer_bgp_withdraw_and_state_unchanged`(N2)、
`h4_link_failure_withdraw_and_recovery`(R1)、`h4_config_render_uses_real_as_neighbor_and_prefix`、
`h4_h5_syntax_help_completion_abbreviation_from_cnode`、`h5_policy_permit_prefix_route_and_ping`(P1)、
`h5_policy_deny_prefix_no_route_ping_fail`(N1)、`h5_implicit_deny_unmatched_default`(N2)、
`h5_undo_node_unbind_recompute_and_recovery`(R1)、`h5_local_preference_changes_real_path_selection`(A1)、
`h5_failure_paths_are_state_unchanged`、`h5_policy_modification_before_after_comparison`、
`stage2c_lint_has_no_duplicates_dangling_drift_or_strict_gaps`、
`stage2c_capability_precedes_semantics_and_unknown_is_not_unsupported`、
`stage2c_dual_channel_and_inspection_is_pure_read`。

---

## 8. HEAD、分支、工作区、未完成项与阻塞

| 项 | 状态 |
|---|---|
| HEAD | `3d784e022c8e70ca3c4aafbce65edd1220064ed7` |
| 分支 | **detached HEAD（no branch）** |
| 提交/推送 | **未提交、未推送**（按要求） |
| 工作区 | 18 个已跟踪 M + 12 组未跟踪（specs.rs、8 个测试文件、cli-engine/src/spec.rs、sim-core/src/profile.rs、docs/cli-fidelity/）；无临时诊断残留 |
| 构建目录 | 独立 `CARGO_TARGET_DIR`（共享 target 权限冲突，环境问题） |

**未完成项（明确后置，未开始）**
- C6b 跨设备 SSH/远程 VTY 认证；Comware/F5/防火墙扩展；全量课程 UI 对齐；`run()` 前端事件动画；完整 AAA。
- 全部流程的**厂商逐字输出黄金**：无带版本 VRP/eNSP 样本，保持「输出待验证」；`simulator_notices` 显式给出证据不足提示。
  **区分**：阶段 2 功能流程切片已完成（语义/状态/数据面/正负 L4 全绿）≠ 厂商逐字输出全部验证完成（后者仍待验证）。
- IOS 183 / VRP 344 / Comware 480 显式未关联节点，作为迁移清单保留，不是流程缺口。

**阻塞**：无功能阻塞。环境事项：共享 target 权限冲突（已用独立目录规避）；本地 preview 服务需后台常驻。
`wasm-opt` 本环境成功，交接文档的 os error 5 未复现。

---

## 9. 停止点

**阶段 2 总回归完成后停止**：H4/H5 闭环、15 条 canonical 流程矩阵齐备、45 binaries / 250 Rust 测试全绿、
六项最终门禁全绿、真实 WASM（含 wasm-opt）与浏览器冒烟（含抓屏）完成、实施记录已交付。
**未进入 C6b / 阶段 3 / Comware / F5 / 防火墙扩展。**