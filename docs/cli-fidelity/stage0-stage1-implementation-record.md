# 阶段 0 + 阶段 1 基础改造 + C1/H0 最小端到端原型 · 实施记录

> 首个执行包范围：`阶段 0` + `阶段 1 基础改造` + `C1/H0 最小端到端原型`。
> 不含 C2～C8、H1～H6、Comware、F5、防火墙或 C6b。
> 详细验收以 `netemu-implementation-handoff.md` 第 6 节为准，测试矩阵参照主方案 §8.1。

---

## 1. 现场确认与工具版本（阶段 0）

| 项目 | 实测值 |
|---|---|
| 固定基线提交 | `3d784e022c8e70ca3c4aafbce65edd1220064ed7` |
| 起始 HEAD / 分支 | detached HEAD，指向上述提交；工作区干净（`git status --short` 为空） |
| 结束时 HEAD | 未改变；未提交、未推送、未建分支 |
| rustc / cargo | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` / `cargo 1.99.1` |
| Node | `v26.13.0` |
| pnpm | 系统 `corepack` 的 `pnpm.js` 缺失，`pnpm --version` 不可用；改用 `npx --yes pnpm@10`（实测 `10.34.9`）执行全部前端命令 |
| wasm-pack | 仓库仅提供 `.github/bin/wasm-pack-linux-x64`（Linux ELF，禁止在 Windows 直接执行）；本机无 Windows 版 wasm-pack |
| Rust 目标 | 已安装：`x86_64-pc-windows-msvc`、两个 OHOS 目标；**无 `wasm32-unknown-unknown`**；`rustup target add wasm32-unknown-unknown` 卡在 `downloading component rust-std`（30s 超时），GitHub release 下载亦失败 |
| 已阅读 | 交接文档、`Cargo.toml`、`web/package.json`、`.github/workflows/ci.yml`、`docs/adr/0001-core-decisions.md`、主方案 §5～§9/§11/§15/§16、现有命令树/handler/黄金与场景测试 |

### 修改前基线（保存在仓库外层 `e:/网络命令/`）

| 命令 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | `FMT_EXIT=0`（`baseline-fmt.log` / `baseline-fmt-exit.txt`） |
| `cargo clippy --workspace --all-targets -- -D warnings` | `CLIPPY_EXIT=0`（`baseline-clippy.log`） |
| `cargo test --workspace` | `TEST_EXIT=0`，全部通过，含 `sim-core` 场景 42 项（`baseline-test.log`） |

**基线无既有失败。** 本次未通过 `reset` / `clean` / 覆盖方式处理任何文件；未提交改动全部为本次产生。

---

## 2. 证据表

证据等级采用主方案 §9.1：`DocumentationOnly` / `EnsPVerified` / `DeviceVerified` / `CommunitySample` / `Unverified`。
README 与仓库内的黄金测试只证明项目当前行为，**不作为厂商行为证据**。

| # | 平台行为 | 等级 | 来源与版本 | 核验方式 / 本切片结论 |
|---|---|---|---|---|
| E1 | IOS 三层路由器口默认 `shutdown`（管理 down） | `DocumentationOnly` | Cisco IOS XE 17.x 接口配置基线（文档基线） | 结构/状态断言：`ports[0].admin_up == false`；`PlatformPolicy` 显式策略测试。逐字厂商输出未验证 |
| E2 | VRP 路由器口默认非 `shutdown`（管理 up） | `DocumentationOnly` | Huawei 通用 VRP 文档基线（**未经 eNSP 校准**） | 结构/状态断言：`ports[0].admin_up == true`；不得用 `undo shutdown` 补救错误默认。逐字厂商输出未验证 |
| E3 | `ip address` / `ip route` / `ip route-static` 的配置语义 | `DocumentationOnly` | 同上两基线 | 状态断言（IP、前缀长度、静态路由表项）+ 配置视图同源渲染；未建逐字黄金 |
| E4 | `show ip route` / `display ip routing-table` / `show running-config` / `display current-configuration` 的**逐字输出** | `Unverified` | **无官方正文/设备样本** | 只做结构断言（包含目标前缀、`Static`/`S`、`ip route`、`ip address` 等字段）；`inspect` 一律 `pending-verification`，不得标 `full-support`；命令仍执行并返回 `Executed` |
| E5 | IOS 路由器口不具备二层交换端口能力，`switchport` 属 LAN 交换端口能力 | `DocumentationOnly` | Cisco Catalyst 9300 / IOS XE 17.6 接口与硬件组件配置指南（PDF，`profile.rs: CISCO_L2_PORT_REF`），**全文正文尚未复核** | 积极证据用于唯一一条 `UnsupportedByDevice`；设备报错复用既有解析器错误结构，**不编造厂商逐字报错** |
| E6 | VRP/Comware 上 `switchport` 一类能力 | `Unverified` | 无正向"不支持"证据 | `CapabilityVerdict::Unknown`；运行时不得据此返回 `UnsupportedByDevice` |
| E7 | VRP `save` 的平台有效性 | `DocumentationOnly` | Huawei 通用 VRP 文档基线 | 平台有效但持久化语义未实现 → `NotSimulated` + `recognized-only`；`device_output` 为空 |
| E8 | ICMP echo 的正/负数据面 | 项目内 L4 建模证据 | `crates/cli-dialects/tests/l4_scenarios.rs` 显式清单 | 端到端 `ping` 输出断言（5/5 通、0/5 超时）+ 丢弃计数；不宣称与真机逐字一致 |

> 快照级线索未用于建立任何逐字输出黄金测试。逐字比对需先取得设备样本并记录 OS/VRP 版本，属阶段 2 待验证项。

---

## 3. C1/H0 支持矩阵

稳定 ID = `模式路径 + 规范字面量路径 + 参数槽序号`，由 `CNode` 派生（`cli-engine/src/spec.rs`），旁表不含第二份命令路径。

### C1 / IOS（`ios-xe-17-router`）

| 命令 | 模式 | 稳定 ID | handler / 效果 | 状态字段 | 支持结论 |
|---|---|---|---|---|---|
| `enable` | Exec | `exec/enable` | `h_enable` / 模式切换 | `Session.mode` | Executed / 完整支持 |
| `configure terminal` | Priv | `priv/configure/terminal` | `h_config_terminal` / 模式切换 | `Session.mode` | Executed |
| `hostname R1` | Config | `config/hostname` | `cmd::hostname` / 配置 | `Device.name` | Executed |
| `interface g0/0` | Config | `config/interface` | `cmd::interface` / 模式切换 | `Session.if_idx` | Executed |
| `ip address A.B.C.D M.M.M.M` | If | `if/ip/address` | `cmd::ip_address` / 配置 | `Port.ip` / `Port.prefix` | Executed |
| `no ip address` | If | `if/no/ip/address` | `cmd::no_ip_address` / 配置 | 同上 | Executed |
| `no shutdown` | If | `if/no/shutdown` | `cmd::admin` / 配置 | `Port.admin_up` | Executed |
| `shutdown` | If | `if/shutdown` | `cmd::admin` / 配置 | `Port.admin_up` | Executed |
| `ip route …` | Config | `config/ip/route` | `cmd::static_route` / 配置 | `RouterState.static_routes` | Executed |
| `no ip route …` | Config | `config/no/ip/route` | `cmd::no_static_route` / 配置（本次新增） | 同上 | Executed |
| `ping <ip>` | Exec/Priv | `exec/ping` / `priv/ping` | `cmd::ping` / 流量动作 | `PingSession` | Executed |
| `show ip route` | Exec/Priv | `exec/show/ip/route` / `priv/show/ip/route` | `show::show_ip_route` / 查询 | 路由状态派生 | **Executed / 待验证**（E4） |
| `show running-config` | Exec/Priv | `exec/show/running-config` / `priv/show/running-config` | `config_render` / 查询 | `DevSummary` | **Executed / 待验证** |
| `exit` / `end` | Config/If | `config/exit` 等 | `h_exit` / `h_end` / 模式切换 | `Session.mode` | Executed |
| `switchport mode access` | If | `if/switchport/mode/access` | `cmd::switchport_mode` / 配置 | `Port.mode` | **UnsupportedByDevice**（E5，IOS Router）；IOS Switch 为 Supported |

### H0 / VRP（`vrp-generic-router`）

| 命令 | 模式 | 稳定 ID | handler / 效果 | 状态字段 | 支持结论 |
|---|---|---|---|---|---|
| `system-view` | Exec | `exec/system-view` | `h_system_view` / 模式切换 | `Session.mode` | Executed |
| `interface GigabitEthernet0/0/0` | Config | `config/interface` | `cmd::interface` / 模式切换 | `Session.if_idx` | Executed |
| `ip address A.B.C.D <len>` | If | `if/ip/address` | `cmd::ip_address` / 配置 | `Port.ip` / `Port.prefix` | Executed |
| `undo shutdown` | Config/If | `config/undo/shutdown` / `if/undo/shutdown` | `cmd::admin` / 配置 | `Port.admin_up` | Executed |
| `shutdown` | If | `if/shutdown` | `cmd::admin` / 配置 | `Port.admin_up` | Executed |
| `ip route-static …` | Config | `config/ip/route-static` | `cmd::static_route` / 配置 | `RouterState.static_routes` | Executed |
| `undo ip route-static …` | Config/If | `config/undo/ip/route-static` / `if/undo/ip/route-static` | `cmd::no_static_route`（本次新增） | 同上 | Executed |
| `undo ip address` | Config/If | `config/undo/ip/address` / `if/undo/ip/address` | `cmd::no_ip_address` / 配置 | `Port.ip` | Executed |
| `ping <ip>` | Exec/Config | `exec/ping` / `config/ping` | `cmd::ping` / 流量动作 | `PingSession` | Executed |
| `display ip routing-table` | Exec/Config/If | `exec/display/ip/routing-table` 等 | `show::display_ip_routing_table` / 查询 | 路由状态派生 | **Executed / 待验证** |
| `display current-configuration` | Exec/Config/If | `exec/display/current-configuration` 等 | `config_render` / 查询 | `DevSummary` | **Executed / 待验证** |
| `quit` / `return` | Config/If | `config/quit` / `config/return` 等 | `h_quit` / `h_return` / 模式切换 | `Session.mode` | Executed |
| `save` | Exec/Config | `exec/save` / `config/save` | `cmd::save_msg`（门禁前不调用） | 无 | **NotSimulated / 仅识别**（E7） |

### 严格门禁与迁移清单（lint 实测）

| 方言 | 派生可执行节点 | 已关联规格 | 严格门禁 | 显式迁移清单（未关联） | 重复/悬空/漂移 |
|---|---|---|---|---|---|
| IOS | 257 | 21 | `21/21`，`strict_ok=true` | 236 | 0 / 0 / 0 |
| VRP | 354 | 25 | `25/25`，`strict_ok=true` | 329 | 0 / 0 / 0 |

`unassociated` **全部显式列出**，无隐藏豁免；未关联节点保持阶段 0 行为，由 lint 报告输出迁移清单。
这是"阶段 1 基础改造"而非全量阶段 1：C2～C8、H1～H6 的可执行节点仍在迁移清单中。

---

## 4. 已接线 handler 分类与输出通道扫描

分类来自 `cli-engine/src/spec.rs::Effect` 与 `cli-dialects/src/specs.rs::effect_counts`。

| 类别 | 含义 | C1/H0 代表 handler |
|---|---|---|
| 配置 `state-change` | 完整预检后提交，改 `sim-core` 状态 | `cmd::ip_address` / `cmd::static_route` / `cmd::no_static_route` / `cmd::admin` / `cmd::no_ip_address` / `cmd::hostname` / `cmd::switchport_mode` |
| 查询 `query` | 从 `DevSummary` 实时渲染，无副作用 | `show::*`、`config_render::render` |
| 模式切换 `mode-switch` | 只改 `Session.mode` / `if_idx` | `h_enable` / `h_config_terminal` / `h_interface` / `h_exit` / `h_end` / `h_system_view` / `h_quit` / `h_return` |
| 会话 `session` | 登录/远程会话骨架 | `LoginFlow` / `RemoteSession`（保留迁移兼容，未在本切片纳入严格门禁） |
| 流量动作 `traffic` | 启动 ICMP echo，由引擎推进 | `cmd::ping` |
| 未实现 `unimplemented` | 平台已确认有效、承诺语义未模拟 | `cmd::save_msg`（由规格门禁拦截，handler 不执行） |

### 输出通道污染扫描结论

- 全仓 `device_output` 路径排查发现两处模拟器措辞污染，已修正：
  1. `cmd::save_msg` 的 `Info: The configuration takes effect immediately in the simulator; no need to save.` → 设备输出改为空；`save` 由 `NotSimulated` 门禁承担，提示进 `simulator_notices`。
  2. `cmd::ospf_network` 的 `Only area 0 is supported in this simulator` → 设备侧改为有证据的错误结构 `Unsupported area`，模拟器限制通过 `Ctx.notices` 输出。
- `exec_line_result` 的**所有** handler 路径（已关联 / 迁移清单 / 成功 / 失败）都会把 `Ctx.notices` 送入 `simulator_notices`，不丢弃。
- `scan_simulator_wording` 对原型全部命令的设备输出扫描为 0 命中；见 `prototype.rs::prototype_outputs_have_no_simulator_wording`、`handler_limitations_go_to_notices_not_device_output`。
- `CommandResult` 仅有 `status / device_output / simulator_notices / state_changed` 四个字段，由 `prototype.rs::command_result_has_no_simulation_event_stream` 穷举解构保证不新增事件流。
- `exec_and_run` / `exec_command` 保持 `String` 迁移期契约；`exec_and_run_result` / `exec_command_result` 为结构化入口。前端 `TerminalPanel` 两个通道各渲染一次。

---

## 5. L4 承诺场景清单

清单显式维护在测试目录 `crates/cli-dialects/tests/l4_scenarios.rs`，每项都有可执行断言，判据是 ping 输出与丢弃计数，不检查"命令返回成功"。

| 场景 ID | 方言 | 期望 | 说明 |
|---|---|---|---|
| `L4-C1-static-route-echo-ok` | IOS | 可达 | 双路由器 + 双向静态路由，5/5 通 |
| `L4-H0-static-route-echo-ok` | VRP | 可达 | 同上，VRP 默认 up |
| `L4-C1-no-route-echo-fail` | IOS | 不可达 | 未配置静态路由，0/5 + 丢弃 |
| `L4-H0-no-route-echo-fail` | VRP | 不可达 | 未配置静态路由，0/5 + 丢弃 |
| `L4-C1-link-shutdown-echo-fail` | IOS | 不可达 | 有路由但上联口 shutdown |
| `L4-C1-route-removed-echo-fail` | IOS | 不可达 | 经 CLI `no ip route` 撤销后不可达 |

未列入清单的协议/数据面行为不在本切片承诺范围。

---

## 6. 测试文件清单

| 文件 | 内容 |
|---|---|
| `crates/cli-engine/src/spec.rs`（`#[cfg(test)]`） | 稳定 ID 不受参数显示名影响、重复 ID、悬空/漂移/未关联、字面量与参数槽漂移、能力先于语义、证据不足仍 `Executed` |
| `crates/sim-core/src/profile.rs`（`#[cfg(test)]`） | Profile 独立与往返、IOS down / VRP up 显式策略、积极证据才能判不支持 |
| `crates/sim-wasm/src/lib.rs`（`#[cfg(test)]`） | 新档 schema 版本与 Profile、旧档映射报告、往返、会话与令牌清空、加载失败保持现场、Profile 级纯读 |
| `crates/cli-dialects/tests/prototype.rs` | 22 项 C1/H0 端到端验收（§6.1～§6.5 + §8.1 测试矩阵） |
| `crates/cli-dialects/tests/l4_scenarios.rs` | L4 承诺清单与 6 个真实数据面场景断言 |
| `crates/cli-dialects/tests/golden.rs` | 原有基础黄金；**仅一处**按 E2 修正 VRP 未接线端口 `*down` → `down`（旧黄金把 IOS 默认套用到 VRP），已加注释说明，非批量重写 |

---

## 7. 实施切片与进度记录

| # | 可验证子切片 | 结果 |
|---|---|---|
| 1 | 阶段 0：现场保护、CI/ADR/源码阅读、修改前基线 | 完成，基线 3 项全绿，日志已保存 |
| 2 | 稳定命令 ID（`Parsed.path`、`match_token` 槽序号、`command_id`、结构指纹 `shape_of`、lint） | 完成；cli-engine 单测全绿 |
| 3 | DeviceProfile / `PlatformPolicy` / 能力证据；`sim-core` 按策略注入默认管理状态；存档 schema v1 与 Profile 字段 | 完成；sim-core 单测与既有 42 项场景全绿 |
| 4 | 结构化结果、四态门禁、纯判断函数 `judge`、输出通道分离、兼容文本 API | 完成；`cargo test --workspace` 全绿 |
| 5 | `specs.rs` 旁表（仅派生视图）+ IOS/VRP 严格门禁 + 静态路由否定路径 | 完成；lint `strict_ok`，迁移清单 236 / 329 全部显式 |
| 6 | Profile 级与设备会话级纯读检查（`inspect_command` / `inspect_device_command` / WASM `verify_device_command` + 会话令牌） | 完成；状态摘要、虚拟时间、事件队列、丢弃计数均不变 |
| 7 | 旧存档映射、往返、会话清空 UI 提示与 `load_with_report` | 完成；sim-wasm 6 项原生测试全绿 |
| 8 | C1/H0 原型（方言独立、配置/数据面、四态、配置视图、lint 反例、污染扫描） | 完成；`prototype.rs` 22 项 + `l4_scenarios.rs` 2 项全绿 |
| 9 | 全量 CI 等价命令 | Rust 三项全绿；WASM/Web 构建受本机工具阻塞，见 §9 |

---

## 8. 待验证项、未完成项与偏差

### 待验证（不阻塞语义执行，禁止标为完整支持）

1. **逐字输出证据缺失**：`show ip route`、`display ip routing-table`、`show running-config`、`display current-configuration` 全部为 `Unverified`，`inspect` 返回 `pending-verification`。需设备样本 + 明确 OS/VRP 版本后才能建逐字黄金。
2. **E5 全文复核**：Cisco 9300 17.6 接口指南仅作为文档基线线索，正文未逐字复核；`UnsupportedByDevice` 的设备报错只复用既有解析器错误结构，不编造厂商文本。
3. **VRP 基线未校准**：`vrp-generic-router` 为 `DocumentationOnly` 通用基线，未经指定 eNSP 镜像校准；不得升级为 `EnsPVerified`。
4. **Comware**：仅有 `generic-unverified` 占位 Profile，本切片不做能力门禁，也不宣称支持。
5. **删除不存在静态路由的厂商行为**：`cmd::no_static_route` 对不存在的路由按幂等成功处理（状态不变）。真机是否报错缺证据，属阶段 2 待验证；本切片不伪造报错文本。

### 未关闭偏差（列入阶段 2，不属本切片验收范围）

1. `config_render` 的 IOS 静态路由使用前缀长度 `ip route <dst> <len> <nh>`，而 IOS CLI 输入使用点分掩码。当前按**结构断言**验证（目标前缀、下一跳、增删一致性），逐字配置文本待证据与阶段 2 修正。
2. `config_render` 其余已知问题（跨方言接口名、DHCP/VRRP/ACL 占位行、用户名权限恒为 15、VTY 层级、OSPF 硬编码）——主方案 §8.1 明确要求阶段 2 处理。
3. 迁移清单中 236（IOS）/ 329（VRP）个可执行节点尚未关联规格，保持阶段 0 行为；lint 完整列出，未设豁免。
4. `verify_command` 仅保留迁移兼容；课程 `CourseView.svelte` 与 `web/src/content/cmdmap.ts` 全量迁移属后续阶段。
5. `run()` 当前无 Web 调用方，本执行包**未接线**（遵循约束）。
6. C2～C8、H1～H6、Comware、F5、防火墙、C6b 全部未开始。

### 明确未做的事

- 未新建平行内核、后端、运行时命令脚本或完整 AAA；`CNode` 仍是语法/模式/帮助/补全唯一输入源；`CommandSpec` 只是派生视图。
- 未建设通用回滚/事务层；handler 均完整预检后提交，`ip route … 0.0.0.0` 的失败路径有状态不变测试。
- 未调用 `run_checks` 实现纯读检查、未发探测包、未推进虚拟时间。
- 未删除/注释/弱化任何测试，未扩大豁免，未做全仓格式化（格式化仅触及本次改动文件）、依赖升级或 UI 美化。
- 未提交、未推送；未生成也未提交 WASM 生成物（`web/src/wasm/` 为 `.gitignore` 明确排除目录）。

---

## 9. 验证命令与真实结果

| 命令 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | **通过**（`exit 0`）。格式化仅影响本次改动文件 |
| `cargo clippy --workspace --all-targets -- -D warnings` | **通过**（`exit 0`）；修复了一处 `len_zero` 告警 |
| `cargo test --workspace` | **通过**（`exit 0`）；合计 **177 项通过、0 失败**，含 `prototype.rs` 22 项、`l4_scenarios.rs` 2 项、`sim-wasm` 6 项、cli-engine 5 项、sim-core 3 项及全部既有测试 |
| `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | **未运行 / 阻塞**：本机无 Windows wasm-pack 与 `wasm32-unknown-unknown` 目标；仓库自带二进制是 Linux ELF，按约束不得在 Windows 执行；`rustup target add` 下载 rust-std 超时，GitHub release 下载失败 |
| `pnpm --dir web install --frozen-lockfile` | 系统 `pnpm` 不可用；以 `npx --yes pnpm@10 --dir web install --frozen-lockfile` 执行，**成功**（354 依赖，`exit 0`） |
| `pnpm --dir web run check` | 官方入口在当前环境失败：缺少 wasm-pack 生成的 `../wasm/net_sim` 声明。已用**临时声明桩**做过局部 `tsc --noEmit` 检查（`exit 0`），桩文件随后删除；此结果**不等同于官方命令通过** |
| `pnpm --dir web run build` | **失败**：147 个模块（含全部 8 个 `.svelte`）转换成功，随后 `Could not resolve "../wasm/net_sim" from "src/lib/sim.ts"`；唯一阻塞是缺失 WASM 生成物。另以 `svelte/compiler` 单独编译 8 个 `.svelte` 文件，`compile-errors=0`（该检查不等同于 build 通过） |

> 以上 WASM/Web 阻塞为**环境工具缺失**，不是代码回归；Rust 侧全量 CI 等价命令已通过。临时桩、临时脚本与失败构建产物均已清理，工作区仅保留本次代码/测试/文档改动。

---

## 10. 停止门禁检查

| 门禁 | 结论 |
|---|---|
| 必须复制语法才能挂接元数据 | 未触发；旁表只存 ID + 结构指纹 |
| 必须建设通用回滚层 | 未触发；预检后提交即可 |
| 纯读检查只能借助 `run_checks` | 未触发；`judge` 为纯函数 |
| 必须伪造厂商输出或削弱测试 | 未触发 |
| 工作区/基线存在无法安全确认的冲突 | 未触发；起始干净、HEAD 未变 |
| 本次修改引入尚未修复的回归 | 未触发；全仓测试 0 失败 |
