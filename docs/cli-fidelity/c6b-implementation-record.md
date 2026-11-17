# C6b 实施记录：管理面登录的数据面接地与 Lab 6 课程原文验收

范围以 `netemu-c6b-implementation-handoff.md` 为准，技术约束以主方案 §4.4 / §10.1 / §10.4 / §11.1 / §15 为准。
本文只记录 C6b，不回写 `stage6a-*` / `stage6b-*` 历史记录。

---

## 0. 现场保护与基线

| 项 | 实测值 |
| --- | --- |
| HEAD | `3d784e022c8e70ca3c4aafbce65edd1220064ed7`（全程未变，detached HEAD） |
| 工作区改动 | 36 项已跟踪修改 + 64 项未跟踪文件（`-uall` 口径；默认折叠口径 64 行），未提交、未推送 |
| 目标目录 | `CARGO_TARGET_DIR=C:/Users/22534/.workbuddy/build/netemu-c6b-target`（与仓库 `target/` 隔离） |
| 并行度 | 全程 `-j 1`；本轮未出现并行编译挂起 |
| 基线复跑 | `cargo test --workspace -j 1`：332 项 / 50 个二进制全绿 |
| 课程表基线 | `stage3a_cmdmap` 打印 `TOTAL_ROWS=449 REJECTED=0` |

C6b 结束后计数：**349 项 / 52 个二进制**（+17 项，+2 个集成测试二进制），课程表仍 `449 / 0`。
新增 17 项 = `c6b_mgmt_grounding`(11) + `c6b_courses`(3) + `sim-wasm::c6b_tests`(3)。

未删除、未放宽任何既有断言；三方言的既有交互登录黄金测试只做**覆盖更强方向的改写**（见 §6）。

---

## 1. 决策 D1：登录数据面接地的范围与文案

### 1.1 负向矩阵（接地前实测）

用临时探针（`crates/cli-dialects/tests/zz_c6b_probe.rs`，**已删除**）在同一拓扑上跑三种探针：
`ping 192.168.1.1` / `pc tcp 192.168.1.1 22` / `ssh 192.168.1.1`。

C6b 之前（`login` 只查目标 IP 是否在某台设备上 + VTY 传输策略）：

| 故障场景 | `ping` | `tcp <IP> 22` | `ssh`（前） | 判定 |
| --- | --- | --- | --- | --- |
| 对端接口 `shutdown` | 不通 | SYN 无响应 | **仍进入 `Username:` 并可登录** | ❌ 缺陷 |
| 链路 down（`link.up=false`） | 不通 | SYN 无响应 | **仍可登录** | ❌ 缺陷 |
| 无路由（删 `ip route` / 网关不在表） | 不通 | SYN 无响应 | **仍可登录** | ❌ 缺陷 |
| 路径上防火墙 drop 转发流量 | 不通 | SYN 无响应 | **仍可登录** | ❌ 缺陷 |
| VTY `transport input ssh` 时 Telnet | — | RST | `Connection refused` | ✅ 既有正确 |
| 口令错误 / 无本地用户 | — | 可建连 | `Permission denied` | ✅ 既有正确 |

结论：管理面登录当时完全绕开数据面，「配置了用户就能登进去」——这与讲义 47 的教学结论自相矛盾，
也是 C6b 要修的唯一缺陷。

### 1.2 接地范围（决策）

**接入对端端口 / 链路 / 转发可达性，复用既有机制，不建第二套转发路径。**

`Engine::login_target()` 在进入认证前做一次 SYN 级别可达性判定，判定链**逐项复用数据面同一函数**：

| 判定维度 | 复用的既有机制 |
| --- | --- |
| 目标归属 | `Net::mgmt_ip_owner()`（端口/SVI 地址表） |
| 路由（含 VRF） | `Net::lookup_route_vrf()`（与 `tcp_forward` 同一张表） |
| 接口状态 | `Port::admin_up` + `link` 存在 + `Net::link_up()`（与 `transmit_frame` 的 `oper_up` 一致） |
| 对端端口管理性关闭 | `mgmt_link_peer()` + 对端 `admin_up`（与 `transmit_frame` 载波检测一致） |
| 接口 ACL | `acl_decide_probe()`（入/出方向同序，与 `tcp_forward` 同入口） |
| 防火墙 | `firewall_action()`（`ZonePairs` / `FlatRules` 唯一策略入口） |
| 服务端口 | `MgmtProto::port()`：SSH→22，Telnet→23 |

登录按**新流**判定：不查、不写既有会话表。理由是真实设备每次登录都是新 TCP 连接（客户端端口不同），
复用固定五元组会出现「改了策略仍能登录」的假象。

**显式排除：zone-pair 对「到设备自身」管理流量的 drop（self 例外）。**
`mgmt_path_hop()` 在 `cur == 目标设备` 时**只检查承载端口 oper up**，不查 `firewall_action`；
源设备自身也以 `in_port = None` 跳过 zone-pair 判定（与 `transmit_frame` 自发流量同语义）。
6B 已核的 self 语义不回退：`zone-pair ... destination self` 里的 deny **不**会挡住 `ssh <本机地址>`。

### 1.3 错误文案（按平台基线，有测试证据）

文案不混入设备输出之外的通道：仍是设备端 CLI 的 `deviceOutput`，模拟器提示只在 `simulatorNotices` / `[netemu]` 行。

| 登录源 | 数据面拒绝文案 | 证据 |
| --- | --- | --- |
| PC 主机（Linux/OpenSSH 基线） | `connect: Network is unreachable` | `c6b_login_denied_by_interface_shutdown` / `..._link_down` / `..._without_route` / `c6b_forwarded_login_follows_zone_based_policy` / `..._vrp_security_policy` |
| IOS 路由器作源 | `% Destination unreachable` | `c6b_router_sourced_login_is_also_grounded` |
| VTY 不放行该协议 | `Connection refused` | `c6b_transport_denied_stays_refused_not_unreachable` |
| 凭据不匹配 | `Permission denied` | C6a 既有用例（保留） |

「文案随源设备平台走」这一条单独有矩阵测试 `c6b_unreachable_text_follows_platform_baseline`。

### 1.4 与既有黄金测试的冲突裁决

冲突事实：`mgmt_golden.rs` 原断言 PC→R1 的 `ssh` 能登录（不涉及数据面），C6b 之后
「接口 down 的链路」必须拒绝。裁决：**不放宽既有断言，改为覆盖更强方向的改写**——
`link_down_then_recover` / `interface_shutdown_then_recover` 在原有「正常登录 + 断开」之外，
新增「先制造故障断言拒绝、再恢复断言可登录」，故障文案统一取主机基线常量 `HOST_UNREACHABLE`。
既有正常路径断言（`Username:` → `Password:` → `<R1>` / `EDGE1#` → `PC1>`）一字未改。

---

## 2. 决策 D2：Lab 6 任务原文的解析方案

Lab 6 的题目是 `labs.ts` / `labs-en.ts` 里的 `tasks: string[]`（不是 Markdown 代码块），
没有代码块锚点可用，因此定义固定文法（Rust 与 WASM 两份实现逐字同源）：

1. **去括号附注**：`（...）` / `(...)` 内是教学说明，不是命令；
2. **设备标签**：第一个箭头之前的 `X：` / `X: `；`PC1 终端` / `PC1 terminal` 归一化为 `PC1` 并标记为**交互终端**；
3. **箭头拆分**：按 `→` / `->` 切成单条步骤，一个箭头一条，不合并；
4. **期望结果**：每段可按 `——` / `—` / ` -- ` 再切出尾部期望；
5. **步骤三分类**：
   - `用户名 X、口令 Y` / `username X, password Y` → 认证输入（**不是** CLI 命令）；
   - `看到 R1# ...` / `seeing R1# ...` → 提示符断言（登录成功的判据）；
   - 其余 → CLI 命令；期望含「拒绝 / refused」→ 断言连接被拒。

门禁：
- 中英两侧必须解析出**完全相同**的 `Task` 序列（`PartialEq` 比较，标签归一化后），任一侧改文案就失败；
- 解析不出步骤、或标签不可路由（不是 `R1` / `PC1`）→ 失败，**不接受手写命令清单兜底**；
- 反向用例 `c6b_lab6_parser_rejects_prose_without_device_label`：无标签散文不算「一条命令」，
  空标签任务永远拿不到执行设备（这正是 C6b 之前课程原文无人执行的根因）。

**课程原文的一处修正（第 5 条任务）**：原文「登录后执行 show ip interface brief 查看 R1 接口，再 exit 断开」
是散文，既无设备标签也无箭头，按上述文法解析为 0 步。改为可执行写法（语义不变）：

- CN：`PC1 终端：show ip interface brief → exit（登录后在远端执行，查看 R1 接口后断开）`
- EN：`PC1 terminal: show ip interface brief -> exit (run on the remote device after login, then disconnect)`

修改的是**表述形式**，不是实验行为；三层测试因此可以逐条执行原文而不是读一段说明文字。

---

## 3. 决策 D3：讲义 47「抓包对比」文案

核验结论：抓包面板展示的是真实仿真帧（`ping` 的 ICMP 请求/应答、`tcp <IP> <端口>` 的 SYN/RST），
而远程登录在模拟器里是 **CLI 层抽象**——用户名/口令交互直接切换会话，不建 TCP 连接、不做 SSH 密钥交换。
因此「抓包对比 Telnet 与 SSH 的报文内容」**不可支撑**，属于对能力的虚假声明。

处置（`web/src/content/lessons/47-ssh-management.md`）：
- 删掉不可支撑的抓包对比表述；
- 改为可支撑的对比动作：把 **SSH 登录成功后的提示符** 与 **Telnet 的 `Connection refused`** 放在一起对比；
- 增补一条显式边界声明：登录不模拟 TCP 握手与密钥交换，抓包看不到登录报文，
  抓包面板真实可支撑的是 ICMP 与 TCP SYN/RST。

文本门禁：`c6b_lesson47_states_capture_and_show_ssh_boundaries` 断言讲义中**不得**再出现「抓包对比」，
且必须包含「不模拟 TCP 握手」「看不到」「ping」「SYN」等边界措辞。

---

## 4. 决策 D4：`show ip ssh` / `display ssh server status`

**不做**（保持未实现），依据：

1. 这两条是「SSH 服务是否启用 / 密钥是否生成」的查询。模拟器里 SSH 服务默认开启、不存在密钥生成状态，
   实现出来只能返回恒定的「enabled」，是**假输出**——比不实现更糟（违反「不得用编造输出充数」的既有约束）；
2. 该能力属于 AAA / 加密算法仿真范畴，本包明确排除；
3. 讲义的教学价值可以保留：第 1 步标注为**真机排障步骤**，并给出模拟器内的等价验证路径
   （`Connection refused` → 查 VTY 传输；`Permission denied` → 查用户与口令）；
4. 真实设备上「密钥没生成导致配了用户也登不上」这一头号坑，讲义如实注明**模拟器覆盖不到**。

讲义同时新增一节「登录也是数据面流量」，把 D1 的接地结论落成教学内容：
接口 shutdown / 链路 down / 无路由 / 防火墙拒绝都会让 `ssh` 失败，结论与 `ping` / `tcp <IP> 22` 一致；
并重申 self 例外（发往设备自身的管理流量不受本机 zone-pair 影响）。

---

## 5. 数据面接地实现

`crates/sim-core/src/engine.rs`（新增私有，全部围绕 `login_target`）：

| 函数 | 作用 |
| --- | --- |
| `login_target(from, target_ip, proto) -> Result<DevId, LoginError>` | 公开入口：先接地判定，再判 VTY 传输策略 |
| `mgmt_path_open(from, target_ip, proto) -> bool` | 逐跳走查入口，构造 `visited` |
| `mgmt_path_hop(cur, in_port, src_ip, target_ip, proto, visited)` | 单跳：出接口状态 → 入/出 ACL → 防火墙 → 对端端口 → 递归 |
| `mgmt_out_ports(cur, kind, in_port, target_ip)` | 候选出接口：主机看同网段/网关；路由器按 VRF 路由；交换机无管理路由时按 L2 透传扩散 |
| `mgmt_source_ip(from, target_ip)` | 仅供 ACL/防火墙匹配的源地址（主机取本机口，其余取出接口地址） |
| `mgmt_owner_port_up(dev, ip)` | 目标设备自身：承载地址的端口是否 oper up（self 例外落点） |
| `mgmt_link_peer(dev, port)` | 对端端口（载波检测） |

`try_login()` 在认证前先调 `login_target()`，失败即返回 `LoginError::Unreachable` / `TransportDenied`，
由 `cli-dialects` 按源设备平台渲染文案——鉴权逻辑（None / Line / Local 三种 `LineAuthMode`）完全未动。

已知边界（写在这里而不是藏起来）：
- 交换机在无管理路由时按 L2 透传处理，不细分 VLAN / STP；
- 判定**不发送真实报文、不推进虚拟时间**（纯查询，可在 `read_only` 判分路径安全调用）；
- `visited` 上限 64 条边，只做环路/规模护栏，不是转发语义。

---

## 6. 三方言黄金测试同步（覆盖更强方向）

`crates/cli-dialects/tests/mgmt_golden.rs`：

- 新增常量 `HOST_UNREACHABLE = "connect: Network is unreachable"`（三个用例的登录源都是 PC 主机，所以文案统一）；
- 新增 `Breakage<'a>` 参数包（避免函数参数过多，也避免三份重复的命令数组）；
- 新增 `login_once()` 复用完整登录流程；`link_down_then_recover()` / `interface_shutdown_then_recover()`
  在既有「正常登录」断言之后追加：**故障 → 必须拒绝 → 恢复 → 必须立即可登录**；
- IOS / VRP / Comware 三个方言各自跑一遍（关机/开机的命令与提示符按各自平台：`enable`/`system-view`、
  `g0/0`/`g0/0/0`/`g1/0/1`、`EDGE1#`/`<R1>`）。

新增 `crates/cli-dialects/tests/c6b_mgmt_grounding.rs`（11 项，正负全覆盖）：

| 用例 | 覆盖 |
| --- | --- |
| `c6b_login_succeeds_on_healthy_path` | 正常登录（正） |
| `c6b_login_denied_by_interface_shutdown` | 对端接口 shutdown 拒绝 |
| `c6b_login_denied_by_link_down` | 链路 down 拒绝 |
| `c6b_login_denied_without_route` | 无路由拒绝 |
| `c6b_transport_denied_stays_refused_not_unreachable` | VTY 限 SSH 时 Telnet 拒绝，且文案仍是 `Connection refused`（不得退化成不可达） |
| `c6b_unreachable_text_follows_platform_baseline` | 文案随源设备平台 |
| `c6b_forwarded_login_follows_zone_based_policy` | 转发路径上的 ZBF drop 生效 |
| `c6b_forwarded_login_follows_vrp_security_policy` | VRP `firewall packet-filter` 同步生效 |
| `c6b_self_management_traffic_ignores_zone_pair_drop` | **self 例外**：到设备自身的登录不被 zone-pair drop |
| `c6b_router_sourced_login_is_also_grounded` | 路由器作源同样接地，文案回落到 `% Destination unreachable` |
| `c6b_mgmt_proto_ports_are_standard` | SSH 22 / Telnet 23 端口常量 |

认证失败（`Permission denied`）沿用 C6a 既有用例，未改写。

---

## 7. Lab 6 课程原文三层测试

### 7.1 Rust 层 `crates/cli-dialects/tests/c6b_courses.rs`（3 项）

- `c6b_lab6_course_original_sequence_runs_on_real_engine`：从 `labs.ts` / `labs-en.ts` 解析出 6 条任务，
  在真实 `Engine` 上逐条执行 0 失败；断言完整提示符链（16 个提示符）：
  `R1#` → `R1(config)#` → `R1(config-if)#`×3 → `R1(config)#` → `R1(config)#` →
  `R1(config-line)#`×3 → `R1(config)#` → `Username:` → `R1#` → `R1#`（远端 show）→ `PC1>` → `PC1>`（Telnet 被拒）；
  并断言远端 `show ip interface brief` 输出确实来自 R1（含 `GigabitEthernet0/0` 与 `192.168.1.1`）、
  `telnet` 输出含 `refused`、`PC1` 会话的 `login` / `remote` 上下文均已清空。
- `c6b_lab6_parser_rejects_prose_without_device_label`：解析器门禁（见 §2）。
- `c6b_lesson47_states_capture_and_show_ssh_boundaries`：讲义 47 的 D3/D4 边界文本门禁。

### 7.2 WASM 层 `crates/sim-wasm/src/c6b_tests.rs`（3 项，`Sim` 真实 API）

- `c6b_lab6_course_original_sequence_runs_on_real_wasm_sim`：同一套解析 + 同一拓扑（`Sim::add_device` /
  `set_host_config` / `add_link`），逐条 `exec_command_result`，断言同一条提示符链与远端输出；
- `c6b_lab6_active_scoring_is_four_of_four_and_reacts_to_breakage`：`run_lab_checks` 4 项全绿；
  并做两次破坏重判：
  - 改 `transport input telnet` → `1 绿 / 3 红`（本地用户项仍绿）；
  - 恢复后再 `interface g0/0` + `shutdown` → `3 绿 / 1 红`（**SSH 登录项因数据面不可达而红**）；
  - 每次恢复后立即回到 4/4（判分不缓存旧结论）；
  另含判分定义与课程文件同形的文本门禁（CN `checks` 的 4 个 `type` 顺序、
  `expect: false`、EN `checkLabels` 数量 = 4）。
- `c6b_lab6_remote_session_channel_is_clean`：`ssh` → `Username:` → `Password:` → `R1#` 的会话切换；
  远端查询 `simulatorNotices` 为空（通道分离）；`exit` 输出 `Connection closed` 并回到 `PC1>`；
  R1 自己的本地会话停在课程原始终态，不受远端会话影响。

---

## 8. 讲义 47 处置（最终形态）

- 「SSH 登录要过三道门」重写为 VTY 放行 / 认证凭据 / 服务端用户，并点明**认证发生在服务器端**；
- 新增「登录也是数据面流量」：四类故障 + self 例外；
- 「经典坑」第 1 步明确标注**真机排障步骤**，并给出模拟器内的等价验证路径与「密钥坑覆盖不到」的说明；
- 「动手」改为可支撑的对比（提示符 vs `Connection refused`）+ 抓包边界声明；
- `lab: mgmt-ssh` 绑定保持不变。

---

## 9. 真实浏览器验收（Chrome + 本次构建）

构建：`wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm`
→ `net_sim_bg-xkuCRFmj.wasm`（1,744,142 B）；
`pnpm --dir web run build` → `assets/index-CxRWmvRD.js`（907,080 B）。
用 `vite preview` 服务 `web/dist`（`http://localhost:4177`）。

验收脚本（Playwright 1.65.1，`channel: 'chrome'`，headless；脚本与日志在临时目录，见 §11）：

| 项 | 结果 |
| --- | --- |
| PWA 清缓存 | 注销 1 个 service worker + 删除 1 个 cache，再带 `?v=c6b` 重载 |
| 拓扑 | `PC1`、`R1`（实验六的 `build`） |
| R1 端配置（键盘逐条） | 11 条命令，提示符逐条精确匹配（`R1#` → `R1(config)#` → `R1(config-if)#`×3 → `R1(config)#` → `username` → `R1(config-line)#`×3 → `R1(config)#`） |
| PC1 交互登录（键盘） | `ssh 192.168.1.1` → `Username:` → `admin` → `Password:` → `Cisco@123` → `R1#` |
| 远端 show | `show ip interface brief` 输出含 `GigabitEthernet0/0` 与 `192.168.1.1` |
| 断开 | `exit` → `[Connection closed by foreign host]` → `PC1>` |
| Telnet 被拒 | `telnet 192.168.1.1` → `Connection refused` → `PC1>` |
| 主动判分 | **4/4 全绿**（本地用户 / VTY 仅 SSH / SSH 登录成功 / Telnet 被拒） |
| 破坏 1：`transport input telnet` | **1 绿 / 3 红**（VTY 项、SSH 登录项、Telnet 负向项翻转） |
| 破坏 2：恢复 VTY 后 `interface g0/0` + `shutdown` | **3 绿 / 1 红**（SSH 登录失败原因 `Unreachable`，即数据面接地在浏览器可见） |
| 恢复后再判分 | **4/4** |
| 控制台 | `pageerror` 0、`console.error` 0 |

截图（`docs/cli-fidelity/`）：
- `c6b-lab6-login.png`（登录后 `R1#` + 远端 show 输出）
- `c6b-lab6-checks.png`（4/4 判分）
- `c6b-lab6-breakage.png`（接口 shutdown 后的重判分）

一处如实说明：破坏 2 里「Telnet 登录被拒绝」项仍显示通过，原因为 `Unreachable` 而非 `TransportDenied`。
这是 6B 既有判分语义（`expect: false` 只断言「没有登录成功」，不区分拒绝原因），C6b 不扩大范围去改它，
在此记录以免被误读为「VTY 仍在拦截」。

---

## 10. 六项门禁（真实结果）

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| 1. clippy | `cargo clippy --workspace --all-targets -j 1 -- -D warnings` | ✅ 0 warning / 0 error（过程见 §10.1） |
| 2. 测试 | `cargo test --workspace -j 1` | ✅ 349 passed / 0 failed / 52 个二进制 |
| 3. WASM | `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | ✅ 成功；**wasm-opt 正常执行、未失败**、未关闭优化 |
| 4. 类型检查 | `npx --yes pnpm@10 --dir web run check` | ✅ `tsc --noEmit` 无输出 |
| 5. 构建 | `npx --yes pnpm@10 --dir web run build` | ✅ `index-CxRWmvRD.js` 907.08 kB / PWA 11 项预缓存 |
| 6. 课程表 | `stage3a_cmdmap --nocapture` | ✅ `TOTAL_ROWS=449 REJECTED=0` |

### 10.1 clippy 过程中的两处修正（如实记录）

1. `Engine::mgmt_path_hop` 初版 8 个参数，触发 `too_many_arguments`；
   改用「`visited` 兼作跳数护栏」方案（去掉 `depth` 参数，改判 `visited.len() > 64`），保持 7 个参数、语义等价。
2. `mgmt_golden.rs` 的破坏/恢复辅助函数初版 9 个参数，同上规则；
   抽出 `Breakage<'a>` 参数包 + `login_once()`，函数参数降到 5 个。

---

## 11. 临时产物与工作区如实计数

| 临时产物 | 状态 |
| --- | --- |
| `crates/cli-dialects/tests/zz_c6b_probe.rs`（负向矩阵探针） | **GONE**（`delete_file` 删除，并按真实文件系统核验不存在） |
| `C:\Users\22534\AppData\Local\Temp\pwc6b\`（Playwright 脚本 + `node_modules` + `result.json` + 日志） | **GONE**（`Test-Path` = False 核验） |
| `e:\网络命令\c6b-preview-out.log` / `c6b-preview-err.log` | **GONE**（核验 False） |
| `e:\网络命令\c6b-final-test.log`（`cargo test` 汇总日志） | **GONE**（核验 False） |
| `vite preview` 进程（4177） | 已终止，端口 4177 监听数 = 0 |
| 独立 `CARGO_TARGET_DIR`（`.../netemu-c6b-target`） | 仓库外，保留不影响工作区 |

工作区最终计数：`git status --porcelain` 折叠口径 **64 行**（36 修改 + 未跟踪目录折叠为 28 行）；
`git status --porcelain -uall` 精确口径 **100 项**（36 修改 + 64 未跟踪文件），

其中 C6b 自身新增/修改：
`crates/sim-core/src/engine.rs`、`crates/cli-dialects/tests/mgmt_golden.rs`、
`crates/cli-dialects/tests/c6b_mgmt_grounding.rs`(新)、`crates/cli-dialects/tests/c6b_courses.rs`(新)、
`crates/sim-wasm/src/lib.rs`（模块注册）、`crates/sim-wasm/src/c6b_tests.rs`(新)、
`web/src/lib/labs.ts`、`web/src/content/labs-en.ts`、`web/src/content/lessons/47-ssh-management.md`、
`docs/cli-fidelity/c6b-implementation-record.md`(新)、`docs/cli-fidelity/c6b-lab6-*.png`(3 张)。

未提交、未推送、HEAD 未变。

---

## 12. C6b 收口结论

1. **缺陷已修**：管理面登录不再绕开数据面。接口 shutdown / 链路 down / 无路由 / 防火墙 drop 转发流量
   四种故障下 `ssh` 一律失败，判定复用既有路由、接口状态、ACL、防火墙与 self 例外，未建第二套转发路径。
2. **6B 语义未回退**：到设备自身的登录不受本机 zone-pair 策略影响（self 例外有独立正负用例与讲义段落）。
3. **课程原文可执行**：Lab 6 的 6 条任务可以被解析成 17 个可执行步骤（R1 11 条 + PC1 6 条）并在 Rust / WASM / 真实浏览器三层逐条跑通，
   中英两侧解析结果强制一致；解析器对散文与无标签任务会失败，不会静默少跑。
4. **判分有效**：`run_lab_checks` 4 项全绿，改 VTY 传输与关接口两种破坏都能让对应项立刻变红，恢复后回到全绿。
5. **文案边界诚实**：讲义 47 删除了不可支撑的「抓包对比」，`show ip ssh` / `display ssh server status`
   明确标注为真机步骤并说明模拟器未实现及原因。
6. **门禁全绿**：clippy 0 warning、349 测试全通过、WASM 构建（wasm-opt 正常）、`tsc` 与 `vite build` 通过、
   课程表 449 单元格零拒绝、真实 Chrome 控制台 0 错误。

C6b 范围内工作到此结束，未进入新平台/新方言、ZBF self 区域策略数据面、`run()` 动画、Console 线、
AAA 服务器、SCP/SFTP、SSH 加密算法仿真等明确排除项。
