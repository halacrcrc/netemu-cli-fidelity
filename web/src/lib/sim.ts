// WASM 引擎加载与类型桥（与 Rust 侧 NetSummary 对应，字段为 snake_case）。

import init, { Sim } from '../wasm/net_sim';

export interface PortSummary {
  idx: number;
  admin_up: boolean;
  ip: string | null;
  prefix: number;
  linked: boolean;
  ipv6?: string | null;
  prefix6?: number;
  zone?: string | null;
}

export interface StaticRouteSummary {
  dst: string;
  len: number;
  nexthop: string;
}

export type DeviceKind =
  | 'Router'
  | 'Switch'
  | 'Host'
  | 'Firewall'
  | 'AccessPoint'
  | 'WlanController'
  | 'LoadBalancer';
export type Dialect = 'Ios' | 'Vrp' | 'Comware' | 'Tmos';

export interface DevSummary {
  id: number;
  name: string;
  kind: DeviceKind;
  dialect: Dialect;
  /** 稳定 Profile ID；阶段 0 旧档缺失，由 wasm load 明确映射。 */
  profile?: string | null;
  ports: PortSummary[];
  gw: string | null;
  gw6?: string | null;
  static_routes: StaticRouteSummary[];
  dhcp_enable?: boolean;
}

export interface LinkSummary {
  id: number;
  up: boolean;
  a: [number, number];
  b: [number, number];
}

export interface NetSummary {
  /** 0/缺失 = 阶段 0 旧存档（无 Profile），由 wasm load 迁移。 */
  schema_version?: number;
  devices: DevSummary[];
  links: LinkSummary[];
}

/** 四态执行结果；"待验证"是证据元数据，不是第五态。 */
export type CommandStatus = 'Executed' | 'UnsupportedByDevice' | 'NotSimulated' | 'InvalidCommand';

/** device_output 只放真实设备文本；模拟器限制/验证状态只放 simulatorNotices。 */
export interface CommandResult {
  status: CommandStatus;
  deviceOutput: string;
  simulatorNotices: string[];
  stateChanged: boolean;
}

/** 被动命令检查（Profile 级 / 设备会话级）的纯读结构化结果。 */
export interface InspectionResult {
  passed: boolean;
  detail: string;
  status: CommandStatus;
  support: string;
  evidence: string;
  specId: string;
}

export interface LoadReport {
  schemaVersion: number;
  legacy: boolean;
  mappedProfiles: number;
  notices: string[];
}

export interface EngineStats {
  virtualNs: number;
  events: number;
  inFlight: number;
  dropped: number;
}

/** 实验批量检查的 WASM 入口（由 wasm-bindgen 生成，保持同步调用契约）。 */
export interface LabCheckApi {
  read_only_lab_checks(defsJson: string): string;
  run_lab_checks(defsJson: string): string;
}

/** Web 声明与生成的 d.ts 之间的类型桥；调用的是真实 WASM 方法，不是桩。 */
export function labCheckApi(sim: Sim): LabCheckApi {
  return sim as unknown as LabCheckApi;
}

let simInstance: Sim | null = null;

export async function loadSim(): Promise<Sim> {
  await init();
  simInstance = new Sim();
  return simInstance;
}

export function getSim(): Sim {
  if (!simInstance) throw new Error('引擎未初始化');
  return simInstance;
}

// ---------- 封装常用调用 ----------

export function dumpSummary(sim: Sim): NetSummary {
  return JSON.parse(sim.dump()) as NetSummary;
}

/** 迁移期文本入口；前端终端请用 `execCommandResult` 分通道渲染。 */
export function execCommand(sim: Sim, dev: number, line: string): string {
  return sim.exec_command(dev, line);
}

export function execCommandResult(sim: Sim, dev: number, line: string): CommandResult {
  return JSON.parse(sim.exec_command_result(dev, line)) as CommandResult;
}

/** Profile 级纯读检查（平台级结果，无设备实例）。 */
export function inspectCommand(
  sim: Sim,
  profileId: string,
  mode: string,
  line: string,
): InspectionResult {
  return JSON.parse(sim.inspect_command(profileId, mode, line)) as InspectionResult;
}

/** 设备会话级纯读检查；需要真实会话令牌，不推进虚拟时间、不发探测包。 */
export function verifyDeviceCommand(
  sim: Sim,
  dev: number,
  sessionId: string,
  line: string,
): InspectionResult {
  return JSON.parse(sim.verify_device_command(dev, sessionId, line)) as InspectionResult;
}

export function sessionIdOf(sim: Sim, dev: number): string {
  return sim.session_id(dev);
}

/** 加载并返回旧档映射/会话清空报告，供 UI 明示。 */
export function loadWithReport(sim: Sim, net: NetSummary): LoadReport {
  return JSON.parse(sim.load_with_report(JSON.stringify(net))) as LoadReport;
}

export function promptOf(sim: Sim, dev: number): string {
  return sim.prompt(dev);
}

export function completeOf(sim: Sim, dev: number, line: string): { line: string; options: string[] } {
  return JSON.parse(sim.complete(dev, line)) as { line: string; options: string[] };
}

export function statsOf(sim: Sim): EngineStats {
  return JSON.parse(sim.stats()) as EngineStats;
}

export function portOper(sim: Sim, dev: number, port: number): boolean {
  const s = dumpSummary(sim);
  const d = s.devices.find((x) => x.id === dev);
  if (!d) return false;
  const p = d.ports[port];
  if (!p) return false;
  return p.admin_up && p.linked;
}
