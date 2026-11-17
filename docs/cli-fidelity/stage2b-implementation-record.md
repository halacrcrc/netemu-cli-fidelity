# 阶段 2B · canonical C3 + C6a/C5/C4/C7/C8/H2/H3/H6 九条流程闭环 · 实施记录

> 执行日期：2026-11-11
> 基线：`3d784e022c8e70ca3c4aafbce65edd1220064ed7`（detached HEAD，未提交、未推送）
> 上一记录：`docs/cli-fidelity/stage2a-implementation-record.md`（保留，不回写）
> 本包范围：**C3（canonical IOS DHCP）→ C6a → C5 → C4 → C7 → C8 → H2 → H3 → H6**
> 停止门禁：H6 全绿后停止；未开始 H4/H5。

---

## 1. 现场保护与基线复跑

| 项 | 结果 |
|---|---|
| `git symbolic-ref -q HEAD` | 失败 → **detached HEAD** |
| `git rev-parse HEAD` | `3d784e022c8e70ca3c4aafbce65edd1220064ed7`（与交接文档固定基线一致） |
| 未提交改动 | 起点 25 项（18 M + 7 组未跟踪），全程未 `reset`/`clean`/覆盖/删测 |
| `cargo fmt --all -- --check` | 基线 0；最终 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 基线 0；最终 0 |
| `cargo test --workspace` | 基线 **43 binaries / 208 passed / 0 failed**；最终 **44 binaries / 233 passed / 0 failed** |
| `npx --yes pnpm@10 --dir web run check` | 基线 0；最终 0 |
| `npx --yes pnpm@10 --dir web run build` | 基线 0；最终 0 |

基线数量与 Stage 2A 记录逐项吻合，**无本次回归**。共享构建目录由环境指向
`C:/Users/22534/.workbuddy/build/ftp-toolbox-target`（非仓库内 `target/`），本次**未**复现
Windows 指纹文件权限冲突，无需切换独立 `CARGO_TARGET_DIR`。

既有偏差（非本次引入，予以保留）：`tests/golden.rs` 的 VRP `display ip interface brief`
把 IOS 默认 `*down` 套到 VRP 的修正、`tests/scenarios.rs` 为 `AclRule` 新增
`src_port/dst_port` 字段的构造补参——均为字段/默认值适配，未删除、注释、跳过或放宽任何断言。

---

## 2. 两个前置问题的结论

### 2.1 C3 / C6a 编号偏差 —— 已关闭

- **结论**：主方案 §8 的 canonical 编号是 `C3 = IOS DHCP Pool / network / default-router`，
  `C6a = IOS 本地用户 / VTY / login local / 认证与传输`。Stage 2A 所称「C3 管理与登录」是
  **C6a 子集**，不能据此认定 DHCP 已完成；本包已**真正补做 C3 DHCP**。
- 处理：Stage 2A 历史记录原样保留（标题仍写 `C1→C2→C3→H0→H1`），不伪造成当时已正确编号。
  当前源码注释与规格段标签改为 canonical：
  - `crates/cli-dialects/tests/c3_mgmt_slice.rs` 文件头改为「规范编号 C6a」，并注明真正 C3
    由 `stage2b_flows.rs` 补做；**全部既有管理测试保留**。
  - `crates/cli-dialects/src/specs.rs` 段标签由「IOS / C3（管理与登录）」改为
    「IOS / C6a（管理与登录；Stage 2A 曾误标为 C3）」，文件头补编号更正说明。
  - 新记录、新支持矩阵、新测试（`stage2b_flows.rs`）一律使用 canonical 编号。
- C3 实体：`ip dhcp pool <name>` / `network <ip> <mask>` / `default-router` / `dns-server` /
  `no ip dhcp pool` / `no default-router` / `no network` / `no dns-server` /
  `ip dhcp excluded-address` / `no ip dhcp excluded-address` / `show ip dhcp binding` /
  VRP `ip pool` + `gateway-list` + `dns-list` + `dhcp select global`；重复 Discover 复用租约
  （`engine.rs` `dhcp_server_ingress` 修复）。
  > 校正（2026-11-12 收口）：初版记录曾写「`host` 静态绑定」，但当前 `CNode` 命令树**没有**
  > `host` 节点，属不实声明，已删除。§5.1 只要求「当前命令树承诺的 pool/network/default-router
  > 及本流程实际接线参数」，未要求静态绑定；不在本包补造该语法。
  > `no default-router` / `no network` / `no dns-server` 为 `pool` 视图真实节点。

### 2.2 enable 空凭据冲突 —— 已关闭（保留实现 + 收窄主方案文字 + 标待验证）

**证据（官方文档正文，DocumentationOnly；访问日期 2026-11-11）**

| 来源 | 版本 / 日期 | 正文要点 |
|---|---|---|
| Cisco《Security and VPN Configuration Guide, Cisco IOS XE 17.x》*Configuring Security with Passwords, Privileges, and Logins*（`m_sec-cfg-sec-4cli-0.html`） | IOS XE 17.x，Updated: January 11, 2021 | 「If an enable password has not been set, privileged EXEC mode can be accessed only by a **local CLI session (terminal connected to the console port)**.」；远程连接未配 enable 口令时「you will see the `% No password set` error message」 |
| Cisco Catalyst 9600《Security Configuration Guide, IOS XE 17.12》*Controlling Switch Access with Passwords and Privilege Levels* | 17.12.x | enable password「No password is defined. The default is level 15 (privileged EXEC level)」；enable secret「By default, no password is defined」；`enable` 步骤「Enter your password, **if prompted**」 |

**结论**

1. 当前 `Session` 是**设备本地 CLI**（等价于 console 直连，`remote: None`），**不是** C6b 的
   跨设备远程 VTY/SSH 会话；`c3_mgmt_slice.rs` 与 `stage2b_flows.rs` 均显式断言这一点。
2. 证据**支持**本地控制台在无 enable secret 时可直接进入 Priv，因此**保留实现与测试**，
   未编造远程 `% No password set` 拒绝文本。
3. 主方案 `netemu-rust-cli-kernel-evaluation-and-design.md` §8.1 已按证据**收窄适用范围**：
   该句仅约束远程 CLI 会话（Telnet/SSH/VTY），明确不适用于本地控制台（见该文件第 587 行
   「适用范围收窄」注）。
4. 远程线路的 `% No password set` 行为属 **C6b 范围，本包不实现**。无带版本设备样本，
   逐字报错文本**列待验证**；因此 C6a 标注为 **`Executed / 待验证`**，不宣称「完整支持」。

已配置 enable secret 的口令提示、错误拒绝（`% Access denied`、留在 Exec）、正确提权、
口令不回显/配置视图脱敏、状态不变测试全部继续通过（`stage2b_flows.rs`
`c6a_line_auth_modes_are_distinct_and_secrets_never_leak`、`c3_mgmt_slice.rs`）。

---

## 3. 九条流程支持矩阵

图例：**完成** = 语义/状态/数据面闭环且测试全绿；**待验证** = 语义已实现但缺带版本设备样本，
输出逐字形态不宣称；**未实现** = 明确不在本包。

| 流程 | 支持状态 | strict 规格 | 否定形式 | 正/负 L4 | 代表测试 |
|---|---|---|---|---|---|
| **C3** IOS DHCP | 完成（输出待验证） | 26 | `no ip dhcp pool`、`no default-router`、`no network` | 3（1 正 2 负） | `c3_dhcp_*`、`c3_vrp_dhcp_uses_vrp_config_wording` |
| **C6a** 管理与认证 | 完成（远程部分待验证/未实现） | 4 | `no login`、`no enable secret`、`no username` | 不适用（管理面，非数据面 L4；理由见 §5） | `c6a_line_auth_modes_are_distinct_and_secrets_never_leak`、`c3_mgmt_slice.rs` |
| **C5** IOS ACL | 完成（输出待验证） | 15 | `no access-list <n> [rule <i>]`、`no ip access-group <n> <dir>` | 3（TCP 正 + 2 负） | `c5_acl_*` |
| **C4** IOS NAT | 完成（输出待验证） | 13 | `no ip nat inside/outside`、`no ip nat inside source static`、`no ip nat inside source list <acl> interface <if> [overload]` | 2（1 正 1 负） | `c4_nat_*` |
| **C7** IOS OSPF | 完成（输出待验证） | 10 | `no router ospf <pid>`、`no network <ip> <wc> area <a>` | 2（1 正 1 负） | `c7_ospf_*` |
| **C8** IOS VRRP | 完成（输出待验证） | 7 | `no vrrp <vrid>`、`no vrrp <vrid> preempt` | 2（1 正 1 负） | `c8_vrrp_priority_election_preempt_and_negation` |
| **H2** VRP STP | 完成（输出待验证） | 13 | `undo stp priority`、`undo stp root`（config/if 两视图）、`stp disable` | 2（1 正 1 负） | `h2_stp_priority_root_role_and_undo` |
| **H3** VRP Eth-Trunk/LACP | 完成（输出待验证） | 12 | `undo eth-trunk`（`if` 视图成员移除；`config` 视图亦有节点） | 2（1 正 1 负） | `h3_eth_trunk_selected_members_failure_and_undo` |
| **H6** VRP OSPF | 完成（输出待验证） | 31 | `undo network`、`undo area`、`undo ospf <pid>` | 2（1 正 1 负） | `h6_vrp_ospf_*` |

> 「输出待验证」适用于全部九条：无版本化设备/eNSP 样本，故不写逐字输出黄金，
> 全部采用字段/结构/状态断言；`simulator_notices` 明确给出证据不足提示。

---

## 4. 每流程证据、否定形式、L4 与测试映射

| 流程 | 能力/证据 | 配置视图（真实状态） | 查询输出 | 否定形式与状态恢复 | 正/负 L4 |
|---|---|---|---|---|---|
| C3 | `DhcpServer` / DocumentationOnly+Unverified；`Executed / 待验证` | `ip dhcp pool`、`network`、`default-router`、`dns-server`；VRP 用 `ip pool`/`gateway-list`/`dns-list`/`dhcp select global` | `show ip dhcp binding`（真实租约） | `no ip dhcp pool` 清池+租约；`pool` 视图 `no default-router`/`no network`/`no dns-server` 逐项回退；无匹配池/全排除不伪造主机配置 | 池+网关下发后 `ping` 5/5；无池 / 全排除 0/5 |
| C6a | `LocalAuth` / DocumentationOnly；Unknown 不伪判不支持 | 真实 privilege、`login` 与 `login local` 区分、`password ***`、`secret ***` | `show running-config` | `no login`/`no enable secret`/`no username`；越界 privilege>15 拒绝且状态不变 | 不适用：管理面认证与传输配置，不产生跨设备 L4 流量；以「错误口令不提权 + 状态指纹不变」作负向验收 |
| C5 | `Acl` / 首条命中 + 隐式拒绝 | 真实 ACL 规则（含 TCP/端口），无占位行；`ip access-group <n> in/out` | `show access-lists`（规则/命中） | `no access-list <n> rule <i>` 删规则、`no ip access-group` 解绑、`no access-list <n>` 删表；非法规则/通配符不部分写入 | TCP 80 规则命中握手成功；端口不匹配 / 隐式拒绝 / 接口方向错误 0 通 |
| C4 | `Nat` / 角色 + 转换表 | 真实接口名（`ifname` 按方言生成）、inside/outside、静态映射、EasyIP | `show ip nat translations`（真实条目） | `no ip nat inside/outside`、`no ip nat inside source static`、`no ip nat inside source list <acl> interface <if> [overload]`（ACL/接口须与绑定一致，否则拒绝且状态不变；撤销清空派生转换表）；接口参数不再静默忽略，前置条件失败拒绝且状态不变 | inside/outside 正确时经转换表 5/5；撤销角色后配置与转换表同步清空 |
| C7 | `Ospf` / 动态邻居 + 路由 | `router ospf <pid>`、`network … area <a>` 用真实进程/区域（去硬编码 1/0） | `show ip ospf neighbor`（真实进程号、邻居） | `no router ospf <pid>`、`no network … area`（区域校验后撤销单条语句，配置/邻居/路由/数据面同步）；链路 shutdown 撤销邻居/路由，no shutdown 恢复重建 | 建邻后跨网段 5/5；区域不匹配 0/5 |
| C8 | `Vrrp` / 选举 + 切换 | `interface <真实名>` 下 `vrrp <id> ip/priority/preempt`，无占位拼接行 | `show vrrp`（Master/Backup/优先级） | `no vrrp <id> preempt`、`no vrrp <id>` | 主故障备份接管 VIP 业务保持 5/5；删除全部组无 Master 0/5；抢占恢复按配置 |
| H2 | `Stp` / 根桥 + 端口角色 | `stp priority <真实值>`、`stp root primary/secondary`、`stp disable` | `display stp brief`（模式/角色，非逐字黄金） | `undo stp priority` / `undo stp root`（config 与 if 两视图）恢复 32768 并重新选举根桥；非法优先级拒绝且状态不变 | 冗余无环 5/5；交换机间链路全断 0/5；恢复后重收敛 5/5；根桥随优先级变化（对照，非硬编码） |
| H3 | `Lacp` / Selected 协商 | `interface Eth-Trunk<n>`、`mode lacp-static`、成员 `eth-trunk <n>` | `display eth-trunk <n>`（成员/Selected） | `if` 视图 `undo eth-trunk` 成员移除（`config` 视图亦有同名节点）；非物理口加入自身、前置条件不满足拒绝且状态不变 | 聚合正常 + 单成员故障 5/5；全部成员失效 0/5；恢复重新 Selected 并 5/5 |
| H6 | `Ospf`（VRP）/ 双层视图 | `ospf <pid>` / `area <a>` / `network` 全用真实值 | `display ospf peer`（真实进程/邻居） | `undo network`、`undo area`、`undo ospf <pid>`；区域视图 `quit` 退进程视图、`return` 直达用户视图，IOS/VRP 模式树不互串 | 建邻后跨网段 5/5；区域不匹配 0/5；故障撤销与恢复重建 |

**L4 场景登记表**：`stage2b_flows.rs::STAGE2B_L4` 共 **18 条**，每条在对应测试中有可执行
ping/TCP 数据面断言；`stage2b_l4_registry_is_explicit_and_nonempty` 校验 8 个数据面流程
（C3/C4/C5/C7/C8/H2/H3/H6）**均同时登记正、负场景**且 ID 不重复。
列表：C3 三条、C5 三条、C4 两条、C7 两条、C8 两条、H6 两条、H2 两条、H3 两条。

**统一验收落实**：strict 规格 131 行（C3 26 / C4 13 / C5 15 / C6a 4 / C7 10 / C8 7 /
H2 13 / H3 12 / H6 31），含 2026-11-12 收口补齐的 5 行适用否定形式（C4 两行、C7 一行、
H2 两行）。ID 与 shape 全部由 `cli-engine::spec::derive_commands` 从 `CNode` 派生；解析覆盖正确/错误模式、缺失/多余/错误参数、唯一缩写、歧义、`?` 帮助与 Tab 候选
（`c3_dhcp_wrong_mode_invalid_params_and_state_unchanged`、
`c3_dhcp_help_completion_and_unique_abbrev`、`abbreviation_slice.rs`、`prototype.rs`）；
handler 完整预检后提交，非 `Executed` 路径以 `state_fingerprint` 断言状态不变；
`stage2b_dual_channel_and_inspection_is_pure_read` 断言设备输出无模拟器措辞、
提示带 `[netemu]` 前缀、旧文本 API 不丢设备输出、inspection 不改状态/不推进时间/不发探测。

---

## 5. 最新 lint 与迁移统计

由 `cli_engine::spec::lint_report` 从当前代码现场导出（未沿用 Stage 2A 的 37/258、36/356）：

| 方言 | 派生总数 | 已关联 | 显式未关联 | strict 总数 | strict 覆盖 | 重复 | 悬空 | 漂移 | strict 缺失 |
|---|---|---|---|---|---|---|---|---|---|
| IOS | 287 | 104 | 183 | 104 | 104 | 0 | 0 | 0 | 0 |
| VRP | 464 | 100 | 364 | 100 | 100 | 0 | 0 | 0 | 0 |
| Comware | 464 | 0 | 464 | 0 | 0 | 0 | 0 | 0 | 0 |

`已关联 + 显式未关联 = 派生总数` 对三个方言均成立（104+183=287、100+364=464、0+464=464）。
（2026-11-12 收口后重测：新增 5 个可执行节点全部关联并启用 strict，故 IOS/VRP 的
派生总数与已关联各 +3/+2，显式未关联数不变。）
`stage2b_lint_has_no_duplicates_dangling_drift_or_strict_gaps` 与
`strict_ids_are_derived_from_the_side_table_not_duplicated` 双重把关。
显式未关联清单（IOS 183 / VRP 364 / Comware 464）由 lint 报告逐条列出，作为迁移清单，
**不设隐藏豁免、不增加 lint 豁免**。

四态与双通道保持不变：`CNode` 仍是语法/模式/帮助/补全唯一输入源，旁表只存派生 ID、
shape 和元数据；平台差异走 `PlatformPolicy`/独立方言树，未在共享 handler 新增 `if dialect`
隐式语义分支；`Unknown` 不伪判 `UnsupportedByDevice`，待验证不是第五态；结构化 API
（`exec_command_result`）、旧文本 API（`exec_command`/`exec_and_run`）、WASM 契约保持兼容。

---

## 6. config_render 偏差逐项关闭证据

关闭证据集中在 `stage2b_config_render_closes_registered_deviations` 与
`config_render_slice.rs`、`c3_vrp_dhcp_uses_vrp_config_wording`：

| 偏差 | 关闭方式 | 断言 |
|---|---|---|
| DHCP 跨方言统一渲染 IOS 命令 | 按 `dialect` 分流：IOS `ip dhcp pool`/`default-router`/`dns-server`；VRP `ip pool`/`gateway-list`/`dns-list`/`dhcp select global` | IOS 侧无 `ip pool`/`gateway-list`；VRP 侧无 `ip dhcp pool`/`default-router` |
| ACL 占位行 | 从 `AclRule` 真实规则渲染（含 proto/端口/通配/命中） | 含 `access-list 100 permit tcp`；不含孤立占位行 `access-list 100` |
| IOS NAT 使用 VRP 接口名 | 接口名由 `ifname` 按方言+设备类型生成，NAT 角色/映射取真实状态 | 含 `interface GigabitEthernet0/0`；不含 `GigabitEthernet0/0/1 nat` 拼接 |
| VRRP 占位行 | 在真实接口块内按真实 vrid/vip/priority/preempt 渲染 | 含 `vrrp 1 ip …`；不含 `: vrrp 1 ip` 占位 |
| 用户名权限恒为 15 / VTY 层级 | 真实 privilege、`login` vs `login local`、`password ***`、`secret ***` | 含 `username admin privilege 3 secret ***`、`line vty 0 4`、脱敏口令；不含 `HIDDEN`/`TOPSECRET`/`LINEPW` |
| OSPF 进程/区域硬编码 1/0 | 取 `ospf_process` 与每条 network 的真实 area | 含 `router ospf 12`、`network … area 3`；不含 `router ospf 1` / `area 0` 硬编码 |

---

## 7. 最终门禁、WASM 与浏览器真实结果

| 门禁 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | **0** |
| `cargo clippy --workspace --all-targets -- -D warnings` | **0**（新增 `map_flatten`、`unnecessary_lazy_evaluations`、`clone_on_copy`、`single_element_loop` 均直接修复，未加任何 `#[allow]` 豁免） |
| `cargo test --workspace` | **0**；44 个测试二进制、**233 passed / 0 failed**（基线 43 / 208，净增 1 binary / 25 tests，即 `stage2b_flows.rs` 全部 25 条） |
| `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | **0**：Rust→WASM 基于当前源码编译、wasm-bindgen 生成声明、`wasm-opt` **本次成功**（`binaryen` 下载未复现 Stage 2A 的失败）。2026-11-12 收口后**重新生成**，产物 `net_sim_bg.wasm` + `.js`/`.d.ts`/`package.json` |
| `npx --yes pnpm@10 --dir web run check` | **0**（`tsc --noEmit`） |
| `npx --yes pnpm@10 --dir web run build` | **0**；Vite 消费重新生成的真实 `net_sim_bg-C8bh9BNI.wasm`（1,471.05 kB，gzip 519.00 kB），非声明桩 |

WASM 总门禁逐段如实记录：Rust→WASM 编译 ✅、wasm-bindgen 声明 ✅、wasm-opt ✅、
Web check ✅、Web build ✅。Stage 2A 记录的 `binaryen-version_130` 下载失败在本次环境未复现；
未关闭优化、未反复重试、未改配置换绿。

**浏览器（`playwright-cli`，`http://127.0.0.1:4173/` 本地 preview + 真实 WASM，视口 1600×1000）**

- 真实 WASM 加载：`window.__netemu` 可用，`exec_command_result` 为 function；拓扑在 WASM 中构建并按页面钩子刷新（UI 显示「设备 4」→ 重建后「设备 2」）。
- 九条流程代表命令全部 `Executed` 且落到真实设备/协议状态：C3（pool/network/default-router/dns）、C6a（username/enable secret）、C5（ACL TCP+端口）、C4（静态 NAT）、C7（ospf 12 / area 3）、C8（见 Rust 测试）、H2（stp priority / display stp brief）、H3（Eth-Trunk / lacp-static）、H6（ospf 9 / area 1 / display ospf peer）。查询输出逐条取自状态（`show running-config`、`show ip dhcp binding`、`show access-lists`、`show ip nat translations`、`show ip ospf neighbor`、`display current-configuration`、`display ospf peer`、`display stp brief`）。
- **双输出通道各渲染一次**：真实 xterm 终端中设备配置行与灰色 `[netemu] 平台能力或输出证据不足：行为待验证…` 提示各自独立输出；`TerminalPanel.svelte` 的 `deviceOutput` / `simulatorNotices` 分别只写一次。
- **口令不泄漏**：`enable secret ***`、`username admin privilege 1 secret ***`；设备输出与双通道均不含 `S3CRET`/`ENABLEPW`。（注：`sim.dump()` 作为存档状态快照保留口令密文以支撑 save/restore，这是既有存档契约，不是设备输出泄漏。）
- **结构化 API 与旧文本 API 均不丢设备输出**：`exec_command_result().deviceOutput` 含 `hostname Router1` / `extended IP access list 100`；`exec_command()` 旧文本同样含两者且带 `[netemu]` 提示通道。
- **存档加载清空会话并提示**：在 `Router1(config)#` 下 `load_with_report(dump)` → `Router1>`，`notices = ["加载将重置所有终端会话"]`，UI 同时显示该提示。
- **控制台**：`Total messages: 0 (Errors: 0, Warnings: 0)`，无本次引入错误。
- **视觉抓屏**：视口可用，已抓取 `docs/cli-fidelity/stage2b-browser.png`（1600×1000，含拓扑画布、真实终端、脱敏配置与提示通道）。这是单张冒烟抓屏，**不宣称完整视觉回归**（未做多分辨率/多主题对比）。

浏览器环境说明：preview 服务曾因后台进程生命周期退出一次并重启（`ERR_CONNECTION_REFUSED` 后重新 `goto` 成功），属本地服务管理问题，不计入测试失败。

---

## 8. 实际代码与测试清单

**修改（既有文件）**
`crates/cli-dialects/src/{config_render.rs,huawei.rs,ios.rs,lib.rs,show.rs}`、
`crates/cli-engine/src/lib.rs`、
`crates/sim-core/src/{device.rs,engine.rs,lib.rs,summary.rs}`、
`crates/sim-wasm/src/lib.rs`、
`crates/cli-dialects/tests/golden.rs`（仅注释+VRP 默认值偏差说明）、
`crates/sim-core/tests/scenarios.rs`（仅 `AclRule` 新字段补参）、
`web/src/{components/LeftPanel.svelte,components/TerminalPanel.svelte,lib/persistence.ts,lib/sim.ts,main.ts}`、
`crates/cli-dialects/tests/c3_mgmt_slice.rs`（仅 canonical 编号注释）、
`netemu-rust-cli-kernel-evaluation-and-design.md`（§8.1 适用范围收窄注）。

**新增**
`crates/cli-dialects/src/specs.rs`（Stage 2B 131 行 strict 规格，含收口补齐 5 行）、
`crates/cli-dialects/tests/stage2b_flows.rs`（**25 条测试**，含 18 条 L4 登记、lint/能力/
双通道/inspection/config_render 横切测试）、
`docs/cli-fidelity/stage2b-implementation-record.md`（本文件）、
`docs/cli-fidelity/stage2b-browser.png`（浏览器抓屏）。

**关键实现修复（非占位、非硬编码）**
- `engine.rs`：重复 DHCP Discover 复用已有租约；`dhcp select global` 写入并约束真实服务接口；
  OSPF 真实进程/区域参与建邻与配置渲染；VRRP 选举/抢占去硬编码；ACL TCP/端口规则参与数据面；
  NAT inside/outside 与 EasyIP 前置校验；LACP Selected、STP 收敛沿用既有真实实现。
- `device.rs`/`summary.rs`：DHCP 池/排除/租约、OSPF 进程与区域、ACL 端口、NAT 角色与 EasyIP、
  VRRP preempt、LACP Selected 等状态字段与存档 schema 兼容（缺省值可反序列化旧档）。
- `ios.rs`/`huawei.rs`：补齐 `overload`、`no access-list <n> rule <i>`、VRP OSPF 双层视图与
  `undo` 层级、STP/LACP 相关节点；`CNode` 仍是唯一语法源。
- `lib.rs`/`show.rs`/`config_render.rs`：查询与配置视图全部取自真实状态，见 §6。

---

## 8.1 收口核验与补齐（2026-11-12）

重新逐条把交付记录中的命令/否定形式声明与 `CNode` 派生 ID 清单比对，发现
**5 处不实声明**，其中 3 处同时是 §5 明确要求的适用否定形式缺口，已全部处理：

| 项 | 核验结论 | 处理 |
|---|---|---|
| C4 `no ip nat inside source list …` | 原命令树无此节点，但 §5.4 要求「覆盖适用 `no` 形式、解绑和配置撤销」 | **实补** `config/no/ip/nat/inside/source/list/@0number/interface/@0if-name[/overload]` 两节点 + `cmd::nat_no_easyip`：完整预检 ACL/接口与当前绑定一致，不匹配拒绝且状态不变；提交后清 `nat_easyip` 与派生 `nat_entries`。含带/不带 `overload` 两变体测试 |
| C7 `no network <ip> <wc> area <a>` | 原 IOS `ospf` 视图无 `no` 节点，但 §5.5 要求「适用 `no` 形式撤销网络/进程」 | **实补** `ospf/no/network/@0ipv4/@0wildcard/area/@0number` + `cmd::ospf_no_network_ios`（先校验区域与配置一致，再委派单条撤销，配置/邻居/路由/数据面同步）。含区域不匹配拒绝、状态不变、撤销后 ping 失败、重复撤销拒绝 |
| H2 `undo stp root` | 原命令树只有 `undo stp priority`，但 `stp root primary` 已承诺 | **实补** `config/undo/stp/root` 与 `if/undo/stp/root` 两节点（复用恢复默认优先级并重选根桥），两视图均有测试 |
| C3 `host` 静态绑定 | 命令树**没有** `host` 节点，属记录不实 | **删除该声明**，改为列出真实的 `pool` 视图 `no default-router`/`no network`/`no dns-server` 等节点；§5.1 未要求静态绑定，不补造语法 |
| H3 `undo interface eth-trunk` | 命令树无此路径，真实节点为 `config/undo/eth-trunk` 与 `if/undo/eth-trunk` | **修正记录文字**为真实节点名；`if` 视图成员移除既有测试已覆盖 |

补齐后：新增 5 个可执行节点**全部关联 strict 规格**，`duplicates/dangling/drift/strict_missing`
仍全为 0（见 §5 更新后的统计）；`stage2b_flows.rs` 仍 25 条测试、全绿；
fmt / clippy / `cargo test --workspace`（44 binaries / 233 passed）/ WASM（含 wasm-opt）/
Web check / Web build 全部重跑通过，WASM 基于补齐后的源码重新生成并被 Web 消费。

> 浏览器冒烟（§7）在补齐前完成，覆盖九流程代表命令、双通道、口令脱敏、双 API、
> 存档会话清空与控制台；新增的三个否定形式由 Rust 层测试覆盖，浏览器端**未重跑**，
> 如实记录为该次冒烟的范围边界，不以 API 结果冒充视觉回归。

---

## 9. HEAD、分支、工作区状态、未完成项与阻塞

| 项 | 状态 |
|---|---|
| HEAD | `3d784e022c8e70ca3c4aafbce65edd1220064ed7` |
| 分支 | **detached HEAD（no branch）** |
| 提交/推送 | **未提交、未推送**（按要求） |
| 工作区 | 18 个已跟踪文件 M；11 组未跟踪（含 `specs.rs`、8 个测试文件、`cli-engine/src/spec.rs`、`sim-core/src/profile.rs`、`docs/cli-fidelity/`）。全部为 Stage 2A + Stage 2B 累积改动，无临时诊断文件残留（`zz_*`、生成脚本、dump 文本、快照 yaml 已清理） |
| 构建目录 | 环境共享 `C:/Users/22534/.workbuddy/build/ftp-toolbox-target`；本次无权限冲突 |

**未完成项（明确后置，未开始）**
- H4/H5（BGP 与 Route-Policy）——按停止门禁，H6 全绿后停止，**未开始**。
- C6b 跨设备 SSH/远程 VTY 认证（含远程 `% No password set` 拒绝文本）。
- Comware / F5 / 防火墙 / 全量课程 UI 对齐、`run()` 事件动画。
- 全部九条流程的**逐字输出黄金**：无版本化设备/eNSP 样本，保持「输出待验证」；
  `simulator_notices` 已显式给出证据不足提示，不宣称「完整支持」。
- IOS 183 / VRP 364 / Comware 464 显式未关联节点，作为迁移清单保留，不是流程缺口。
- 2026-11-12 新增的三个否定形式未在浏览器端重跑冒烟（见 §8.1 范围边界）。

**阻塞**：无功能阻塞。唯一环境事项为本地 preview 服务需以后台进程常驻（曾退出一次并重启），
不影响任何门禁结果。`wasm-opt` 在本次环境成功，Stage 2A 的 binaryen 下载问题未复现。

---

## 10. 停止点

**H6 全绿后停止**：九条流程全部完成、44/233 Rust 测试全绿、六项最终门禁全绿、
真实 WASM 与浏览器冒烟（含视觉抓屏）完成、实施记录已交付。未启动 H4/H5。

2026-11-12 又做了一轮**本包内**收口核验（§8.1）：修正 5 处记录不实声明、补齐 3 项
§5 要求的适用否定形式，并重跑全部六项门禁与 WASM 生成。停止门禁保持不变：
**H4/H5 仍未开始**，本轮未扩张到 C6b / Comware / F5 / 防火墙 / 全量课程 UI。
