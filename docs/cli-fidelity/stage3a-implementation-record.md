# 阶段 3A · 课程对照表 Profile 级 inspection → 纯读/主动判分分离 → 5 个代表实验闭环 · 实施记录

> 执行日期：2026-11-12
> 基线：`3d784e022c8e70ca3c4aafbce65edd1220064ed7`（detached HEAD，未提交、未推送）
> 上一记录：`docs/cli-fidelity/stage2c-implementation-record.md`（保留，不回写）
> 本包范围：工作项 A（课程对照表）→ B（实验检查契约 / UI）→ C（5 个代表实验与浏览器回归）
> 停止门禁：5 个代表实验与关键浏览器流程完成后停止；**未进入 C6b / 阶段 3B / Comware / F5 / 防火墙扩展**。

---

## 1. 现场保护与基线复跑

| 项 | 结果 |
|---|---|
| `git symbolic-ref -q HEAD` | 失败 → **detached HEAD** |
| `git rev-parse HEAD` | `3d784e022c8e70ca3c4aafbce65edd1220064ed7` |
| 起点工作区 | 17 个已跟踪 M + 12 组未跟踪；全程未 `reset` / `clean` / 覆盖 / 删测 / 放宽断言 |
| 基线 `cargo test --workspace` | **45 binaries / 250 passed / 0 failed**，与 Stage 2C 记录一致 |
| 构建目录 | 共享 target 在本环境有指纹文件权限冲突 → 全程使用独立 `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-3a-target` |

已完整阅读交接文档、主方案 §5.4 / §5.5 / §11 / §15、Stage 2C 记录，以及真实 `lab-checks`、
`sim-wasm`、`labs.ts`、`LeftPanel.svelte`、`CourseView.svelte`、`sim.ts`、`cmdmap.ts`、`spec.rs`、
`profile.rs`、`engine.rs` 等实现后再改代码。

---

## 2. 实际代码与测试

### 2.1 生产代码

| 文件 | 改动 |
|---|---|
| `crates/lab-checks/src/lib.rs` | 新增 `CheckBehavior`（`ReadOnly` / `ActiveExperiment`）、`CheckKind::behavior()`、`CheckGroup`（`Single` / `Negate(routeAbsent)` / `Either(portRoleEither)`）、`LabCheck`、`LabCheckResult`；`run_read_only_checks(&Engine, …)` 与 `run_active_checks(&mut Engine, …)` 双入口；`eval_read_only(&Engine, …)` 为纯状态判断唯一实现，`eval_probe(&mut Engine, …)` 只处理四类主动探测；`run_one_value` 是唯一组合点。**旧 `run_checks` 保留兼容并走主动入口**，新增 `run_checks_read_only` 供旧 `check_*` 包装器使用。主动入口整批只调用一次 `run_to_quiescence(ACTIVE_CONVERGENCE_NS = 45_000_000_000)`。 |
| `crates/sim-wasm/src/lib.rs` | 34 个纯状态 `check_*` 改走 `run_checks_read_only`（不再隐藏 45s 推进）；`check_ping` / `check_ping6` / `check_tcp_probe` / `check_login` 保留主动推进；新增 `read_only_lab_checks(defs_json)` / `run_lab_checks(defs_json)` 两个同步批量入口（wasm-bindgen 自动生成 d.ts）；抽出 `verify_device_command_checked()` 纯逻辑入口；**会话序号改为单调**，加载/重置不回退，旧令牌不会因重新签发而复活。 |
| `crates/sim-wasm/src/lab_defs.rs`（新增） | Web `LabCheckDef.kind` → `CheckKind` 的 JSON 协议，覆盖全部 39 种 type；设备名按真实 `Engine` 的 `Device::name` 解析；结果序列化含 `behavior` / `skipped`。只做声明转换，不复制判断语义。 |
| `web/src/lib/sim.ts` | 新增 `LabCheckApi` / `labCheckApi(sim)` 类型桥；`inspectCommand` / `verifyDeviceCommand` / `sessionIdOf` 原有封装保持不变。 |
| `web/src/lib/labs.ts` | 结果模型 `LabCheckResult { label, passed, detail, behavior, skipped }`；`LabCheckDef.behavior?` 元数据；`checkBehavior()` / `labActiveBehaviors()` / `labHasActiveChecks()` / `labHasReadOnlyChecks()`；`runLabChecks(lab, 'readOnly' \| 'activeExperiment')` 一次调用真实 WASM 批量入口，**每次重新判分、不缓存**。修正 `office-l3`（Comware 拓扑的 VRP SVI 命令/标签）、`stp-redundancy`（VRP 设备误写 `show spanning-tree`）、`enterprise`（去掉未由检查证明的「NAT 生效」宣称）。 |
| `web/src/lib/store.svelte.ts` | `app.labChecks` / `setLabChecks` 切换到 `LabCheckResult[]`。`app.labPassed` 存档格式未动。 |
| `web/src/components/LeftPanel.svelte` | 两个明示按钮（纯读检查 / 运行实验判分）；执行前显示本实验真实行为清单；结果逐项显示 `纯读` / `主动实验` 来源与 `skipped` 的「—」；**只有主动按钮触发 `markLabPassed`**，且要求 `passed && !skipped`；无纯读项时按钮禁用。 |
| `web/src/components/CourseView.svelte` | 切换到 `sim.inspect_command` 的结构化 `InspectionResult`；缓存 key = `profileId|mode|cmd`（不含语言，异常不缓存）；逐单元格显示 `support` / `status` / `evidence` / `specId` / `detail`；平台级标签与纯读说明；暴露 `window.__netemu.cmdmapInspections()` 供浏览器验收。 |
| `web/src/content/cmdmap.ts` | 新增显式 `PROFILE_IDS`、`TOPIC_PROFILE_ROLE`（32 个 topic 全部登记）、`CmdRow.role` / `profileIos|Vrp|Comware`、`rowMode()` / `profileForRow()`；L2 主题选 switch Profile，`campus` 的 Trunk 行行级覆盖为 switch；修正 `mgmt` 对照行 VRP/Comware `local-user admin` 缺少 password 参数的明确错误写法。未登记 topic 直接报错而不是静默套 router。 |
| `web/src/content/labs-en.ts` | 同步修正三个实验的英文命令 / 标签。 |
| `web/src/lib/i18n-dict.ts` | 新增中英文行为声明、双按钮、结果来源、对照表 5 种状态与 Comware 说明文案。 |

**未改动**：`TerminalPanel.svelte`（双通道仍各渲染一次）、`sim-core` 唯一状态事实来源、四态执行状态、
`exec_command` 旧文本 API、`verify_command` 迁移期兼容 API、同步 WASM 契约、`run()` 前端接线。

### 2.2 新增测试

| 文件 | 数量 | 覆盖 |
|---|---|---|
| `crates/cli-dialects/tests/stage3a_cmdmap.rs` | 5 | 直接从真实 `cmdmap.ts` `include_str!` 解析 438 个单元格；router/switch 结构化状态差异；Comware 全部不得 `full-support`；invalid / pending 区分；探测并定位被拒绝行（`REJECTED=0`）。 |
| `crates/sim-wasm/src/stage3a_tests.rs` | 10 | 行为分类矩阵（4 主动 / 12+ 纯读）；纯读批量前后状态指纹、`now_ns`、协议/队列统计不变；主动批量收敛 + 探测 + 负向断言元数据；混合检查与旧 `run_checks` 共享判断语义；旧 `check_*` 副作用拆分；`inspect_command` / `verify_device_command_checked` 纯读性与真实 Profile/模式；错误令牌与加载/重置失效；双通道与旧文本 API；39 种 JSON 检查类型可解析；5 个代表实验声明可解析并正确分类。 |

既有 250 个测试全部保留并通过，未删除、跳过或放宽断言。唯一调整：`sim-wasm::tests::successful_load_clears_sessions_and_session_tokens`
中 `next_session_seq == 0` 改为 `== 7`，理由是本包把会话序号改为单调以杜绝旧令牌复活，属于收紧而非放宽。

---

## 3. 课程对照表状态映射

| `InspectionResult.support` | UI 标记 / 文案 | 说明 |
|---|---|---|
| `full-support` | `✓ 完整支持` | 只依据结构化 `support`，**不由 `passed` 单独推导** |
| `recognized-only` | `◐ 仅识别 / 未模拟` | `status = NotSimulated`，证据充分时才判为仅识别 |
| `unsupported-by-device` | `⊘ 当前 Profile 不支持` | 能力门禁（如 IOS router 的 `switchport`） |
| `invalid-command` | `✗ 命令错误` | `status = InvalidCommand` |
| `pending-verification` | `? 待验证` | **证据元数据，不是第五种执行状态**；`status` 仍可为 `Executed` |
| Comware 任意 `full-support`（防御） | `? 待验证` | 阶段 4 拆树与 strict 覆盖未完成，UI 层不显示完整支持；不伪造结构化结果 |

- 每行显式选择 Profile：`TOPIC_PROFILE_ROLE` 逐 topic 登记 + 行级 `role` / `profile*` 覆盖；L2 主题（vlan / stp / lag / stack / mlag / ha / vxlan / isolation）用 switch，其余 router。
- 缓存 key 含 Profile、模式、命令；语言切换不进入 key；WASM 异常不写缓存。
- Profile 级检查纯读：不创建设备、不推进协议、不改虚拟时间。

---

## 4. 纯读 / 主动检查分类矩阵

| 类别 | `LabCheckDef.kind.type` / `CheckKind` | 纯读路径 | 主动路径 |
|---|---|---|---|
| 纯状态读取 | `vlanExists` `accessVlan` `trunkAllows` `sviIp` `portIp` `routeExists` `staticRoute` `mgmtUser` `vtyTransport` `rootBridge` `portRole` `portRoleEither` `portState` `lagMember` `lagSelected` `mplsLsp` `vrrpMaster` `ospfNeighbors` `ospfv3Neighbors` `securityZone` `securityPolicy` `ipsecTunnel` `vpnConnected` `wlan*` `wifi*` `vxlanVni` `stackMember` `mlagDomain` `evpnRoute` `ltmVirtual` `portIsolate` `muxRole` `srv6Route` | **执行**（`&Engine`，零副作用） | 一次收敛后执行，结果仍标 `readOnly` |
| 组合纯读 | `routeAbsent` = `Negate(RouteExists)`；`portRoleEither` = `Either(PortRole, PortRole)` | 执行同一判断函数 | 执行同一判断函数 |
| 主动探测 | `ping` `ping6` `tcpProbe` `login`（含 `expect:false` 负向） | **skipped**，不计通过/失败 | 整批一次收敛后真实发包 / 登录，标 `activeExperiment` |

- `run_read_only_checks(&Engine, &[LabCheck])`：`&Engine` 编译期保证不能推进；主动项返回 `skipped=true`。
- `run_active_checks(&mut Engine, &[LabCheck])`：一次 `run_to_quiescence(45s)`，随后执行**全部**检查（含收敛后的状态项），不让每项重复推进。
- 两者共用 `eval_read_only` + `eval_probe` + `run_one_value`，没有第二套判断语义。
- UI：含主动项的实验，纯读全绿也**不会**写 `app.labPassed`；负向 Ping / TCP / 登录同样是主动行为并标来源。

---

## 5. 5 个代表实验逐项验收映射（真实 WASM · 真实 CLI 命令 · 真实检查 API）

浏览器脚本经 `window.__netemu.sim` 驱动真实编译出的 WASM；5 个实验共 **122 条真实 CLI 配置命令，`commandFailures = 0`**。

| 实验 | 命令数 | 纯读项 / 主动项 | 纯读前后 | 主动判分 | 关键断言 |
|---|---|---|---|---|---|
| `office-l2` | 24 | 2 纯读 Trunk / 2 主动 Ping | `dump`、`stats` 完全不变（events 4451，virtualNs 1.44e12） | **4/4** | 同 VLAN 正向 Ping 通；跨 VLAN `expect:false` 不通；SW1/SW2 Trunk 放行 VLAN10 |
| `office-l3` | 15 | 2 纯读 SVI / 1 主动 Ping | 不变（events 1382） | **3/3** | Comware `vlan-interface 10/20` 的 SVI 地址；PC1 → 192.168.20.12 跨 VLAN 通 |
| `enterprise` | 16 | 2 纯读（DHCP 地址、外网直连路由）/ 1 主动 Ping | 不变（events 4，virtualNs 250176） | **3/3** | PC1 = 192.168.1.2；R1 有 202.100.1.0/24；配置 `ip nat inside/outside` 后正向 Ping 通（标签不再宣称「NAT 生效」） |
| `stp-redundancy` | 19 | 2 纯读 STP 状态 / 1 主动 Ping | 不变（events 3598） | **3/3** | SW1 根桥；SW2 根端口 `Root / Alternate`；PC1 → PC2 经冗余路径通 |
| `bgp-route-policy` | 48 | 5 纯读路由 / 2 主动 Ping | 不变（events 2561） | **7/7** | R2/R3 过滤 172.16.9.0/24；R2/R3/R1 学到允许前缀；PC2 → 192.168.1.10 通、→ 172.16.9.10 不通 |

### 5.1 STP 纯读 / 主动收敛差异（未经 CLI 预推进的拓扑）

| 路径 | 引擎统计 | `SW2 根端口收敛` |
|---|---|---|
| 纯读 | `events 0 → 0`，`virtualNs 0 → 0`，`dump` 不变 | `false`，`Designated / Designated`（未收敛） |
| 主动 | `events 0 → 187`，`virtualNs 0 → 44,000,010,424`（≈44s） | `true`，`Root / Alternate`（已收敛） |

这是「纯读不主动收敛、主动判分先按既有 45s 语义收敛」的直接证据。

### 5.2 修改配置后重新判分（BGP）

`undo peer 10.0.0.1 route-policy FILTER import`（`Executed`）后重新调用两个真实入口：

- `R2 / R3 过滤 172.16.9.0/24`：`passed true → false`，detail 由「不存在」变「存在」；
- `PC2 → 受控前缀不可达`：`passed true → false`，detail 由「ping 不通」变「ping 连通」；
- 允许前缀与回程路由保持 `true`。

结果来自当前引擎状态，未使用缓存假结果。

---

## 6. 设备会话级命令检查（真实 WASM 接线）

| 验证点 | 真实结果 |
|---|---|
| `session_id` + `verify_device_command` | `dev0-s3`；detail 含 `profile=vrp-generic-router mode=exec authenticated=true remote=false`；`status=Executed`，`support=pending-verification` |
| 纯读性 | `dump` 不变；`stats` 前后均 `{virtualNs:0, events:0, inFlight:0, dropped:0}` |
| 错误令牌 | 抛 `session id mismatch`（会话错误，**不是**「设备不支持」结构化结果） |
| 当前会话模式 | Exec 下 `sysname R1` → `invalid-command`；执行 `system-view` 后 → `mode=config` 且不再是命令错误 |
| 加载 / 重置后失效 | `load_with_report` 后旧令牌报 `no active device session`；重新签发 `dev0-s4`（序号单调，旧令牌不复活）；`reset()` 后同样失效（Rust 契约测试覆盖） |

Rust 侧另覆盖 `inspect_command` 的 Profile 差异：同一 `switchport access vlan 10` 在
`ios-xe-17-router` 为 `unsupported-by-device`，在 `ios-xe-17-l3-switch` 不是；Comware 不因旧解析器可识别而 `full-support`。

---

## 7. 浏览器关键流程与截图边界

通过 `playwright-cli` + `http://localhost:4173/`（Vite preview，真实 `net_sim_bg.wasm`）验证。
浏览器验收所用 WASM 与最终门禁的 WASM 来自**同一份 Rust 源码**（两者之间只有 `cargo fmt` 的空白差异）；
最终门禁又重新执行 `wasm-pack build` 并由 `vite build` 消费，见 §8.1。

1. **课程表**：首屏 21 个单元格（vlan 课程 7 行 × 3），切 basics 课程后 45；`ping 10.0.0.2` 生成三个互不串用的 key（`ios-xe-17-router` / `vrp-generic-router` / `comware-generic-router`）；语言切换前后 key 集合完全一致；28 条 `pending-verification`，Comware `full-support = 0`；页面可见「平台级结果」「待验证」及 Comware 阶段 4 说明。
2. **会话级命令检查**：见 §6，状态 / 时间不变。
3. **office-l2 / office-l3**：纯读跳过 Ping 且统计不变，主动判分正 / 负 Ping 正确。
4. **STP**：见 §5.1，纯读与主动差异可见、可解释。
5. **BGP Route-Policy**：见 §5 / §5.2，过滤与正 / 负 Ping 端到端正确，重判分反映新状态。
6. **双通道**：`exec_command_result` 的 `deviceOutput` 无 simulator/netemu 措辞、`simulatorNotices` 独立 1 条；旧 `exec_command` 同时含设备文本与 `[netemu]` 提示；`TerminalPanel` 各自单一渲染未改动。
7. **UI 主动路径**：真实点击实验一 → 24 条真实 CLI 命令 → 「▶ 运行实验判分」，页面显示行为清单（推进协议收敛 / 发送 ICMP Ping 探测 / 含主动项不因纯读全绿完成）、两个按钮、4/4 结果与 `主动实验` / `纯读` 来源标签。
8. **控制台**：`Total messages: 0 (Errors: 0, Warnings: 0)`。

**截图**（`docs/cli-fidelity/`）：

- `stage3a-course.png`：课程表平台级结构化状态（完整支持 / 待验证 / 证据 / Comware 说明）。
- `stage3a-lab-active.png`：实验一主动判分页面（行为清单、双按钮、4/4 逐项结果与来源）。

**截图边界（如实说明）**：两张均为真实流程抓屏，但课程表需滚动 `.reader`、判分结果需滚动 `.left-panel` 才完整可见；这是**单页流程证据，不是全量视觉回归**，未逐课程、逐终端窗口或逐语言抓屏。实质验证由页内真实 WASM API 的结构化结果完成。

---

## 8. 最终六项门禁真实结果

| 门禁 | 退出码 | 结果 |
|---|---|---|
| `cargo fmt --all -- --check` | **0** | 先 `cargo fmt --all` 统一新增文件后复跑 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | **0** | 无 `#[allow]` 规避 |
| `cargo test --workspace` | **0** | **265 passed / 0 failed / 0 ignored**（基线 250，净增 15）；45 个既有测试二进制全绿 |
| `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | **0** | 见 §8.1 |
| `npx --yes pnpm@10 --dir web run check` | **0** | `tsc --noEmit` |
| `npx --yes pnpm@10 --dir web run build` | **0** | Vite 消费重新生成的 `net_sim_bg-B-glHd3P.wasm`（1,545.51 kB，gzip 541.11 kB） |

### 8.1 WASM 总门禁逐段

- **Rust→WASM 编译**：✅ release profile，基于当前源码（24.22s）。
- **wasm-bindgen 声明**：✅ `net_sim.js` / `net_sim.d.ts` / `net_sim_bg.wasm.d.ts` / `package.json`；`net_sim.d.ts` 含 `read_only_lab_checks` / `run_lab_checks` / `inspect_command` / `verify_device_command` / `session_id`。
- **wasm-opt**：✅ 本环境**未复现**交接文档的 `拒绝访问 (os error 5)`；未关闭优化、未改配置换绿。
- **WASM 总门禁**：✅ 退出 0。
- **Web check / build**：✅ / ✅，消费上述真实 WASM（非声明桩）。

### 8.2 测试数量汇总

| 层 | 数量 / 结果 |
|---|---|
| Rust 单元 + 集成 + doctest | 265 passed / 0 failed（新增 15：5 `stage3a_cmdmap` + 10 `stage3a_tests`） |
| WASM 契约（`sim-wasm --lib`） | 16 passed / 0 failed（6 既有 + 10 新增） |
| 浏览器真实流程 | 5 个实验 × 2 路径 + STP 差异 + BGP 重判分 + 课程表 + 会话级 + 双通道 + UI 主动路径；122 条实验 CLI 命令全部 `Executed`；0 控制台消息 |
| Web | `tsc --noEmit` 0 错误；Vite build 0 错误 |

---

## 9. HEAD、分支、工作区、未完成项与阻塞

| 项 | 状态 |
|---|---|
| HEAD | `3d784e022c8e70ca3c4aafbce65edd1220064ed7` |
| 分支 | **detached HEAD（no branch）** |
| 提交 / 推送 | **未提交、未推送** |
| 工作区 | 24 个已跟踪 M + 15 组未跟踪（含本包新增 `lab_defs.rs`、`stage3a_tests.rs`、`stage3a_cmdmap.rs`、`stage3a-implementation-record.md`、`stage3a-course.png`、`stage3a-lab-active.png`）；起点改动全部保留 |
| 构建目录 | 独立 `CARGO_TARGET_DIR` |
| 临时文件 | `web/src/lib/_stage3a_runlab.txt` 用完即删；`web/dist/stage3a-verify.js` 为浏览器验收临时脚本，已被最终 `vite build` 清空 dist 删除；无诊断残留 |

**未完成项（明确后置，未开始）**

- C6b 完整跨设备 SSH / 远程 VTY 认证：`LoginAttempt` 只做了主动行为分类与测试，`mgmt-ssh` **不作为 C6b 完成证据**。
- Comware 阶段 4 拆树与 strict 覆盖：对照表 Comware 列一律不标完整支持，未补 Comware 功能。
- 全量课程整改、防火墙 / IOS ZBF、F5 TMOS、`run()` 前端事件动画、阶段 3B。
- 厂商逐字输出黄金：没有带版本设备 / eNSP 样本，保持「输出待验证」；**本包未新增逐字厂商黄金，未升级证据等级**。

**阻塞**：无功能阻塞。环境事项：共享 target 指纹权限冲突（已用独立目录规避）；preview 服务需后台常驻；交接文档的 `wasm-opt os error 5` 本次未复现（如实记录，不以 Web 成功掩盖该项）。

---

## 10. 停止点

**5 个代表实验与关键浏览器流程完成后停止**：对照表 Profile 级结构化 inspection、纯读 / 主动判分契约与双路径 UI、
5 个实验端到端验收、15 个新增 Rust/WASM 测试、六项门禁全绿、真实 WASM 与两张流程截图、本实施记录已交付。
**未进入 C6b / 阶段 3B / Comware / F5 / 防火墙扩展 / `run()` 前端动画。**
