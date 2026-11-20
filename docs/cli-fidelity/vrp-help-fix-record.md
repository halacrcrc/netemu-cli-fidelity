# VRP 终端 `?` 上下文帮助修复实施记录

- 日期：2026-11-20
- 仓库：`e:\网络命令\netemu`，分支 `main`
- 固定基线：`20c356f8e930408fad35014e592f7fed5d1b7b25`（执行前后 `HEAD` 未变，未提交、未推送）
- 依据：`netemu-vrp-help-fix-implementation-handoff.md`（范围与验收）、`netemu-rust-cli-kernel-evaluation-and-design.md` §4.4/§5/§9/§10.4/§15
- 性质：已定位缺陷的前端小修 + 服务端契约回归测试；**未新增任何 VRP 命令**

---

## 1. 用户问题与根因

**用户反馈**：华为设备在用户视图与系统视图中输入 `?` 不显示上下文候选，似乎只能用教程/实验规定的命令。

**根因（复核方已定位，本次复现一致）**：后端 `?` 帮助能力一直存在，前端漏传问号。

`web/src/components/TerminalPanel.svelte` 原 `case '?'`：

```js
ed.term.write('?');
const out = execCommandResult(getSim(), ed.dev, ed.buffer).deviceOutput; // ← 发送的是不带 ? 的原缓冲区
if (out) ed.term.write('\r\n' + out);
ed.buffer = ed.buffer.replace(/[?]/g, '').trimEnd();                    // ← 事后用 trimEnd 重写输入
```

实际后果：单按 `?` 发送空串（返回空输出）；`display ?` 只发送 `display `（按普通命令解析）；`system-view?` 只发送 `system-view`（可能真的切视图）。

**服务端既有能力（未改动）**：`crates/cli-dialects/src/lib.rs:5302`

```rust
if trimmed.contains('?') {
    return CommandResult::executed(render_help(dialect, sess, line), false);
}
```

`?` 在解析之前被截获 → `render_help` → `cli_engine::help_for(t.root_for(sess.mode), line, dialect)` → 当前会话模式对应的 `CNode`。因此帮助天然是「当前模式的可选命令」，不是白名单，且 `state_changed=false`。

---

## 2. 实际修改

```
 crates/cli-dialects/tests/stage2c_flows.rs | 119 +++++++++++++++++++++++++++++
 web/src/components/TerminalPanel.svelte    |   7 +-
 2 files changed, 124 insertions(+), 2 deletions(-)
```

生产代码只改 `case '?'`（唯一一处）：

```js
case '?': {
  // `?` 只是临时查询：回显后按「原始输入 + ?」请求既有帮助，`?` 不写入编辑缓冲区。
  // 帮助渲染完按原样恢复输入（含尾随空格），不 trimEnd、不提交、不记历史、不执行原命令。
  const originalBuffer = ed.buffer;
  ed.term.write('?');
  const out = execCommandResult(getSim(), ed.dev, originalBuffer + '?').deviceOutput;
  if (out) ed.term.write('\r\n' + out);
  ed.buffer = originalBuffer;
  ed.term.write('\r\n' + promptOf(getSim(), ed.dev) + ed.buffer);
  return;
}
```

对照冻结决策：

| 决策 | 落地 |
|---|---|
| D1 查询串 = `originalBuffer + '?'`，不建平行帮助表 | 复用 `execCommandResult → Sim.exec_command_result → cli_dialects → help_for → CNode`，无新表/新解析器 |
| D2 帮助后精确恢复原输入 | `ed.buffer = originalBuffer`（不 `trimEnd`、不 `replace`）；不 `submit()`、不入历史、不执行原命令 |
| D3 不建纯读帮助 API | 未改 `exec_and_run_result`、未新增 WASM API、未改协议推进契约 |
| D4 输出保真 | 帮助文本仍走 `deviceOutput` 单通道；未新增逐字排版黄金 |

**未改**：`cli-engine`、`cli-dialects` 生产代码、华为/IOS/Comware/TMOS 命令树、规格表、WASM API。

---

## 3. 新增回归测试

`crates/cli-dialects/tests/stage2c_flows.rs` → `vrp_context_help_user_and_system_views_from_cnode`（真实 `Engine` + `Session` + VRP `Router`）：

| # | 断言 | 性质 |
|---|---|---|
| 1 | 用户视图 `?`：`Executed`，输出含 `system-view` / `display` / `ping`；`state_changed=false`；`state_fingerprint` 与 `now_ns()` 不变 | 正向 + 纯读 |
| 2 | `display ?`：含 `current-configuration`，不含 `Error`（不是普通命令错误） | 正向 |
| 3 | `dis?`：前缀候选含 `display`，会话仍 `Mode::Exec` | 正向 + 不执行 |
| 4 | `system-view?`：出候选且**仍为 `Mode::Exec`**、状态指纹不变 | **负向：不得切视图** |
| 5 | 正常 `system-view` 后 `?`：含 `sysname` / `interface` / `undo` | 正向 |
| 6 | 系统视图 `display ?` 与 `display c?`：同一 `CNode` 子节点，状态不变 | 正向 |
| 7 | 帮助输出 `scan_simulator_wording` 为空、不含 `[netemu]` | 通道污染负向 |

不断言逐字空格/列宽/完整候选全集（无带版本 VRP 设备样本）。**未删改任何既有测试**。

---

## 4. 真实浏览器验收

构建：本次 `wasm-pack` + `pnpm build` 产物用 `vite preview`（`http://localhost:4322`）服务。

### 4.1 缓存与产物核对

| 项 | 结果 |
|---|---|
| Service Worker | 注销前 1 个 → **注销后 0 个** |
| caches | 删除前 `["workbox-precache-v2-http://localhost:4322/"]` → **删除后 `[]`** |
| 导航 | 带新查询参数 `?v=vrphelp2` 重新加载 |
| 实际加载资源 | `index-CSQB9hiL.js`、`index-DsnzkBrK.css`、`net_sim_bg-DUZR5JWb.wasm`（与本次 `web/dist` 完全一致） |

### 4.2 逐项结果（全部为**真实 xterm 键盘事件**：`page.keyboard.type/press` → `term.onData`）

拓扑：`设备库 → 路由器 → 华为 VRP → 添加`，属性面板改名 `R1`（`dump` 确认 `{name:"R1", dialect:"Vrp"}`）。

| # | 操作 | 结果 |
|---|---|---|
| 1 | `<R1>` 单按 `?` | 列出 `system-view` / `ping` / `display` / `save` / `reset` / `quit` / `telnet` / `ssh` + `<cr>`；回车后仍为 `<R1>`。`dump()` 前后完全相同 |
| 2 | 输入 `display ` 再按 `?` | 列出 `display` 子命令（`ip`/`vlan`/`firewall`/`nat`/`stp`/`eth-trunk`… + `<cr>`）；恢复行 `<R1>display ` |
| 3 | 承接上一步直接按 `c?` | 输出 `current-configurationCurrent configuration`，恢复行 `<R1>display c ` —— **反证尾随空格被保留**（若被 `trimEnd`，请求会变成 `displayc?` 而无候选） |
| 4 | 输入 `dis` 再按 `?` | 前缀候选 `display`；恢复行 `<R1>dis`（不自动补全、不加空格） |
| 5 | 输入 `system-view` 再按 `?` | 仅列出 `system-view` 候选；**提示符仍是 `<R1>`，未出现 `[R1]`**；`dump()` 不变。随后 `Ctrl+C` 清理输入 |
| 6 | `Ctrl+C` → 回车执行 `system-view` → `[R1]` 单按 `?` | 列出 `sysname` / `ospf` / `ospfv3` / `srv6` / `bgp` / `security-policy` / `firewall` / `ipsec` / `interface` / `undo` / `display` / `quit` / `return` / `save` + `<cr>`；提示符保持 `[R1]` |
| 7 | 系统视图 `display c?` | 输出 `current-configuration`，恢复行 `[R1]display c ` |
| 8 | 状态对照 | 连续两次 `?` 之间 `dump()` **字符串完全相同**；配置无变化、无重复帮助、无 `[netemu]` 混入（`[netemu]` 关键字命中 false） |
| 9 | 历史记录 | 帮助后按 `↑`，回显的是真实提交过的 `system-view`，**不是** `system-view?` / `dis` / `display ` → 帮助未入历史 |
| 10 | IOS 共享分支回归 | 新增 IOS 路由器（`Router2`），`enable` 后 `show ip ?` → 列出 `ospf`/`nat`/`dhcp`/`bgp` + `<cr>`，恢复行 `Router2#show ip `；未改 IOS 后端 |
| 11 | 控制台 | `Total messages: 0 (Errors: 0, Warnings: 0)` |

### 4.3 截图

| 文件 | 内容 |
|---|---|
| `docs/cli-fidelity/vrp-help-user-view.png` | `<R1>?` 用户视图候选 + 恢复 `<R1>` |
| `docs/cli-fidelity/vrp-help-system-view.png` | `[R1]?` 系统视图候选（`sysname`/`ospf`/`ospfv3`/`srv6`/`bgp`/`security-policy` 等） |
| `docs/cli-fidelity/vrp-help-preserved-input.png` | `display ?` 帮助后恢复 `<R1>display `（尾随空格可见，光标停在空格后） |

---

## 5. 六项门禁（真实输出）

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo clippy --workspace --all-targets -j 1 -- -D warnings` | ✅ 0 warning / 0 error（`Finished dev profile in 48.44s`） |
| 2 | `cargo test --workspace -j 1` | ✅ **356 passed / 0 failed / 53 个测试二进制** |
| 3 | `cargo test -p cli-dialects --test stage3a_cmdmap stage3a_cmdmap_probe_rejected_rows -j 1 -- --nocapture` | ✅ `TOTAL_ROWS=449 REJECTED=0` |
| 4 | `wasm-pack build crates/sim-wasm --target web --out-name net_sim --out-dir ../../web/src/wasm` | ✅ 成功；`wasm-opt` 正常执行（本次**未**出现 `os error 5`）；`net_sim_bg.wasm` 1,765,848 B |
| 5 | `npx --yes pnpm@10 --dir web run check`（`tsc --noEmit`） | ✅ 0 错误 |
| 6 | `npx --yes pnpm@10 --dir web run build` | ✅ 成功；**已知警告未隐瞒**：`(!) Some chunks are larger than 500 kB after minification`（既有体积告警，与本次改动无关）；PWA precache 11 entries |

构建产物：`index-CSQB9hiL.js` 913,404 B、`index-DsnzkBrK.css` 118,515 B、`net_sim_bg-DUZR5JWb.wasm` 1,765,848 B。

### 5.1 测试数变化

| 时点 | 测试数 | 二进制 |
|---|---|---|
| 基线（`20c356f`） | 355 | 53 |
| 本次 | **356** | 53 |
| 增量 | **+1**（`vrp_context_help_user_and_system_views_from_cnode`） | 0（落在既有 `stage2c_flows` 二进制内） |

### 5.2 格式化与 wasm-opt 环境事项

- 本机 `rustfmt` 对 `cli-dialects/src/{ios,lib}.rs` 仍会挂起（既有环境问题），本次**按要求只对改动文件**执行：`rustfmt --edition 2021 --check crates/cli-dialects/tests/stage2c_flows.rs` → **exit 0**。
- 未对任何无关文件做格式化，未追求全局 fmt 变绿。
- `wasm-opt` 本次正常完成；未关闭优化。

---

## 6. 工作区与临时产物

### 6.1 最终 `git status`（`HEAD` 仍为 `20c356f`）

```
 M crates/cli-dialects/tests/stage2c_flows.rs
 M web/src/components/TerminalPanel.svelte
?? docs/cli-fidelity/vrp-help-preserved-input.png
?? docs/cli-fidelity/vrp-help-system-view.png
?? docs/cli-fidelity/vrp-help-user-view.png
```

已跟踪修改 **2** 个；未跟踪新增 **3** 个（均为计划保留的验收截图）；本记录文件随后新增。

### 6.2 临时产物 GONE 核验（按文件系统）

| 产物 | 用途 | 核验 |
|---|---|---|
| `E:\netemu-target-help\` | 独立 `CARGO_TARGET_DIR`（Rust 全程 `-j 1`） | `Test-Path` → **False** |
| `E:\vrphelp-preview.log` / `.err` | `vite preview` 日志 | 均 **False** |
| `.playwright-cli\` | 浏览器快照目录 | **False** |
| `vite preview` 进程（:4322） | 验收服务 | 进程已终止；`Invoke-WebRequest` 连接失败 = 已停 |
| 保留 | 三张验收截图 + 本记录 | 属交付物，非临时产物 |
| 忽略目录 | `web/src/wasm/`、`web/dist/` | 被 `.gitignore` 覆盖，未入库 |

---

## 7. 声明

- **未新增任何 VRP 命令**，未改命令树、规格表、`DeviceProfile`、WASM API。
- **未修改后端帮助架构**：`help_for` / `render_help` / `exec_line_result` 的 `?` 分支一行未动。
- **未删测、未注释测试、未跳过测试、未放宽既有断言**；测试数只增不减（355 → 356）。
- **未提交、未推送、未打 tag、未发布**；`HEAD` 保持 `20c356f`。
- 未处理任何范围外事项（SRv6/Lab 22、`enable secret`、PWA 首载、部署、仓库可见性、`<cr>` 推断、Tab 参数补全、Profile 过滤）。

### 7.1 范围外观察（只登记，未修）

`format_help` 使用 `{:<20}` 固定列宽，候选词长 21 字符时（`current-configuration`）会与帮助文本粘连，实际显示为
`  current-configurationCurrent configuration`。
这是**既有**通用排版行为（属 D4 明确排除的"无设备样本逐字排版"范畴），本次未改动，记录备查。

---

## 8. 待复核方重点核对

1. 浏览器证据是否确为真实键盘链路（可用同样步骤在本地复跑 §4.2 的 1/2/5/6 四项）；
2. `originalBuffer + '?'` 与后端 `help_for` 的接触面是否还有未覆盖形态（如行首空格、`?` 后紧跟空格）；
3. 新增测试是否有"过度断言"（本包刻意只做字段/状态级，不含逐字空格与列宽）；
4. `display c?` 反证法（§4.2 第 3 项）是否足以证明尾随空格保留，或需要补更直接的缓冲区断言。
