# 合并记录：公开仓库新工作（SRv6 等 5 提交）并入内核主线

- 日期：2026-11-17
- 仓库：`e:\网络命令\netemu`，分支 `main`
- 合并对象：`upstream/main`（`58e22e9`，公开仓库 `github.com/halacrcrc/netemu`）→ `main`（`16f10a7`，私有仓库 `origin`）
- 合并基线：`3d784e0`
- 方式：`git merge upstream/main`（普通合并提交，不 rebase；两侧提交已各自公开）
- 合并提交：`fdcfbe4`，父提交 `16f10a7`（我方）+ `58e22e9`（对方）
- 本记录随该合并提交一并入库

---

## 1. 结论

| 项 | 结果 |
|---|---|
| 冲突 | 9 文件 14 块，全部解决；冲突标记残留 0（`git grep -E '^(<<<<<<<|=======|>>>>>>>)'` 空） |
| 双方功能存活 | 我方 ZBF / 防火墙接地 / 规格驱动 / 双通道 / 课程原文测试 × 对方 SRv6 Policy / EVPN TTL / ping6 会话泄漏修复 / UI 打磨，全部在 |
| 测试 | **355 passed / 0 failed / 53 个测试二进制**（基线 349 / 52，差值见 §4） |
| 门禁 | 本地 clippy / test / wasm / tsc / vite build 全绿；fmt 差异已由 `4ac608a` 收口（§4.3、§9），CI 全绿（run 44055929810） |
| 浏览器 | 真实 Chrome + 本次构建，Lab 6 全流程 4/4、讲义 08 ZBF 简流程会话 1 条、实验二十二 SRv6 Policy 子视图可进；控制台 0 错误 |
| 推送 | `git push origin main` 已成功（`16f10a7..490c0bf`）；直连 443 被阻断，经用户提供的本地代理 `127.0.0.1:10808` 完成，过程见 §8.1（**不推 `upstream`**） |

---

## 2. 冲突块逐文件处置表（9 文件 14 块）

| # | 文件 | 块 | 处置 | 说明 |
|---|---|---|---|---|
| 1 | `crates/cli-engine/src/lib.rs` | 1 | 合写 | `SubCtx` 两侧字段全保留：我方 `ospf` / `vlan` / `cmap` / `pmap` / `pmap_class` + 对方 `srv6_policy`。 |
| 2 | `crates/lab-checks/src/lib.rs` | 1 | 合写 | 对方 `CheckKind::Srv6Policy` 判分臂并入我方**纯读通道** `eval_read_only`（与 `CheckKind::behavior()` 默认 `ReadOnly` 一致）；我方 `Ping6 { .. } => return None`（主动通道）与 `SviIp` 臂保留。上游单体 `run_one` 的判分逻辑无损搬入，断言未改写。 |
| 3 | `crates/cli-dialects/src/ios.rs` | 1 | 合写 | `quit` 层级：`Vlan` / `ClassMap` / `Srv6Policy` / `PolicyMap` → `Config`，`PolicyMapClass` → `PolicyMap`（ZBF 两级回退与 SRv6 视图共存）。 |
| 4 | `crates/cli-dialects/src/huawei.rs` | 1 | 合写 | `quit` 层级：`Vlan` / `ClassMap` / `PolicyMap` / `Srv6Policy` / `PolicyMapClass` → `Config`。 |
| 5 | `crates/cli-dialects/src/huawei.rs` | 2 | 合写 | 用户视图 `exec` 树：我方 `reset firewall session table` 与对方 `quit` 并存。 |
| 6 | `crates/cli-dialects/src/lib.rs` | 1 | 合写 | `Trees` 字段全保留：`vlan` / `tmos_ltm` / `tmos_sys` / `class_map` / `policy_map` / `policy_map_class` + `srv6_policy`。 |
| 7 | `crates/cli-dialects/src/lib.rs` | 2 | 合写 | `root_for` 覆盖全部模式（含 `Mode::Srv6Policy`）。 |
| 8 | `crates/cli-dialects/src/lib.rs` | 3 | 合写 | IOS 提示符：`Vlan` 兜底、`tmsh` 兜底 + `(config-srv6-policy)#`。 |
| 9 | `crates/cli-dialects/src/lib.rs` | 4 | 合写 | VRP/Comware 提示符：`[name-vlanN]` + `[name-srv6-policy-X]` + ZBF 三视图兜底 + tmsh 分支（`Dialect::Tmos`）整体保留。 |
| 10 | `web/src/components/CourseView.svelte` | 1 | 合写 | import 合并：我方 `inspectCommand` / `InspectionResult` + 对方 `setViewName`；`app` 已无引用，导入一并去掉（避免未使用导入）。 |
| 11 | `web/src/components/CourseView.svelte` | 2 | 取对方 | 跳转改为 `setViewName('lab')` + `setTimeout` 派发。原因：我方原文 `app.view = 'lab'` 指向**已改名的 store 字段**（现为 `app.viewName`），即对方本提交修复的「课程页跳转无效」；对方写法覆盖更强且保留了「切到实验视图并打开该实验」的我方意图。 |
| 12 | `web/src/components/LeftPanel.svelte` | 1 | 合写 | 保留我方 `left.checkModeHint`（判分不缓存提示），同时采用对方去掉 `🌐` emoji 的方言切换按钮。 |
| 13 | `web/src/lib/labs.ts` | 1 | 取我方架构 + 保留对方实验内容 | 我方已把判分改为 WASM 批量两条路径（`read_only_lab_checks` / `run_lab_checks`），上游此块仍是**单体 `runLabChecks` 逐 kind 分派**。保留我方实现并删除该单体函数（否则同名重复定义）；对方本块真正的功能是**实验二十二菱形双路径 + `srv6Policy` 检查项 + 5 条判分**，这些全部保留（`LabCheckDef` 新 kind、`lab22` 的 build/checks/tasks 逐字保留）。分类一致性：`checkBehavior` 对 `srv6Policy` 落到默认 `readOnly`，与 Rust `CheckKind::behavior()` 默认值一致（WASM 侧行为分类校验通过）。 |
| 14 | `web/src/content/labs-en.ts` | 1 | 合写 + 1 处去重 | 保留我方 Lab 19（TMOS 17.x / tmsh 严格序列）文案；并入对方新增的 `ospfv3` / `stack-irf` / `port-isolation` / `srv6-be` 四段 EN；**删除对方同名旧 `'f5-ltm'` 条目**——同一 key 不允许两份，且其内容引用 `ltm pool WEB member` 等**已被我方阶段 5A 移除的抽象 ltm 命令**，保留会给出不可执行的英文讲解。这是本包唯一的单边取舍，两侧意图（我方 TMOS 保真 / 对方补齐 EN 覆盖）在结果中都成立。 |

### 2.1 解冲突后为编译/ID 一致性补齐的两处（非单边丢弃）

| 文件 | 补充 | 理由 |
|---|---|---|
| `crates/cli-engine/src/lib.rs` | `Mode::tag()` 增 `Mode::Srv6Policy => "srv6_policy"` | 上游没有 `Mode::tag()`（该函数是我方规格驱动的稳定 ID 来源）；不补则 `match` 非穷尽无法编译，且规格 ID 会缺一模式。 |
| `crates/cli-dialects/src/comware.rs`、`tmos.rs` | `srv6_policy: vec![]`（各带注释） | `Trees` 新增字段后必须初始化。Comware 树本身没有 `srv6 policy` 进入命令（现状，不属本包新增功能），TMOS 没有 SRv6，故均为空视图。 |

---

## 3. 语义核对（自动合并 ≠ 语义正确）

| 文件 | 核对内容 | 结果 |
|---|---|---|
| `crates/sim-core/src/engine.rs` | 对方：EVPN 外层 TTL 递减、SRv6 Policy 封装（`segments_left` 队列语义、End 逐段改目的、End.DT6 解封装）、ping6 无路由时标记会话完成（修泄漏）、OSPFv3/OSPF `needless_range_loop` 改迭代器。我方：防火墙接地/ZBF 判定与推进链路。 | 双方均在；我方段落未被改写（`git diff HEAD` 仅出现对方 hunk）。 |
| `crates/sim-core/src/device.rs` | `Srv6Route` 增加 `policy: Option<String>`（`#[serde(default)]`），去 `Copy` 保留 `Clone`；新增 `Srv6Policy{name,color,endpoint,segments}`、`Srv6Config.policies`。 | 在；序列化默认值保证旧存档可读。 |
| `crates/sim-core/src/summary.rs` | `srv6_policies` 摘要 + 回灌；`Srv6RouteSummary.policy`。 | 在（dump/restore 双向都补了）。 |
| `crates/sim-wasm/src/lib.rs` | `check_srv6_policy(dev,name,endpoint)` 走 `lab_checks` 同一批入口。 | 在。 |
| `crates/cli-dialects/src/config_render.rs` | 三方言渲染：对方接口名修复改用 `ifname::canonical(dialect, d.kind, p)`（不再硬编码 `GigabitEthernet0/0/x`）、新增 VRF / MPLS LSP / route-policy / IPsec / WLAN / SRv6 Policy 渲染段落；我方 `security_lines()` 分叉与口令脱敏不变。 | 在；但发现并修掉一处**合并引入的重复**（见 §3.1）。 |
| `web/src/lib/i18n-dict.ts` | 对方去 emoji（连接/抓包/导入导出/备份等）+ 文案勘误；我方 `left.checkReadOnlyBtn` / `left.checkActiveBtn` / `left.checkModeHint` / `left.behavior.*` / `left.src.*` 全在。 | 两侧共存，键无覆盖。 |
| `web/src/components/TerminalPanel.svelte` | 对方空态改为绝对定位 `.term-empty`；我方标签页/关闭/焦点逻辑不变。 | 在，无回归。 |

### 3.1 语义核对中修掉的一处合并缺陷

`config_render.rs` 双方**各自独立**加过同一句 IOS `enable secret ***` 渲染（我方在 `mgmt` 段，对方在其新增块里），git 文本合并成两段并存 →
`show running-config` 会**打印两遍** `enable secret ***`。
处置：删除对方新增的那一段，保留我方 `mgmt` 段落内的那一处（位置正确、与 `username ... secret ***` 相邻）。
现有测试没有覆盖「同一口令行只出现一次」，因此门禁全绿也不代表该处正确——记在此处备查。

---

## 4. 门禁（真实数字）

### 4.1 结果

| # | 门禁 | 命令 | 结果 |
|---|---|---|---|
| 1 | clippy | `cargo clippy --workspace --all-targets -j 1 -- -D warnings` | ✅ **0 warning / 0 error**（无需 `#[allow]`；对方 CI 新版规则下的 `needless_range_loop` / `redundant pattern matching` 已在对方提交中修好，合并后直接沿用） |
| 2 | 测试 | `cargo test --workspace -j 1` | ✅ **355 passed / 0 failed / 0 ignored，53 个测试二进制** |
| 2b | 课程表 | `cargo test -p cli-dialects --test stage3a_cmdmap -- --nocapture` | ✅ `TOTAL_ROWS=449 REJECTED=0`（零拒绝不放宽） |
| 3 | WASM | `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | ✅ 成功；`wasm-opt` 正常执行未关闭；产物 `net_sim_bg-BENAVF5J.wasm` 1,765,848 B |
| 4 | 类型检查 | `npx --yes pnpm@10 --dir web run check`（`tsc --noEmit`） | ✅ 0 错误 |
| 5 | 生产构建 | `npx --yes pnpm@10 --dir web run build` | ✅ `index-BLsvm_wc.js` 913,426 B / `index-DsnzkBrK.css` 118,515 B；PWA precache 11 entries |
| 6 | fmt | 逐文件 `rustfmt --edition 2021 --check` | 12 个改动的 `.rs` 全部通过；`ios.rs` 与 `sim-wasm/src/c6b_tests.rs` 存在差异，**合并前即存在**（见 §4.3） |

### 4.2 测试数差值解释（349 → 355）

- 基线（我方 `16f10a7`）：**349 passed / 52 二进制**。
- 合并后：**355 passed / 53 二进制**，差值 **+6 / +1 二进制**。
- 来源唯一：对方新增集成测试文件 `crates/cli-dialects/tests/srv6_policy_golden.rs`，内含 **6 个 `#[test]`**：
  `host_gw_ping_regression`、`srv6_policy_configured`、`underlay_reachable`、`end_to_end_via_policy`、`plain_ipv6_regression`、`srv6_be_still_works`。
- 逐条核验：`git diff 3d784e0 upstream/main -- '*/tests/*'` 除该新文件外**没有**任何新增 `#[test]`；我方 349 项一条未删、一行断言未改写（无「去重」损耗）。
- 结论：`349 + 6 = 355`，**差值完全解释**。

### 4.3 fmt：`ios.rs` 挂起属已知环境项（如实登记）

- `cargo fmt` 处理 `cli-dialects/src/lib.rs` 时会连带格式化子模块 `ios.rs` 并挂起，属既有环境事项；本包改用**逐文件** `rustfmt --check` 替代。
- 逐文件结果：`cli-engine/src/lib.rs`、`lab-checks/src/lib.rs`、`cli-dialects/{lib,huawei,comware,tmos,config_render}.rs`、`sim-core/{engine,device,summary}.rs`、`sim-packets/src/lib.rs`、`sim-wasm/src/lib.rs`、`tests/srv6_policy_golden.rs` 全部通过。
- 剩余差异（`ios.rs:966` 附近 ZBF `class-map` 代码块、`sim-wasm/src/c6b_tests.rs:386` 的 `en_count` 断行）在**合并前就在**：把 `16f10a7` 的同名文件单独 `rustfmt --check`，差异位置与内容一致 → 属我方既有未格式化，不是本次合并引入。
- 当时判为"不改写以避免无关行 diff"，**事后证明是错的**：这批差异在 CI 里表现为 `格式检查` 一步失败，后续 Clippy/测试/WASM/前端构建全部 skipped（run 44007744327，且合并前 `16f10a7` 的 CI 同样红）。
- 已单独收口（2026-11-17，见 §9）：CI 实测共 **26 处 / 5 文件**（`ios.rs` 4、`lib.rs` 8、`tests/c6b_courses.rs` 4、`tests/c6b_mgmt_grounding.rs` 8、`sim-wasm/src/c6b_tests.rs` 2），由一次性工作流在 CI 同版本 rustfmt 下 `cargo fmt --all` 修掉，commit `4ac608a`。
- 新增代码（§2 合写块、`Srv6Policy` 臂、`Trees` 字段）已随上述逐文件检查确认为 fmt 合规。

---

## 5. 内核不变式 grep 自查

| 不变式 | 自查口径 | 合并前（`16f10a7`） | 合并后 | 结论 |
|---|---|---|---|---|
| `if dialect` 零新增 | `git grep -c 'if dialect' -- 'crates/**/*.rs'` | 32 | 32 | 持平 |
| 方言分支不外溢 | `git grep -c 'dialect ==' -- 'crates/**/*.rs'` | 36 | 36 | 持平 |
| 四态命令模型 | `CommandStatus` 变体 | 4：`Executed` / `UnsupportedByDevice` / `NotSimulated` / `InvalidCommand` | 同 4 | 未新增第五态 |
| 双通道分离 | `deviceOutput`（设备真实文本） / `simulatorNotices`（平台提示） | 分离 | 分离 | 对方 SRv6 与审计修复未把提示塞进设备输出 |
| 单一状态源 | `sim-core` 为唯一引擎状态；`sim-wasm` 只转 JSON | — | — | SRv6 Policy 状态存 `Srv6Config.policies`，未在 `cli-dialects` 另建影子结构 |
| 防火墙单一判定入口 | `firewall_action(...)` 为唯一判定；`firewall_allow` / `firewall_tcp_allow` 只是取 in/out port 的薄封装 | 入口 1 个 + 2 薄封装，调用点 3 处 | 同 | 对方未在数据面加第二套策略判定 |

> 过程记录：合并后曾出现 `if dialect` 33 / `dialect ==` 37（+1），定位为 §3.1 的 `config_render` 重复 `enable secret` 段；删除该重复段后回到 32 / 36。

---

## 6. 浏览器冒烟（真实 Chrome + 本次构建）

- 服务：`vite preview` 服务本次 `web/dist`（`http://localhost:4321`），地址带 `?v=merge…` 查询参数；全新浏览器 profile（无历史 service worker / cache，等效清 PWA 缓存）。
- 构建产物：`net_sim_bg-BENAVF5J.wasm` + `index-BLsvm_wc.js`（与本记录 §4.1 同一批产物）。

### 6.1 Lab 6 全流程（`docs/cli-fidelity/merge-lab6-grading.png`）

| 项 | 结果 |
|---|---|
| 拓扑 | `PC1` / `R1`（`mgmt-ssh` 的 `build`），设备 2 / 链路 1 |
| R1 配置（键盘逐条） | 11 条，提示符逐条精确：`R1#` → `R1(config)#` → `R1(config-if)#`×3 → `R1(config)#` → `username` → `R1(config-line)#`×3 → `R1(config)#` |
| PC1 交互登录 | `ssh 192.168.1.1` → `Username:` → `admin` → `Password:` → `Cisco@123` → **`R1#`** |
| 远端 show | `show ip interface brief` 含 `GigabitEthernet0/0` / `192.168.1.1` / `up up` |
| 断开 | `exit` → `[Connection closed by foreign host]` → `PC1>` |
| Telnet 负向 | `telnet 192.168.1.1` → `connect: Connection refused` → `PC1>` |
| 主动判分 | **4/4 全绿**：本地用户 admin（纯读）、VTY 仅 SSH（纯读）、SSH 登录成功（主动）、Telnet 被拒（主动，`Some(TransportDenied)`） |

### 6.2 讲义 08 ZBF 简流程（`docs/cli-fidelity/merge-zbf-session1.png`）

实验十拓扑（PC1 / FW1 / SRV，设备 3 / 链路 2），FW1 方言切到思科 IOS 后：

| 步骤 | 结果 |
|---|---|
| 接口 | `interface GigabitEthernet0/0`（192.168.1.1/24）、`GigabitEthernet0/1`（203.0.113.1/24），提示符 `FW1(config-if)#` |
| 区域 | `zone security INSIDE` / `OUTSIDE`；接口下 `zone-member security INSIDE` / `OUTSIDE` |
| 对象与策略 | `class-map type inspect match-any ICMP-CLASS` → `match protocol icmp`；`policy-map type inspect PMAP` → `class type inspect ICMP-CLASS` → `inspect`，`class class-default` → `drop` |
| 提示符链 | `FW1(config-cmap)#` → `FW1(config-pmap)#` → `FW1(config-pmap-c)#` → `FW1(config-sec-zone-pair)#`（与官方逐级一致） |
| 挂接 | `zone-pair security IN2OUT source INSIDE destination OUTSIDE` → `service-policy type inspect PMAP` |
| 正向 ping | PC1 `ping 203.0.113.10`：5 transmitted / 5 received / 0% loss |
| 会话 | `show policy-map type inspect zone-pair`：`ICMP-CLASS ... inspect 1 packets`、`class-default drop 0 packets`；`... zone-pair session`：**`sessions: 1 icmp, 0 tcp`** |

### 6.3 实验二十二（SRv6 Policy 合并点抽查）

- 菱形拓扑建成 6 设备：`PC1 / PE1 / P1 / P2 / PE2 / PC2`（`build` 中 `add_link` 六条与上游一致）。
- PE1 交互面：`system-view` → `srv6 policy POL1 color 100 end-point 2001:db8:200::1` → 提示符 **`[PE1-srv6-policy-POL1]`**（我方 `Trees.srv6_policy` 字段 + 对方命令 + 我方 VRP 提示符三者衔接正确）→ `segment-list 2001:db8:400::2 2001:db8:200::1` 执行成功；`display srv6` 正常返回。
- 端到端数据面（Policy 引导 → P2 End → PE2 End.DT6、PC1→PC2 通）由 `srv6_policy_golden::end_to_end_via_policy` 与 `srv6_be_still_works` 在真实引擎上覆盖（§4.2）。

### 6.4 控制台

- `console error`：**Total messages: 0（Errors: 0, Warnings: 0）**（Lab 6 与 ZBF/SRv6 两个会话分别复核）。

---

## 7. 临时产物 GONE 核验（按真实文件系统）

| 产物 | 用途 | 核验 |
|---|---|---|
| `E:\netemu-target-merge\` | 独立 `CARGO_TARGET_DIR`（全程 `-j 1`，无并发挂起） | `Test-Path` → `False` |
| `E:\fmtcheck\` | fmt 归因用的 HEAD 版本临时副本 | `False` |
| `E:\netemu-test-after.txt` | 测试计数日志 | `False` |
| `E:\netemu-preview.log` / `.err` | 预览服务日志 | 均 `False` |
| `.playwright-cli\` + `*.yaml` 快照 | 浏览器冒烟快照 | `False`（根目录 `cur.yaml` / `lab6.yaml` / `lab10.yaml` / `s1.yaml` 已删） |
| `vite preview` 进程（:4321） | 冒烟服务 | 进程已终止；`Invoke-WebRequest` 连接失败＝已停 |
| 保留 | `docs/cli-fidelity/merge-lab6-grading.png`、`merge-zbf-session1.png`、本记录 | 已入库 |
| 忽略目录 | `web/src/wasm/`、`web/dist/` | 被 `.gitignore` 覆盖，未入库 |

---

## 8. 工作区计数与推送

- 合并改动：**29 个文件变更**（+1148 / −129），其中新增文件 1 个（`crates/cli-dialects/tests/srv6_policy_golden.rs`），本记录与 2 张截图为后续新增。
- 合并后 `git status`：无 `UU`/未解决冲突（`git diff --check` 空、冲突标记 grep 空）。
- 分支历史：`main` 保留两侧提交 + 一个合并提交 `fdcfbe4`（父 `16f10a7` + `58e22e9`；不 rebase、不改写任一侧提交）。
- 推送：`git push origin main` → **成功**，`16f10a7..490c0bf  main -> main`；远端 `refs/heads/main = 490c0bf` 已回读核对，`git status -sb` 显示 `## main...origin/main`（无 ahead/behind）。
- **明确未做**：未推送 `upstream`；未新增功能；未删测、未放宽断言；未改写给定提交。

### 8.1 推送过程（直连失败 → 本地代理成功）

| 顺序 | 命令 | 结果 |
|---|---|---|
| 1 | `git push origin main` | `OpenSSL SSL_read: unexpected eof while reading` |
| 2 | `git push origin main` | `Recv failure: Connection was reset` |
| 3 | `git -c http.version=HTTP/1.1 push origin main` | `Failed to connect to github.com:443 after 21039 ms` |
| 4~6 | 间隔 8s / 10s 直连重试、`git ls-remote origin` | 同「connect 超时」 |
| 7 | `curl -x http://127.0.0.1:10808 https://github.com/` / `--socks5-hostname` | 均 `200`（代理可用） |
| 8 | `git -c http.proxy=http://127.0.0.1:10808 -c https.proxy=http://127.0.0.1:10808 push origin main` | ✅ `16f10a7..490c0bf  main -> main` |

归因：直连 `github.com:443`（解析 `20.6.7.17`）握手超时，属本机网络出口限制，与仓库凭据/远端状态无关；用户提供本地代理 `127.0.0.1:10808` 后一次成功。
代理只以**单命令 `-c` 参数**传入，未写入 `.gitconfig`、未改动任何 git 配置。
后续若需再次推送且直连仍不通：`git -c http.proxy=http://127.0.0.1:10808 push origin main`（**不要** `git push upstream`）。

---

## 9. 合并后收口：CI 变红与部署拓扑调整（2026-11-17）

### 9.1 CI 为什么红（两个独立问题，都不是本次合并引入）

| 现象 | 证据 | 归因 |
|---|---|---|
| `CI` 卡在 `格式检查`，后续 Clippy/测试/WASM/前端构建全部 skipped | run `44007744327`（`bd6e71c`）；合并前 `16f10a7` 的 run `43998695817` 同样卡在此步 | 我方既有未格式化代码，§4.3 已登记但当时未修 |
| `Deploy to GitHub Pages` 的 `deploy` job 404 | `Failed to create deployment (status: 404) … Ensure GitHub Pages has been enabled`；合并前 run `43998695895` 同样 404 | 私有仓库 `netemu-cli-fidelity` 从未启用 Pages（`GET /repos/{r}/pages` → 404）；上游公开仓库的 Pages 一直正常，`gh run list` 默认指向它，所以"部署成功"的记录其实来自另一个仓库 |

### 9.2 fmt 收口（26 处 / 5 文件）

本机 `rustfmt 1.10.0-stable` 对 `cli-dialects/src/ios.rs`、`lib.rs` 会**挂死**（45 秒超时后 kill；纯 ASCII 路径 `E:\fmtwork` 复现，非路径编码问题；同一 rustfmt 对 `tiny.rs` 与其余 3 个测试文件正常），无法本地产生与 CI 一致的结果。
处置：临时加 `workflow_dispatch` 工作流 `.github/workflows/fmt-fix.yml`，在 CI 同版本 rustfmt 下 `cargo fmt --all` 并提交 → `4ac608a`（5 文件 +113/−76，逐条抽查为纯换行/折行）→ 删工作流 `1360686`。

格式化后本地复跑（`CARGO_TARGET_DIR=E:\netemu-target-inline`）：clippy **0 warning**；**355 passed / 0 failed / 53 二进制**（与格式化前一致，确认无语义影响）。

### 9.3 部署拓扑调整（用户决策：旧仓下线、新仓上台）

| 仓库 | 调整前 | 调整后 | 实际执行 |
|---|---|---|---|
| `halacrcrc/netemu` | public，Pages 启用（`https://halacrcrc.github.io/netemu/`） | **private**，Pages 删除，`Deploy to GitHub Pages` 工作流 `disabled_manually`（CI 保留） | `gh workflow disable` → `DELETE /repos/{r}/pages` → `PATCH private=true` |
| `halacrcrc/netemu-cli-fidelity` | private，Pages 未启用 | **public**，Pages 启用（`build_type: workflow`，`https://halacrcrc.github.io/netemu-cli-fidelity/`） | `PATCH private=false` → `POST /repos/{r}/pages -F build_type=workflow` |

转公开前做过敏感信息扫描：tracked 文件中无 `ghp_`/`gho_`/`AKIA`/`sk-`/私钥块命中；历史里 `ghp_` 仅出现在笔记页的占位文案 `ghp_…（经典 PAT，gist 权限）`，非真实令牌；无 `.env`/证书类文件入库。

### 9.4 收口后的真实结果

| 项 | 结果 |
|---|---|
| CI（`1360686`，run `44055929810`） | ✅ 格式检查 → Clippy → 单元与场景测试 → 构建 WASM → 前端构建 全通过 |
| Pages 部署（run `44055929829`） | ✅ `build` 1m21s、`deploy` 10s 均成功 |
| 线上站点 | `curl` → **200**，`<title>网工模拟器 · 网络工程师学习练习场</title>`；资源 `index-D11s63pn.js` / `index-DsnzkBrK.css` 200 |
| 线上功能抽查（真实 Chrome） | 首页正常渲染（设备库/方言下拉）；实验六拓扑 2 设备；R1 终端 `enable` → `R1#`；`pageerror` 0。截图 `docs/cli-fidelity/pages-live-check.png` |
| 旧站点 `halacrcrc.github.io/netemu/` | **404**（已按决策下线） |

> 遗留观察（未处理，仅登记）：首次访问线上站点时偶发一次空渲染，重载后正常；同次 `net_sim_bg-*.wasm` 出现一条 `net::ERR_ABORTED`（`service worker` 预缓存与模块加载竞争的可能），功能抽查未受影响。
