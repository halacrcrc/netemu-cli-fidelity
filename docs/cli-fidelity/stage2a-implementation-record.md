# 阶段 2A · 前置门禁 + C1→C2→C3→H0→H1 闭环 · 实施记录

> 本执行包范围：前置门禁复跑 + 流程 `C1 → C2 → C3 → H0 → H1`。
> **H1 全绿后停止**，不继续 C6a～H5、Comware 能力门禁、F5、防火墙。
> 本记录是 `stage0-stage1-implementation-record.md` 的续篇；C1/H0 的最小端到端原型已在阶段 0+1 建立，本切片在其上补齐 C2/C3/H1 并修复本切片横切问题。
> 验收以交接文档第 6 节与主方案 §8.1 测试矩阵为准。

---

## 1. 现场确认与工具版本

| 项目 | 实测值 |
|---|---|
| 固定基线提交 | `3d784e022c8e70ca3c4aafbce65edd1220064ed7` |
| 起始 / 结束 HEAD | 未改变；detached HEAD（`git branch --show-current` 为空）；未提交、未推送、未建分支 |
| 工作区改动 | 25 项：15 个修改 + 10 个新增（见 §1.1）；全部为阶段 0+1 与本次产生 |
| 现场保护 | 未 `reset` / `clean` / 覆盖任何文件；未删除/注释/弱化任何测试 |
| rustc / cargo | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` / `cargo 1.99.1` |
| Node | `v26.13.0` |
| pnpm | 系统 `pnpm` 不可用；统一用 `npx --yes pnpm@10`（实测 `10.34.9`） |
| wasm-pack | 本机已可用 `wasm-pack 0.15.1`；`wasm32-unknown-unknown` 目标已安装（与阶段 0+1 的"工具缺失"状态不同） |
| wasm-opt | 可选优化步骤需联网下载 `binaryen-version_130`；本机代理 `127.0.0.1:10808` 不通、直连 GitHub 超时 → **环境阻塞**，见 §2 |

### 1.1 工作区清单（`git status -s`）

修改（15）：
`crates/cli-dialects/src/config_render.rs`、`huawei.rs`、`ios.rs`、`lib.rs`、`tests/golden.rs`、`crates/cli-engine/src/lib.rs`、`crates/sim-core/src/device.rs`、`lib.rs`、`summary.rs`、`crates/sim-wasm/src/lib.rs`、`web/src/components/LeftPanel.svelte`、`TerminalPanel.svelte`、`web/src/lib/persistence.ts`、`sim.ts`、`main.ts`。

新增（10）：
`crates/cli-dialects/src/specs.rs`、`tests/abbreviation_slice.rs`、`tests/c2_h1_vlan_slice.rs`、`tests/c3_mgmt_slice.rs`、`tests/config_render_slice.rs`、`tests/l4_scenarios.rs`、`tests/prototype.rs`、`crates/cli-engine/src/spec.rs`、`crates/sim-core/src/profile.rs`、`docs/cli-fidelity/`。

> `web/src/wasm/` 为 `.gitignore` 排除目录，WASM 生成物不计入工作区改动；本次生成真实生成物供 Web 门禁消费（§2），未提交。

---

## 2. 前置门禁复跑 · 真实结果

所有退出码均从命令自身日志读取（不采信后台任务包装退出码）。

| # | 命令 | 真实结果 |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | **`FMT_EXIT=0`**，0 diff |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | **`CLIPPY_EXIT=0`**，0 warning / 0 error |
| 3 | `cargo test --workspace` | **`TEST_EXIT=0`**，**43 个测试二进制、208 通过、0 失败、0 ignored** |
| 4 | `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | **部分成功（环境阻塞）**：release 编译 + `wasm-bindgen` 成功，产出真实生成物；`wasm-opt` 因 binaryen 下载失败而退出非 0。详见下方说明 |
| 5 | `npx --yes pnpm@10 --dir web run check`（`tsc --noEmit`） | **`WEB_CHECK_EXIT=0`**；真实 WASM 声明解析通过，**无临时声明桩** |
| 6 | `npx --yes pnpm@10 --dir web run build`（`vite build`） | **`WEB_BUILD_EXIT=0`**；`✓ built in 3.54s`，打包 `net_sim_bg-Dj6UfkFb.wasm 1,860.27 kB（gzip 510.24 kB）`；PWA v2.0.0 `generateSW`，precache 11 项，生成 `dist/sw.js`。chunk-size 警告仅为提示 |

### WASM 门禁诚实说明

- 真实生成物（`web/src/wasm/`，本次 09:32 产出）：`net_sim_bg.wasm` 1,860,268 字节、`net_sim.js` 59,720、`net_sim.d.ts` 16,940、`net_sim_bg.wasm.d.ts` 7,535。
- `wasm-pack` 的 Rust→WASM 编译与 `wasm-bindgen` 绑定**均成功**；唯一失败是末尾的可选 `wasm-opt` 优化，需联网取 `binaryen-version_130`，本机网络不可达。
- 这是**环境工具/网络阻塞，不是代码回归**：Web check 能解析真实声明、Web build 能打包真实 `.wasm`，二者均绿。
- **未**为换取绿色而修改 `Cargo.toml` 关闭 `wasm-opt`，**未**用临时声明桩冒充真实生成物。

---

## 3. 证据表

证据等级采用主方案 §9.1：`DocumentationOnly` / `EnsPVerified` / `DeviceVerified` / `CommunitySample` / `Unverified`。
README 与仓库内黄金只证明项目当前行为，**不作为厂商行为证据**。无带版本设备样本 → 不写逐字黄金，采用字段/结构/状态断言。

| # | 平台行为 | 等级 | 来源与版本 | 核验方式 / 本切片结论 |
|---|---|---|---|---|
| E1 | IOS 三层路由器口默认 `shutdown` | `DocumentationOnly` | Cisco IOS XE 17.x 接口配置基线 | 状态断言 `ports[0].admin_up==false`；`PlatformPolicy` 显式注入 |
| E2 | VRP 路由器口默认非 `shutdown` | `DocumentationOnly` | Huawei 通用 VRP 基线（**未经 eNSP 校准**） | 状态断言 `ports[0].admin_up==true`；不得用 `undo shutdown` 补救错误默认 |
| E3 | `ip address` / `ip route` / `ip route-static` 配置语义 | `DocumentationOnly` | 同上两基线 | 状态断言 + 配置视图同源渲染 |
| E4 | `show ip route` / `display ip routing-table` / `show running-config` / `display current-configuration` 逐字输出 | `Unverified` | 无官方正文/设备样本 | 仅结构断言；`inspect` 一律 `pending-verification`，命令仍 `Executed` |
| E5 | IOS 路由器口不具备二层交换端口能力，`switchport` 属 LAN 交换端口能力 | `DocumentationOnly` | Cisco Catalyst 9300 / IOS XE **17.6** 接口与硬件组件配置指南 PDF（`profile.rs:91 CISCO_L2_PORT_REF`）；**正文已于本切片通过同族 17.13 HTML 章节复核**（`profile.rs:97 CISCO_L2_PORT_HTML_REF`）："A routed port is a Layer 3 interface only and does not support Layer 2 protocols, such as DTP and STP." | 积极证据用于唯一一条 `UnsupportedByDevice`（IOS Router 的 L2SwitchPort）；设备报错复用既有解析器错误结构，**不编造厂商逐字报错**。逐字设备报错文本仍未验证，等级保持 `DocumentationOnly` |
| E6 | VRP/Comware 上 `switchport` 一类能力 | `Unverified` | 无正向"不支持"证据 | `CapabilityVerdict::Unknown`；运行时不得据此返回 `UnsupportedByDevice`；语义已实现仍执行，`inspect` 待验证 |
| E7 | VRP `save` 平台有效性 | `DocumentationOnly` | Huawei 通用 VRP 基线 | 平台有效但持久化语义未实现 → `NotSimulated` + `recognized-only`；`device_output` 为空 |
| E8 | ICMP echo 正/负数据面 | 项目内 L4 建模证据 | `tests/l4_scenarios.rs` 显式清单 | 端到端 `ping` 输出断言（5/5 通、0/5 超时）+ 丢弃计数；不宣称与真机逐字一致 |

> E5 复核仅佐证书正文，不改变 `DocumentationOnly` 等级，也不作为逐字输出黄金依据。快照级线索未用于建立任何逐字黄金。

---

## 4. 逐流程支持矩阵

稳定 ID = `模式路径 + 规范字面量路径 + 参数槽序号`，由 `CNode` 派生（`cli-engine/src/spec.rs`）；`specs.rs` 旁表只存 ID + 结构指纹，不含第二份命令路径。

### C1 / IOS（`ios-xe-17-router`）— 继承阶段 0+1，本切片复跑全绿

`enable`、`configure terminal`、`hostname`、`interface g0/0`、`ip address`/`no ip address`、`shutdown`/`no shutdown`、`ip route`/`no ip route`、`ping`、`show ip route`（待验证）、`show running-config`（待验证）、`exit`/`end`。
`switchport mode access` 在 IOS Router 上 **UnsupportedByDevice**（E5）；IOS Switch 为 Supported。
本切片修复：`show running-config` 的 IOS 静态路由渲染改为点分掩码（§6）。

### C2 / IOS VLAN/Access/Trunk/SVI（新增 8 行规格）

| 命令 | 模式 | 稳定 ID | 效果 | 支持结论 |
|---|---|---|---|---|
| `vlan <n>` | Config | `config/vlan` | state-change | Executed（IOS Switch）；IOS Router **UnsupportedByDevice**（E5） |
| `interface vlan <n>` | Config | `config/interface/vlan/@0number` | mode-switch | Executed（选中 SVI） |
| `switchport mode trunk` | If | `if/switchport/mode/trunk` | state-change | Executed；初始放行范围由平台策略注入（IOS 全部 VLAN，`trunk_allowed=None`） |
| `switchport access vlan <n>` | If | `if/switchport/access/vlan/@0number` | state-change | Executed；非法 VLAN ID（如 4095）`InvalidCommand` 且状态不变 |
| `switchport trunk allowed vlan <list>` | If | `if/switchport/trunk/allowed/vlan/@0vlan-list` | state-change | Executed；IOS 替换列表，不强制 VLAN1 |
| `no switchport access vlan` | If | `if/no/switchport/access/vlan` | state-change | Executed；恢复默认 VLAN1（共享 `cmd::no_access_vlan`，无 `if dialect`） |
| `show vlan brief` | Exec/Priv | `exec/show/vlan/brief`、`priv/show/vlan/brief` | query | **Executed / 待验证**（PENDING_OUTPUT） |

### C3 / IOS 管理与登录（新增 8 行规格）

| 命令 | 模式 | 稳定 ID | 效果 | 支持结论 |
|---|---|---|---|---|
| `username <w> privilege <n> secret <w>` | Config | `config/username/@0word/privilege/@0number/secret/@0word` | state-change | Executed；`privilege>15` 完整预检拒绝（`InvalidCommand`，不创建用户）；15 接受 |
| `username <w> secret <w>` | Config | `config/username/@0word/secret/@0word` | state-change | Executed（默认 privilege 1） |
| `enable secret <w>` | Config | `config/enable/secret/@0word` | state-change | Executed；配置视图掩码 `enable secret ***` |
| `line vty <a> <b>` | Config | `config/line/vty/@0number/@0number` | mode-switch | Executed（进入 VTY 视图） |
| `login` / `login local` | Line | `line/login`、`line/login/local` | state-change | Executed |
| `transport input <list>` | Line | `line/transport/input/@0word-list` | state-change | Executed（telnet/ssh/all/none） |
| `exit` | Line | `line/exit` | mode-switch | Executed |

**enable 口令门禁**（`Session.enable_prompt`，本切片新增字段）：
- 无 `enable secret`：`enable` 直进 Priv。
- 有 `enable secret`：`enable` 置 `enable_prompt=true`，`prompt()` 返回 `Password: `，模式仍 Exec；下一行经 `enable_password_step` 校验——正确进 Priv、错误返回 `% Access denied` 并留在 Exec、清除等待标志。
- 口令交互**不改设备状态**（`state_fingerprint` 不变），口令**不回显进 `device_output`**。
- `state_fingerprint` 只跟踪设备状态，不含会话状态，故 `enable_prompt` 切换不影响指纹。

### H0 / VRP（`vrp-generic-router`）— 继承阶段 0+1，本切片复跑全绿

`system-view`、`interface GigabitEthernet0/0/0`、`ip address`/`undo ip address`、`shutdown`/`undo shutdown`、`ip route-static`/`undo ip route-static`、`ping`、`display ip routing-table`（待验证）、`display current-configuration`（待验证）、`quit`/`return`、`save`（**NotSimulated / 仅识别**，E7）。

### H1 / VRP VLAN/Trunk/SVI（新增 11 行规格）

| 命令 | 模式 | 稳定 ID | 效果 | 支持结论 |
|---|---|---|---|---|
| `vlan <n>` | Config | `config/vlan` | state-change | Executed / 待验证（E6 Unknown） |
| `vlan batch <list>` | Config | `config/vlan/batch` | state-change | Executed / 待验证 |
| `interface vlanif <n>` | Config | `config/interface/vlanif/@0number` | mode-switch | Executed |
| `port link-type access` | If | `if/port/link-type/access` | state-change | Executed / 待验证 |
| `port link-type trunk` | If | `if/port/link-type/trunk` | state-change | Executed；默认仅放行 VLAN1（`trunk_allowed=Some([1])`，平台策略注入） |
| `port default vlan <n>` | If | `if/port/default/vlan/@0number` | state-change | Executed / 待验证 |
| `port trunk allow-pass vlan <list>` | If | `if/port/trunk/allow-pass/vlan/@0vlan-list` | state-change | Executed；**要求先 `link-type trunk`**（否则 `InvalidCommand` "not a trunk port"，模式不变）；累加并保留 VLAN1 |
| `undo port default vlan` | If | `if/undo/port/default/vlan` | state-change | Executed；恢复 VLAN1（共享 `cmd::no_access_vlan`） |
| `display vlan` | Exec/Config/If | `exec/display/vlan`、`config/display/vlan`、`if/display/vlan` | query | Executed / 待验证 |

> Comware 本切片**不做能力门禁、不宣称支持**：`specs.rs` 无 Comware 行，全部 356 个可执行节点在迁移清单中（§5）。

---

## 5. 严格门禁与迁移统计（lint 实测，本次新鲜提取）

| 方言 | 派生可执行节点 | 已关联规格 | 严格门禁 | 显式迁移清单（未关联） | 重复/悬空/漂移/严格缺失 |
|---|---|---|---|---|---|
| IOS | 258 | 37 | `37/37`，`strict_ok=true` | 221 | 0 / 0 / 0 / 0 |
| VRP | 356 | 36 | `36/36`，`strict_ok=true` | 320 | 0 / 0 / 0 / 0 |
| Comware | 356 | 0 | `0/0`，`strict_ok=true` | 356 | 0 / 0 / 0 / 0 |

- `unassociated` **全部显式列出**，无隐藏豁免；`已关联 + 未关联 == 派生总数`（IOS 37+221=258；VRP 36+320=356；Comware 0+356=356）。
- 相较阶段 0+1（IOS 21/257、VRP 25/354）：IOS 新增 16 行（C2 8 + C3 8）、派生 +1（`no switchport access vlan` 子节点）；VRP 新增 11 行（H1）、派生 +2（`undo port default vlan` 子节点）。
- `effect_counts` 含 `state-change>0` 与 `query>0`（`prototype.rs::lint_reports_full_migration_backlog_and_strict_coverage` 断言）。
- 反例测试 `lint_catches_deliberate_drift_dangling_duplicate_and_strict_gap` 验证漂移/悬空/重复/严格缺失均能被捕获。

---

## 6. 横切修复

### 6.1 歧义缩写独立解析结果（`tests/abbreviation_slice.rs`）

- `CliError::Ambiguous` 是独立解析结果（非 Unknown/TooMany），经 `exec_line_result` 落到 `InvalidCommand` + 方言专属报错，且状态不变。
- IOS `show i`（同时命中 ip/ipv6）→ `% Ambiguous command:  "i"`；VRP `display i` → 方言专属歧义文本。
- 唯一缩写与全称执行等价。

### 6.2 config_render 本切片逐项关闭（`tests/config_render_slice.rs`，字段/结构断言）

| 项 | 关闭情况 | 位置 |
|---|---|---|
| IOS 静态路由点分掩码 | **已关闭**：`ip route <dst> <mask> <nh>`，`mask=ipv4_str(mask_from_prefix(len))`；不再渲染前缀长度 | `config_render.rs:201-202` |
| IOS SVI 点分掩码 | **已关闭**：`interface Vlan10` + `ip address <ip> <mask>` | `config_render.rs` SVI 分支 |
| IOS username 真实 privilege | **已关闭**：渲染存储的 privilege（如 7），不再硬编码 15；口令掩码 `secret ***` | `config_render.rs:303` 附近 |
| IOS enable secret 掩码 | **已关闭**：`enable secret ***` | `config_render.rs:297` 附近 |
| VTY 块渲染 | **已关闭**：`line vty 0 4` + ` login local` + ` transport input ssh` | `config_render.rs` mgmt 分支 |
| VRP 前缀长度形态 | **保持不变**：`ip route-static <dst> <len> <nh>`、`interface Vlanif10` + `ip address <ip> <len>` | `config_render.rs:204,189` |

### 6.3 后续流程偏差（本切片**未改**，列入后续）

- OSPF 硬编码进程 1 / area 0（`config_render.rs:220,226-227`）。
- DHCP 跨方言措辞 `ip dhcp pool`（VRP 应为 `dhcp server ip-pool` 一类）。
- NAT 硬编码接口名 `GigabitEthernet0/0/{idx+1}`（`config_render.rs:276-294`）。
- ACL / VRRP 占位行；BGP / OSPFv3 渲染细节。
- 以上属 C6a～H5 范围，本切片不扩大处理。

### 6.4 输出通道与四态

- `CommandResult` 仅 `status / device_output / simulator_notices / state_changed` 四字段；`device_output` 只放真实设备文本，模拟器限制/待验证提示只放 `simulator_notices`。
- 四态：`Executed` / `UnsupportedByDevice` / `NotSimulated` / `InvalidCommand`；"待验证"是证据元数据，不是第五态。
- `judge()` 纯函数：能力先于语义；仅 `Unsupported` 阻断（`Unknown` 放行）；`evidence.can_decide_capability()==false` → `PendingVerification`（仍 `Executed`）。
- 平台默认在 Profile 边界**显式注入**（`profile.rs:319-322` 的 `match dialect`），共享 handler 消费 `PlatformPolicy` 字段，**无新增 `if dialect` 隐式语义分支**。
- 文本 API（`exec_and_run`/`exec_command`）与结构化 API（`exec_and_run_result`/`exec_command_result`）兼容并存；命令检查纯读，不推进虚拟时间、不发探测包。

---

## 7. L4 承诺场景清单（10 项，`tests/l4_scenarios.rs`）

判据是 `ping` 输出（经 `exec_and_run_result` 走完整 CLI→引擎静默→输出渲染链）与丢弃计数，不检查"命令返回成功"，不用 `run_checks` 判分。

| 场景 ID | 方言 | 期望 | 说明 |
|---|---|---|---|
| `L4-C1-static-route-echo-ok` | IOS | 可达 | 双路由器 + 双向静态路由，5/5 通 |
| `L4-H0-static-route-echo-ok` | VRP | 可达 | 同上，VRP 默认 up |
| `L4-C1-no-route-echo-fail` | IOS | 不可达 | 未配静态路由，0/5 + 丢弃 |
| `L4-H0-no-route-echo-fail` | VRP | 不可达 | 未配静态路由，0/5 + 丢弃 |
| `L4-C1-link-shutdown-echo-fail` | IOS | 不可达 | 有路由但上联口 shutdown |
| `L4-C1-route-removed-echo-fail` | IOS | 不可达 | 经 CLI `no ip route` 撤销后不可达 |
| `L4-C2-same-vlan-echo-ok` | IOS | 可达 | 交换机两 access 口同属 VLAN10 同网段，5/5 通 |
| `L4-C2-cross-vlan-isolated-echo-fail` | IOS | 不可达 | 两 access 口分属 VLAN10/20 无 SVI，0/5 + 100% loss |
| `L4-H1-cross-vlan-routed-echo-ok` | VRP | 可达 | VLAN10/20 + 双 Vlanif SVI 路由，跨 VLAN 5/5 通 |
| `L4-H1-cross-vlan-isolated-echo-fail` | VRP | 不可达 | 两 access 口分属 VLAN10/20 无 Vlanif，0/5 + 100% loss |

> VLAN 隔离场景的丢弃计数语义：无 SVI 时网关 ARP 无法解析，报文在主机侧即不可达，引擎转发丢弃计数为 0 属正确语义；隔离判据是 `0 received + 100% loss`。路由器场景仍要求 `dropped>0`（`l4_scenarios.rs:318` 分支断言）。
> 未列入清单的协议/数据面行为不在本切片承诺范围。

---

## 8. 测试文件清单

| 文件 | 内容 |
|---|---|
| `crates/cli-engine/src/spec.rs`（`#[cfg(test)]`） | 稳定 ID/结构指纹、重复/悬空/漂移/未关联、能力先于语义、证据不足仍 Executed |
| `crates/sim-core/src/profile.rs`（`#[cfg(test)]`） | Profile 独立与往返、IOS down/VRP up 显式策略、trunk 平台默认、积极证据才能判不支持 |
| `crates/sim-wasm/src/lib.rs`（`#[cfg(test)]`） | schema 版本/Profile、旧档映射报告、往返、会话令牌清空、加载失败保持现场、Profile 级纯读 |
| `crates/cli-dialects/tests/prototype.rs` | C1/H0 端到端验收 + lint 全量迁移清单/严格覆盖 + 反例 + 输出通道污染扫描 |
| `crates/cli-dialects/tests/abbreviation_slice.rs` | 歧义缩写独立结果（IOS/VRP）、状态不变、唯一缩写等价 |
| `crates/cli-dialects/tests/c2_h1_vlan_slice.rs` | 9 项：C2 能力门禁/参数校验/否定/trunk 默认；H1 trunk 默认/前置条件/累加/否定 |
| `crates/cli-dialects/tests/c3_mgmt_slice.rs` | 6 项：enable 门禁（无/有 secret、错/对口令、不泄漏）、privilege 边界（>15 拒绝/15 接受）、管理面配置视图掩码 |
| `crates/cli-dialects/tests/config_render_slice.rs` | 4 项：IOS 静态路由/SVI 点分掩码、username 真实 privilege、VRP 前缀长度保持 |
| `crates/cli-dialects/tests/l4_scenarios.rs` | 3 个测试函数覆盖 10 个真实数据面场景 |
| `crates/cli-dialects/tests/golden.rs` | 原有基础黄金；仅一处按 E2 修正 VRP 未接线端口 `*down`→`down`（阶段 0+1 已加注释） |

---

## 9. 浏览器冒烟（真实 WASM 集成）

经 `window.__netemu`（`main.ts:77`，与终端 submit 同一调用链 `exec → 引擎静默 → 界面刷新`）驱动真实 WASM `Sim`，调用 `exec_command_result` / `prompt` / `inspect_command`（`TerminalPanel` 使用的同一结构化 API）。

| 验证点 | 结果 |
|---|---|
| WASM 加载与提示符 | 视口可见时 xterm 渲染 `Router1>` 横幅；`prompt(0)="Router1>"`、`prompt(1)="Router2>"` |
| C3 enable 门禁 | 无 secret `enable`→`Router2#`；配 `enable secret Cisco@123` 后 `enable`→`Password: `；`wrongpass`→`% Access denied` 回 `Router2>`；`Cisco@123`→`Router2#`；口令不进 `deviceOutput` |
| 四态 | `Executed`（show/ping/ip route）；`UnsupportedByDevice`（IOS Router `switchport mode access`，spec `if/switchport/mode/access`，support `unsupported-by-device`）；`NotSimulated`（VRP `save`，spec `exec/save`，support `recognized-only`，evidence `DocumentationOnly`）；`InvalidCommand`（`show cdp neighbors`/`show vlan` 不完整） |
| 双通道 | `show ip route`/`show running-config` 真实设备文本进 `deviceOutput`；`[netemu] 平台能力或输出证据不足：行为待验证…` 进 `simulatorNotices` |
| 配置视图 | `enable secret ***` 掩码；`ip route 10.0.0.0 255.255.255.0 192.168.1.1`（点分掩码）浏览器渲染确认 |
| 纯读检查 | `inspect_command` 平台级（无设备实例），状态/虚拟时间/事件队列不变 |

> **诚实说明**：会话中途 in-app 浏览器视口变为隐藏（0×0，`visibilityState=hidden`），xterm 视觉抓屏与指针操作受阻（`NATIVE_BROWSER_VIEWPORT_UNAVAILABLE`）。早期视口可见时已确认终端横幅正常渲染；实质 WASM+CLI 集成经应用真实暴露 API 全量验证（上表）。视觉渲染层未能在隐藏视口下再次抓屏，如实记录，不宣称完成视觉回归。
> 冒烟用的 dev server 已停止；浏览器 localStorage 拓扑为冒烟产物，与代码工作区无关。

---

## 10. 逐流程闭环状态

| 流程 | 规格严格门禁 | 解析/模式/参数/缩写 | 能力+证据 | 预检状态语义 | 否定形式 | 配置视图 | 设备输出 | L4 正/负 | 测试 | 状态 |
|---|---|---|---|---|---|---|---|---|---|---|
| C1 | ✓ | ✓ | ✓（E5） | ✓ | ✓（`no ip route`/`no ip address`） | ✓（点分掩码修复） | ✓（待验证） | ✓（4 场景） | ✓ | **完成** |
| C2 | ✓ | ✓ | ✓（E5/E6） | ✓（非法 VLAN ID 状态不变） | ✓（`no switchport access vlan`） | ✓（SVI 点分掩码） | ✓（待验证） | ✓（2 场景） | ✓（9 项含 H1） | **完成** |
| C3 | ✓ | ✓ | ✓ | ✓（privilege>15 拒绝、口令交互状态不变） | — | ✓（掩码/真实 privilege/VTY） | ✓（`% Access denied`） | —（管理面非数据面） | ✓（6 项） | **完成** |
| H0 | ✓ | ✓ | ✓ | ✓ | ✓（`undo ip route-static`/`undo ip address`） | ✓（前缀长度保持） | ✓（待验证） | ✓（2 场景） | ✓ | **完成** |
| H1 | ✓ | ✓ | ✓（E6 Unknown） | ✓（非 trunk 拒绝、模式不变） | ✓（`undo port default vlan`） | ✓（Vlanif 前缀长度） | ✓（待验证） | ✓（2 场景） | ✓（9 项含 C2） | **完成** |

**H1 全绿，按约束停止。** 不继续 C6a～H5。

---

## 11. 待验证项、未完成项与偏差

### 待验证（不阻塞语义执行，禁止标完整支持）

1. 逐字输出证据缺失：`show ip route`/`display ip routing-table`/`show running-config`/`display current-configuration`/`show vlan brief`/`display vlan` 均 `pending-verification`；需设备样本 + 明确 OS/VRP 版本后才建逐字黄金。
2. E5 逐字设备报错文本未验证：正文已复核（17.13 HTML），但设备逐字报错仍复用既有解析器错误结构，等级保持 `DocumentationOnly`。
3. VRP 基线未校准：`vrp-generic-router` 为 `DocumentationOnly` 通用基线，未经指定 eNSP 镜像校准，不得升级 `EnsPVerified`。
4. Comware：仅 `generic-unverified` 占位 Profile，本切片不做能力门禁、不宣称支持。
5. 删除不存在静态路由的厂商行为：`no_static_route` 按幂等成功处理（状态不变），真机是否报错缺证据。

### 未完成 / 后续偏差

1. config_render 后续流程问题（OSPF 硬编码、DHCP 跨方言措辞、NAT 硬编码接口名、ACL/VRRP 占位）——见 §6.3，属 C6a～H5。
2. 迁移清单 221（IOS）/ 320（VRP）/ 356（Comware）个可执行节点未关联规格，保持阶段 0 行为，lint 完整列出，无豁免。
3. `verify_command` 仅保留迁移兼容；`CourseView.svelte` 与 `web/src/content/cmdmap.ts` 全量迁移属后续阶段。
4. `run()` 当前无 Web 调用方，本执行包**未接线**（遵循约束）。
5. C6a～C8、H2～H6、Comware 能力门禁、F5、防火墙全部未开始。
6. WASM `wasm-opt` 可选优化受网络阻塞（§2）；非代码回归，未关闭优化换绿。
7. 浏览器视觉回归在隐藏视口下未完成抓屏（§9）；实质集成已验证。

### 明确未做的事

- 未新建平行内核/后端/运行时命令脚本/完整 AAA；`CNode` 仍是语法/模式/帮助/补全唯一输入源，`CommandSpec` 只是派生视图。
- 未建通用回滚/事务层；handler 完整预检后提交，失败路径有状态不变测试。
- 未用 `run_checks` 实现纯读检查、未发探测包、未推进虚拟时间。
- 未删除/注释/弱化任何测试，未扩大豁免，未做全仓格式化（仅触及本次改动文件）、依赖升级或 UI 美化。
- 未新增 `if dialect` 隐式语义分支；平台默认在 Profile 边界显式注入。
- 未提交、未推送；未提交 WASM 生成物（`.gitignore` 排除）。
- 临时迁移统计测试 `zz_scratch_stats.rs` 用后即删，工作区不残留。

---

## 12. 停止门禁检查

| 门禁 | 结论 |
|---|---|
| 必须复制语法才能挂接元数据 | 未触发；旁表只存 ID + 结构指纹 |
| 必须建设通用回滚层 | 未触发；预检后提交即可 |
| 纯读检查只能借助 `run_checks` | 未触发；`judge` 为纯函数 |
| 必须伪造厂商输出或削弱测试 | 未触发 |
| 必须新增 `if dialect` 隐式语义分支 | 未触发；平台默认 Profile 边界显式注入 |
| 工作区/基线存在无法安全确认的冲突 | 未触发；HEAD 未变、无 reset/clean |
| 本次修改引入尚未修复的回归 | 未触发；全仓 208 通过 0 失败 |
| H1 全绿后是否继续扩大范围 | **已停止**，等待下一次交接 |