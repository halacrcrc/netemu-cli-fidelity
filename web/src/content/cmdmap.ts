// 三厂商命令对照表（单一数据源）。
// 每行按显式选择的 router / switch Profile 走 sim.inspect_command 的平台级纯读检查；
// 不再使用旧 boolean verify_command 推导「完整支持」。

export type CmdMode =
  | 'Exec'
  | 'Priv'
  | 'Config'
  | 'If'
  | 'Pool'
  | 'Acl'
  | 'Line'
  | 'Aaa'
  | 'Mst'
  | 'Ospf'
  | 'OspfArea'
  | 'Bgp'
  | 'Isis'
  | 'RoutePolicy'
  | 'Zone'
  | 'SecurityPolicy'
  | 'SecurityRule'
  | 'ClassMap'
  | 'PolicyMap'
  | 'PolicyMapClass'
  | 'Ipsec'
  | 'Ospfv3';

/** 平台角色：决定能力门禁（尤其 IOS 的 L2SwitchPort 在 router / switch 下不同）。 */
export type ProfileRole = 'router' | 'switch';
/** 阶段 5A：F5 TMOS 17.x 是独立第四方言，LTM 命令只出现在这一列。 */
export type CmdVendor = 'ios' | 'vrp' | 'comware' | 'tmos';

/** 与 sim-core::profile 中稳定 Profile ID 一一对应；缓存 key 必须包含它。 */
export const PROFILE_IDS: Record<CmdVendor, Record<ProfileRole, string>> = {
  ios: { router: 'ios-xe-17-router', switch: 'ios-xe-17-l3-switch' },
  vrp: { router: 'vrp-generic-router', switch: 'vrp-generic-l3-switch' },
  comware: { router: 'comware-generic-router', switch: 'comware-generic-l3-switch' },
  // TMOS 只有 LoadBalancer 一种设备类型，不按 router/switch 拆分。
  tmos: { router: 'f5-bigip-tmos-17-loadbalancer', switch: 'f5-bigip-tmos-17-loadbalancer' },
};

/**
 * 课程映射：逐 topic 显式声明平台角色，避免把 L2 命令静默套到 router Profile。
 * 未登记的 topic 会在 profileForRow 中报错，而不是默认成 router。
 */
export const TOPIC_PROFILE_ROLE: Record<string, ProfileRole> = {
  basics: 'router',
  vlan: 'switch',
  stp: 'switch',
  dhcp: 'router',
  acl: 'router',
  nat: 'router',
  ospf: 'router',
  bgp: 'router',
  mpls: 'router',
  security: 'router',
  ipsec: 'router',
  sslvpn: 'router',
  wlan: 'router',
  vxlan: 'switch',
  ha: 'switch',
  ltm: 'router',
  campus: 'router',
  lag: 'switch',
  stack: 'switch',
  mlag: 'switch',
  fhrp: 'router',
  ipv4: 'router',
  routing: 'router',
  ipv6: 'router',
  services: 'router',
  isis: 'router',
  qos: 'router',
  ops: 'router',
  isolation: 'switch',
  ospfv3: 'router',
  srv6: 'router',
  mgmt: 'router',
};

export interface CmdRow {
  /** 功能描述 */
  feature: string;
  topic: string;
  /** 各平台真实写法；空字符串表示该平台无逐字对应命令，不调用 inspection。 */
  ios: string;
  vrp: string;
  comware: string;
  /** 阶段 5A：F5 TMOS 17.x / tmsh 真实写法（其余行留空 = 不适用）。 */
  tmos?: string;
  /** 命令所在模式（各列共用；tmsh 的 root 模块是 Exec） */
  mode: CmdMode;
  /** 个别厂商模式不同时的覆盖 */
  modeIos?: CmdMode;
  modeVrp?: CmdMode;
  modeComware?: CmdMode;
  modeTmos?: CmdMode;
  /** 行级显式角色覆盖（例如 campus 中的 Trunk 行）。 */
  role?: ProfileRole;
  /** 行级显式 Profile ID 覆盖（最高优先级）。 */
  profileIos?: string;
  profileVrp?: string;
  profileComware?: string;
  profileTmos?: string;
  /**
   * Stage 3B：netemu 教学抽象标识（课程内容元数据，不是第五种执行状态）。
   * 只标在确实不是厂商 CLI 的单元格上；结构化 status/support/evidence 照常展示。
   */
  teachingVendors?: CmdVendor[];
}

export function rowMode(row: CmdRow, vendor: CmdVendor): CmdMode {
  if (vendor === 'ios') return row.modeIos ?? row.mode;
  if (vendor === 'vrp') return row.modeVrp ?? row.mode;
  if (vendor === 'comware') return row.modeComware ?? row.mode;
  return row.modeTmos ?? row.mode;
}

/** 每行显式选择 Profile：行级 ID > 行级角色 > 已登记的 topic 角色。 */
export function profileForRow(row: CmdRow, vendor: CmdVendor): string {
  const explicit =
    vendor === 'ios' ? row.profileIos : vendor === 'vrp' ? row.profileVrp : vendor === 'comware' ? row.profileComware : row.profileTmos;
  if (explicit) return explicit;
  const role = row.role ?? TOPIC_PROFILE_ROLE[row.topic];
  if (!role) throw new Error(`课程对照表 topic ${row.topic} 未显式登记 Profile 角色`);
  return PROFILE_IDS[vendor][role];
}

export const CMDMAP: CmdRow[] = [
  // 基础
  { feature: '设置设备名称', topic: 'basics', ios: 'hostname R1', vrp: 'sysname R1', comware: 'sysname R1', mode: 'Config' },
  { feature: '查看接口摘要', topic: 'basics', ios: 'show ip interface brief', vrp: 'display ip interface brief', comware: 'display ip interface brief', mode: 'Exec' },
  { feature: 'ping 测试', topic: 'basics', ios: 'ping 10.0.0.2', vrp: 'ping 10.0.0.2', comware: 'ping 10.0.0.2', mode: 'Exec' },
  { feature: '配置接口 IP', topic: 'basics', ios: 'ip address 192.168.1.1 255.255.255.0', vrp: 'ip address 192.168.1.1 24', comware: 'ip address 192.168.1.1 24', mode: 'If' },
  { feature: '启用接口', topic: 'basics', ios: 'no shutdown', vrp: 'undo shutdown', comware: 'undo shutdown', mode: 'If' },
  { feature: '静态路由', topic: 'basics', ios: 'ip route 192.168.2.0 255.255.255.0 10.0.0.2', vrp: 'ip route-static 192.168.2.0 24 10.0.0.2', comware: 'ip route-static 192.168.2.0 24 10.0.0.2', mode: 'Config' },
  { feature: '查看路由表', topic: 'basics', ios: 'show ip route', vrp: 'display ip routing-table', comware: 'display ip routing-table', mode: 'Exec' },

  // VLAN
  { feature: '创建 VLAN', topic: 'vlan', ios: 'vlan 10', vrp: 'vlan batch 10', comware: 'vlan 10', mode: 'Config' },
  { feature: '进入接口', topic: 'vlan', ios: 'interface g0/0', vrp: 'interface g0/0/1', comware: 'interface g1/0/1', mode: 'Config' },
  { feature: 'Access 口划 VLAN', topic: 'vlan', ios: 'switchport access vlan 10', vrp: 'port default vlan 10', comware: 'port access vlan 10', mode: 'If' },
  { feature: 'Trunk 模式', topic: 'vlan', ios: 'switchport mode trunk', vrp: 'port link-type trunk', comware: 'port link-type trunk', mode: 'If' },
  { feature: 'Trunk 放行 VLAN', topic: 'vlan', ios: 'switchport trunk allowed vlan 10,20', vrp: 'port trunk allow-pass vlan 10 20', comware: 'port trunk permit vlan 10 20', mode: 'If' },
  { feature: 'SVI 网关', topic: 'vlan', ios: 'interface vlan 10', vrp: 'interface vlanif 10', comware: 'interface vlan-interface 10', mode: 'Config' },
  { feature: '查看 VLAN', topic: 'vlan', ios: 'show vlan brief', vrp: 'display vlan', comware: 'display vlan', mode: 'Exec' },

  // STP
  { feature: '设置 STP 优先级', topic: 'stp', ios: 'spanning-tree vlan 1 priority 4096', vrp: 'stp priority 4096', comware: 'stp priority 4096', mode: 'Config' },
  { feature: '关闭 STP', topic: 'stp', ios: 'no spanning-tree vlan 1', vrp: 'stp disable', comware: 'stp disable', mode: 'Config' },
  { feature: '接口成本', topic: 'stp', ios: 'spanning-tree vlan 1 cost 100', vrp: 'stp cost 100', comware: 'stp cost 100', mode: 'If' },
  { feature: '查看 STP', topic: 'stp', ios: 'show spanning-tree', vrp: 'display stp brief', comware: 'display stp brief', mode: 'Exec' },

  // DHCP
  { feature: '创建地址池', topic: 'dhcp', ios: 'ip dhcp pool LAN', vrp: 'ip pool LAN', comware: 'ip pool LAN', mode: 'Config' },
  { feature: '池网段', topic: 'dhcp', ios: 'network 192.168.1.0 255.255.255.0', vrp: 'network 192.168.1.0 mask 255.255.255.0', comware: 'network 192.168.1.0 mask 255.255.255.0', mode: 'Pool' },
  { feature: '池网关', topic: 'dhcp', ios: 'default-router 192.168.1.1', vrp: 'gateway-list 192.168.1.1', comware: 'gateway-list 192.168.1.1', mode: 'Pool' },

  // ACL
  { feature: '创建高级 ACL', topic: 'acl', ios: 'access-list 100 deny icmp 192.168.1.0 0.0.0.255 192.168.3.0 0.0.0.255', vrp: 'acl 3000', comware: 'acl advanced 3000', mode: 'Config' },
  { feature: 'ACL 加规则', topic: 'acl', ios: 'access-list 100 permit icmp 10.0.0.0 0.0.0.255 192.168.3.0 0.0.0.255', vrp: 'rule permit icmp source 10.0.0.0 0.0.0.255', comware: 'rule permit icmp source 10.0.0.0 0.0.0.255', mode: 'Acl', modeIos: 'Config' },
  // Stage 4A：Comware 官方写法是 packet-filter <acl> <dir>（E9）；VRP 才是 traffic-filter。
  // 该 Comware 命令在模拟器里只识别、未模拟，不显示完整支持。
  { feature: '接口应用 ACL', topic: 'acl', ios: 'ip access-group 100 in', vrp: 'traffic-filter inbound acl 3000', comware: 'packet-filter 3000 inbound', mode: 'If' },
  { feature: '查看 ACL', topic: 'acl', ios: 'show access-lists', vrp: 'display acl all', comware: 'display acl all', mode: 'Exec' },

  // NAT
  { feature: '标记 NAT 内口', topic: 'nat', ios: 'ip nat inside', vrp: 'nat outbound', comware: 'nat outbound', mode: 'If' },
  { feature: '标记 NAT 外口', topic: 'nat', ios: 'ip nat outside', vrp: 'nat outbound', comware: 'nat outbound', mode: 'If' },
  { feature: '静态 NAT', topic: 'nat', ios: 'ip nat inside source static 192.168.1.100 202.100.1.50', vrp: 'nat static global 202.100.1.50 inside 192.168.1.100', comware: 'nat static global 202.100.1.50 inside 192.168.1.100', mode: 'Config' },
  { feature: '查看转换表', topic: 'nat', ios: 'show ip nat translations', vrp: 'display nat session', comware: 'display nat session', mode: 'Exec' },

  // OSPF / VRRP
  { feature: '启动 OSPF 进程', topic: 'ospf', ios: 'router ospf 1', vrp: 'ospf 1', comware: 'ospf 1', mode: 'Config' },
  {
    feature: '宣告网段（区域 0）',
    topic: 'ospf',
    ios: 'network 10.1.0.0 0.0.0.3 area 0',
    vrp: 'network 10.1.0.0 0.0.0.3',
    comware: 'network 10.1.0.0 0.0.0.3',
    mode: 'Ospf',
    modeVrp: 'OspfArea',
    modeComware: 'OspfArea',
  },
  { feature: '查看 OSPF 邻居', topic: 'ospf', ios: 'show ip ospf neighbor', vrp: 'display ospf peer', comware: 'display ospf peer', mode: 'Exec' },
  { feature: 'VRRP 虚拟 IP', topic: 'ospf', ios: 'vrrp 1 ip 10.0.0.1', vrp: 'vrrp vrid 1 virtual-ip 10.0.0.1', comware: 'vrrp vrid 1 virtual-ip 10.0.0.1', mode: 'If' },
  { feature: 'VRRP 优先级', topic: 'ospf', ios: 'vrrp 1 priority 200', vrp: 'vrrp vrid 1 priority 200', comware: 'vrrp vrid 1 priority 200', mode: 'If' },
  { feature: '查看 VRRP 状态', topic: 'ospf', ios: 'show vrrp', vrp: 'display vrrp', comware: 'display vrrp', mode: 'Exec' },

  // BGP / route-policy
  { feature: '启动 BGP 进程', topic: 'bgp', ios: 'router bgp 65001', vrp: 'bgp 65001', comware: 'bgp 65001', mode: 'Config' },
  { feature: '配置 BGP 邻居', topic: 'bgp', ios: 'neighbor 10.0.0.2 remote-as 65002', vrp: 'peer 10.0.0.2 as-number 65002', comware: 'peer 10.0.0.2 as-number 65002', mode: 'Bgp' },
  { feature: '宣告 BGP 网段', topic: 'bgp', ios: 'network 192.168.1.0 mask 255.255.255.0', vrp: 'network 192.168.1.0 255.255.255.0', comware: 'network 192.168.1.0 255.255.255.0', mode: 'Bgp' },
  { feature: '创建路由策略', topic: 'bgp', ios: 'route-map FILTER deny 10', vrp: 'route-policy FILTER deny node 10', comware: 'route-policy FILTER deny node 10', mode: 'Config' },
  { feature: '策略匹配 ACL', topic: 'bgp', ios: 'match ip address 2000', vrp: 'if-match acl 2000', comware: 'if-match acl 2000', mode: 'RoutePolicy' },
  { feature: '设置 Local-Pref', topic: 'bgp', ios: 'set local-preference 200', vrp: 'apply local-preference 200', comware: 'apply local-preference 200', mode: 'RoutePolicy' },
  {
    feature: '邻居应用策略（入）',
    topic: 'bgp',
    ios: 'neighbor 10.0.0.1 route-map FILTER in',
    vrp: 'peer 10.0.0.1 route-policy FILTER import',
    comware: 'peer 10.0.0.1 route-policy FILTER import',
    mode: 'Bgp',
  },
  { feature: '查看 BGP 路由', topic: 'bgp', ios: 'show ip bgp', vrp: 'display bgp routing-table', comware: 'display bgp routing-table', mode: 'Exec' },
  { feature: '查看路由策略', topic: 'bgp', ios: 'show route-map', vrp: 'display route-policy', comware: 'display route-policy', mode: 'Exec' },

  // VRF / MPLS
  { feature: '创建 VRF', topic: 'mpls', ios: 'vrf definition CUST1', vrp: 'ip vpn-instance CUST1', comware: 'ip vpn-instance CUST1', mode: 'Config' },
  { feature: '接口绑定 VRF', topic: 'mpls', ios: 'ip vrf forwarding CUST1', vrp: 'ip binding vpn-instance CUST1', comware: 'ip binding vpn-instance CUST1', mode: 'If' },
  {
    feature: '静态 LSP（入口）',
    topic: 'mpls',
    ios: 'mpls static-lsp ingress to-ce2 destination 192.168.2.0 24 nexthop 10.0.0.2 out-label 100',
    vrp: 'static-lsp ingress to-ce2 destination 192.168.2.0 24 nexthop 10.0.0.2 out-label 100',
    comware: 'static-lsp ingress to-ce2 destination 192.168.2.0 24 nexthop 10.0.0.2 out-label 100',
    mode: 'Config',
  },
  {
    feature: '静态 LSP（中转）',
    topic: 'mpls',
    ios: 'mpls static-lsp transit t1 incoming-label 100 nexthop 10.0.1.2 out-label 200',
    vrp: 'static-lsp transit t1 incoming-label 100 nexthop 10.0.1.2 out-label 200',
    comware: 'static-lsp transit t1 incoming-label 100 nexthop 10.0.1.2 out-label 200',
    mode: 'Config',
  },
  { feature: '查看 LSP', topic: 'mpls', ios: 'show mpls lsp', vrp: 'display mpls lsp', comware: 'display mpls lsp', mode: 'Exec' },
  { feature: '查看 VRF 路由表', topic: 'mpls', ios: 'show ip route vrf CUST1', vrp: 'display ip routing-table vpn-instance CUST1', comware: 'display ip routing-table vpn-instance CUST1', mode: 'Exec' },

  // 防火墙 / IOS XE 17.x ZBF 真实对照。
  // 空字符串 = 该平台没有逐字对应命令，UI 显示「无逐字对应」且不调用 inspection。
  // Stage 4A：Comware 用 security-zone name / import interface；VRP 才是 firewall zone / add interface。
  // Stage 6A：防火墙基线 = 华为 VRP 安全策略。VRP 行已按规格升级为「完整支持」
  // （Implemented + DocumentationOnly + strict，见 specs.rs 与 stage6a_firewall.rs）；
  // Cisco ASA 的 nameif / security-level 已从命令树移除，不再出现在对照表；
  // Comware 行保持 Stage 4A 边界（区域/策略可解析、可显示，安全数据面不在本包）。
  // Stage 6B：IOS XE 17.x ZBF 已实现完整数据面（class-map → policy-map → zone-pair →
  // service-policy；inspect 状态化 / pass 无状态 / drop），Stage 3B 的简化
  // 旧简化 zone-pair 教学抽象（source/destination/action）已移除。
  { feature: '创建安全区域', topic: 'security', ios: 'zone security trust', vrp: 'firewall zone trust', comware: 'security-zone name trust', mode: 'Config' },
  {
    feature: '接口划入安全区域',
    topic: 'security',
    ios: 'zone-member security trust',
    vrp: 'add interface g0/0/0',
    comware: 'import interface g1/0/1',
    mode: 'If',
    modeVrp: 'Zone',
    modeComware: 'Zone',
  },
  { feature: 'IOS ZBF：定义检查类映射（class-map）', topic: 'security', ios: 'class-map type inspect match-any WEB', vrp: '', comware: '', mode: 'Config' },
  { feature: 'IOS ZBF：类映射匹配协议（match protocol）', topic: 'security', ios: 'match protocol icmp', vrp: '', comware: '', mode: 'ClassMap' },
  { feature: 'IOS ZBF：类映射匹配 ACL（match access-group）', topic: 'security', ios: 'match access-group 100', vrp: '', comware: '', mode: 'ClassMap' },
  { feature: 'IOS ZBF：创建检查策略映射（policy-map）', topic: 'security', ios: 'policy-map type inspect PMAP', vrp: '', comware: '', mode: 'Config' },
  { feature: 'IOS ZBF：策略映射引用类（class type inspect）', topic: 'security', ios: 'class type inspect WEB', vrp: '', comware: '', mode: 'PolicyMap' },
  { feature: 'IOS ZBF：状态化检查（inspect）', topic: 'security', ios: 'inspect', vrp: '', comware: '', mode: 'PolicyMapClass' },
  { feature: 'IOS ZBF：无状态放行（pass）', topic: 'security', ios: 'pass', vrp: '', comware: '', mode: 'PolicyMapClass' },
  { feature: 'IOS ZBF：丢弃命中流量（drop）', topic: 'security', ios: 'drop', vrp: '', comware: '', mode: 'PolicyMapClass' },
  { feature: 'IOS ZBF：创建区域对并指定方向（zone-pair）', topic: 'security', ios: 'zone-pair security IN-OUT source INSIDE destination OUTSIDE', vrp: '', comware: '', mode: 'Config' },
  { feature: 'IOS ZBF：区域对挂接检查策略（service-policy）', topic: 'security', ios: 'service-policy type inspect PMAP', vrp: '', comware: '', mode: 'SecurityRule' },
  { feature: 'IOS ZBF：查看区域对', topic: 'security', ios: 'show zone-pair security', vrp: '', comware: '', mode: 'Exec' },
  { feature: 'IOS ZBF：删除类映射', topic: 'security', ios: 'no class-map WEB', vrp: '', comware: '', mode: 'Config' },
  { feature: 'IOS ZBF：删除策略映射', topic: 'security', ios: 'no policy-map PMAP', vrp: '', comware: '', mode: 'Config' },
  { feature: 'IOS ZBF：删除区域对', topic: 'security', ios: 'no zone-pair security IN-OUT', vrp: '', comware: '', mode: 'Config' },
  { feature: 'IOS ZBF：解除区域对策略挂接', topic: 'security', ios: 'no service-policy type inspect PMAP', vrp: '', comware: '', mode: 'SecurityRule' },
  { feature: '创建区域间策略（VRP / Comware）', topic: 'security', ios: '', vrp: 'security-policy', comware: 'security-policy', mode: 'Config' },
  { feature: '策略源区域（VRP / Comware）', topic: 'security', ios: '', vrp: 'source-zone trust', comware: 'source-zone trust', mode: 'SecurityRule' },
  { feature: '策略目的区域（VRP / Comware）', topic: 'security', ios: '', vrp: 'destination-zone untrust', comware: 'destination-zone untrust', mode: 'SecurityRule' },
  { feature: '策略动作（VRP / Comware）', topic: 'security', ios: '', vrp: 'action permit', comware: 'action permit', mode: 'SecurityRule' },
  { feature: '查看安全区域', topic: 'security', ios: 'show zone security', vrp: 'display firewall zone', comware: 'display security-zone', mode: 'Exec' },
  { feature: '查看检查策略 / 安全策略', topic: 'security', ios: 'show policy-map type inspect zone-pair', vrp: 'display security-policy', comware: 'display security-policy', mode: 'Exec' },
  // Stage 6A 新增：会话表查询与清除（VRP 基线）；IOS/Comware 无逐字对应命令。
  { feature: '查看会话表（VRP 基线）', topic: 'security', ios: '', vrp: 'display firewall session table', comware: '', mode: 'Exec' },
  { feature: '清除会话表，使策略变更对既有流立即生效（VRP 基线）', topic: 'security', ios: '', vrp: 'reset firewall session table', comware: '', mode: 'Exec' },
  { feature: '启用 / 禁用安全策略规则（VRP 基线）', topic: 'security', ios: '', vrp: 'disable', comware: '', mode: 'SecurityRule' },
  { feature: '调整安全策略规则顺序（VRP 基线）', topic: 'security', ios: '', vrp: 'rule move allow-out before deny-all', comware: '', mode: 'SecurityPolicy' },
  { feature: '删除安全策略规则（VRP 基线）', topic: 'security', ios: '', vrp: 'undo rule name allow-out', comware: '', mode: 'SecurityPolicy' },
  { feature: '接口移出安全区域（VRP 基线）', topic: 'security', ios: '', vrp: 'undo add interface g0/0/0', comware: '', mode: 'Zone' },

  // IPsec / SSL VPN
  {
    feature: '创建 IPsec 隧道',
    topic: 'ipsec',
    ios: 'ipsec tunnel T1 local 10.0.0.1 remote 10.0.0.2',
    vrp: 'ipsec tunnel T1 local 10.0.0.1 remote 10.0.0.2',
    comware: 'ipsec tunnel T1 local 10.0.0.1 remote 10.0.0.2',
    mode: 'Config',
  },
  { feature: '本地保护子网', topic: 'ipsec', ios: 'local-subnet 192.168.1.0 24', vrp: 'local-subnet 192.168.1.0 24', comware: 'local-subnet 192.168.1.0 24', mode: 'Ipsec' },
  { feature: '远端保护子网', topic: 'ipsec', ios: 'remote-subnet 192.168.2.0 24', vrp: 'remote-subnet 192.168.2.0 24', comware: 'remote-subnet 192.168.2.0 24', mode: 'Ipsec' },
  { feature: '查看 IPsec 隧道', topic: 'ipsec', ios: 'show ipsec tunnel', vrp: 'display ipsec tunnel', comware: 'display ipsec tunnel', mode: 'Exec' },
  { feature: 'SSL VPN 虚拟地址池', topic: 'sslvpn', ios: 'sslvpn virtual-ip 10.10.0.0 24', vrp: 'sslvpn virtual-ip 10.10.0.0 24', comware: 'sslvpn virtual-ip 10.10.0.0 24', mode: 'Config' },
  { feature: '创建本地用户', topic: 'sslvpn', ios: 'username admin privilege 15 secret Admin@123', vrp: 'local-user admin password cipher Admin@123', comware: 'local-user admin password cipher Admin@123', mode: 'Config' },
  { feature: '查看 SSL VPN 会话', topic: 'sslvpn', ios: 'show sslvpn session', vrp: 'display sslvpn session', comware: 'display sslvpn session', mode: 'Exec' },

  // WLAN
  { feature: '配置 SSID', topic: 'wlan', ios: 'wlan ssid CORP', vrp: 'wlan ssid CORP', comware: 'wlan ssid CORP', mode: 'Config' },
  { feature: 'WPA2 预共享密钥', topic: 'wlan', ios: 'wlan security wpa2 psk Admin@123', vrp: 'wlan security wpa2 psk Admin@123', comware: 'wlan security wpa2 psk Admin@123', mode: 'Config' },
  { feature: 'WPA3-SAE 口令', topic: 'wlan', ios: 'wlan security wpa3 sae Guest@123', vrp: 'wlan security wpa3 sae Guest@123', comware: 'wlan security wpa3 sae Guest@123', mode: 'Config' },
  { feature: 'SSID 业务 VLAN', topic: 'wlan', ios: 'wlan vlan 10', vrp: 'wlan vlan 10', comware: 'wlan vlan 10', mode: 'Config' },
  { feature: 'AP 注册 AC', topic: 'wlan', ios: 'capwap ac 10.0.0.2', vrp: 'capwap ac 10.0.0.2', comware: 'capwap ac 10.0.0.2', mode: 'Config' },
  { feature: '查看 WLAN', topic: 'wlan', ios: 'show wlan', vrp: 'display wlan', comware: 'display wlan', mode: 'Exec' },

  // VXLAN / EVPN
  { feature: '配置本地 VTEP', topic: 'vxlan', ios: 'vxlan local 10.0.0.1', vrp: 'vxlan local 10.0.0.1', comware: 'vxlan local 10.0.0.1', mode: 'Config' },
  { feature: 'VNI 映射 VLAN', topic: 'vxlan', ios: 'vxlan vni 10000 vlan 10', vrp: 'vxlan vni 10000 vlan 10', comware: 'vxlan vni 10000 vlan 10', mode: 'Config' },
  { feature: '静态远端 VTEP', topic: 'vxlan', ios: 'vxlan peer 10.0.0.2', vrp: 'vxlan peer 10.0.0.2', comware: 'vxlan peer 10.0.0.2', mode: 'Config' },
  { feature: '启用 EVPN', topic: 'vxlan', ios: 'evpn enable', vrp: 'evpn enable', comware: 'evpn enable', mode: 'Config' },
  { feature: '配置 EVPN 邻居', topic: 'vxlan', ios: 'evpn peer 10.0.0.2', vrp: 'evpn peer 10.0.0.2', comware: 'evpn peer 10.0.0.2', mode: 'Config' },
  { feature: '查看 VXLAN', topic: 'vxlan', ios: 'show vxlan', vrp: 'display vxlan', comware: 'display vxlan', mode: 'Exec' },
  { feature: '查看 EVPN', topic: 'vxlan', ios: 'show evpn', vrp: 'display evpn', comware: 'display evpn', mode: 'Exec' },

  // 堆叠 / M-LAG
  { feature: '配置堆叠域', topic: 'ha', ios: 'stack domain 10', vrp: 'stack domain 10', comware: 'stack domain 10', mode: 'Config' },
  { feature: '配置堆叠成员号', topic: 'ha', ios: 'stack member 1', vrp: 'stack member 1', comware: 'stack member 1', mode: 'Config' },
  { feature: '配置主设备优先级', topic: 'ha', ios: 'stack priority 200', vrp: 'stack priority 200', comware: 'stack priority 200', mode: 'Config' },
  { feature: '配置 M-LAG 域', topic: 'ha', ios: 'mlag domain 1', vrp: 'mlag domain 1', comware: 'mlag domain 1', mode: 'Config' },
  { feature: '指定 M-LAG peer-link', topic: 'ha', ios: 'mlag peer-link', vrp: 'mlag peer-link', comware: 'mlag peer-link', mode: 'If' },
  { feature: '查看堆叠', topic: 'ha', ios: 'show stack', vrp: 'display stack', comware: 'display stack', mode: 'Exec' },
  { feature: '查看 M-LAG', topic: 'ha', ios: 'show mlag', vrp: 'display mlag', comware: 'display mlag', mode: 'Exec' },

  // 阶段 5A：LTM 只属于独立 F5 TMOS 17.x / tmsh。IOS / VRP / Comware 三列
  // 全部为空（无逐字对应命令），不再用跨厂商抽象命令伪装成真实厂商 CLI。
  { feature: '创建池并加入成员', topic: 'ltm', ios: '', vrp: '', comware: '', tmos: 'create ltm pool WEB members add { 10.0.1.10:80 }', mode: 'Exec' },
  { feature: '创建虚拟服务器', topic: 'ltm', ios: '', vrp: '', comware: '', tmos: 'create ltm virtual VS destination 10.0.0.100:80 pool WEB', mode: 'Exec' },
  { feature: '查看池配置', topic: 'ltm', ios: '', vrp: '', comware: '', tmos: 'list ltm pool /Common/WEB', mode: 'Exec' },
  { feature: '查看虚拟服务器配置', topic: 'ltm', ios: '', vrp: '', comware: '', tmos: 'list ltm virtual /Common/VS', mode: 'Exec' },

  // 园区综合
  { feature: 'Trunk 放行 VLAN', topic: 'campus', ios: 'switchport trunk allowed vlan 10,20', vrp: 'port trunk allow-pass vlan 10 20', comware: 'port trunk permit vlan 10 20', mode: 'If', role: 'switch' },
  { feature: '配置默认路由', topic: 'campus', ios: 'ip route 0.0.0.0 0.0.0.0 10.0.0.1', vrp: 'ip route-static 0.0.0.0 0 10.0.0.1', comware: 'ip route-static 0.0.0.0 0 10.0.0.1', mode: 'Config' },
  { feature: '查看 NAT 会话', topic: 'campus', ios: 'show ip nat translations', vrp: 'display nat session', comware: 'display nat session', mode: 'Exec' },

  // 厂商对照：链路聚合
  { feature: '成员口加入聚合', topic: 'lag', ios: 'channel-group 1 mode active', vrp: 'eth-trunk 1', comware: 'port link-aggregation group 1', mode: 'If' },
  { feature: '查看聚合状态', topic: 'lag', ios: 'show etherchannel summary', vrp: 'display eth-trunk 1', comware: 'display link-aggregation verbose', mode: 'Exec' },

  // 厂商对照：堆叠
  { feature: '配置堆叠域', topic: 'stack', ios: 'stack domain 10', vrp: 'stack domain 10', comware: 'stack domain 10', mode: 'Config' },
  { feature: '配置成员编号', topic: 'stack', ios: 'stack member 1', vrp: 'stack member 1', comware: 'stack member 1', mode: 'Config' },
  { feature: '配置主设备优先级', topic: 'stack', ios: 'stack priority 200', vrp: 'stack priority 200', comware: 'stack priority 200', mode: 'Config' },
  { feature: '查看堆叠状态', topic: 'stack', ios: 'show stack', vrp: 'display stack', comware: 'display stack', mode: 'Exec' },

  // 厂商对照：跨设备聚合
  { feature: '配置 M-LAG 域', topic: 'mlag', ios: 'mlag domain 1', vrp: 'mlag domain 1', comware: 'mlag domain 1', mode: 'Config' },
  { feature: '指定 peer-link', topic: 'mlag', ios: 'mlag peer-link', vrp: 'mlag peer-link', comware: 'mlag peer-link', mode: 'If' },
  { feature: '查看 M-LAG 状态', topic: 'mlag', ios: 'show mlag', vrp: 'display mlag', comware: 'display mlag', mode: 'Exec' },

  // 厂商对照：首跳冗余（模拟器实现 VRRP）
  { feature: '配置虚拟网关 IP', topic: 'fhrp', ios: 'vrrp 1 ip 10.0.0.1', vrp: 'vrrp vrid 1 virtual-ip 10.0.0.1', comware: 'vrrp vrid 1 virtual-ip 10.0.0.1', mode: 'If' },
  { feature: '配置优先级', topic: 'fhrp', ios: 'vrrp 1 priority 200', vrp: 'vrrp vrid 1 priority 200', comware: 'vrrp vrid 1 priority 200', mode: 'If' },
  { feature: '开启抢占', topic: 'fhrp', ios: 'vrrp 1 preempt', vrp: 'vrrp vrid 1 preempt-mode', comware: 'vrrp vrid 1 preempt-mode', mode: 'If' },
  { feature: '查看主备状态', topic: 'fhrp', ios: 'show vrrp', vrp: 'display vrrp', comware: 'display vrrp', mode: 'Exec' },

  // IA：IPv4 地址
  { feature: '配置接口 IPv4 地址', topic: 'ipv4', ios: 'ip address 192.168.1.1 255.255.255.0', vrp: 'ip address 192.168.1.1 24', comware: 'ip address 192.168.1.1 24', mode: 'If' },
  { feature: '查看接口摘要', topic: 'ipv4', ios: 'show ip interface brief', vrp: 'display ip interface brief', comware: 'display ip interface brief', mode: 'Exec' },
  { feature: '连通性测试', topic: 'ipv4', ios: 'ping 192.168.1.2', vrp: 'ping 192.168.1.2', comware: 'ping 192.168.1.2', mode: 'Exec' },

  // IA：静态路由
  { feature: '配置静态路由', topic: 'routing', ios: 'ip route 192.168.2.0 255.255.255.0 10.0.0.2', vrp: 'ip route-static 192.168.2.0 24 10.0.0.2', comware: 'ip route-static 192.168.2.0 24 10.0.0.2', mode: 'Config' },
  { feature: '配置默认路由', topic: 'routing', ios: 'ip route 0.0.0.0 0.0.0.0 10.0.0.1', vrp: 'ip route-static 0.0.0.0 0 10.0.0.1', comware: 'ip route-static 0.0.0.0 0 10.0.0.1', mode: 'Config' },
  { feature: '查看路由表', topic: 'routing', ios: 'show ip route', vrp: 'display ip routing-table', comware: 'display ip routing-table', mode: 'Exec' },

  // IA：IPv6
  { feature: '配置 IPv6 地址', topic: 'ipv6', ios: 'ipv6 address 2001:db8:10::1/64', vrp: 'ipv6 address 2001:db8:10::1/64', comware: 'ipv6 address 2001:db8:10::1/64', mode: 'If' },
  { feature: 'IPv6 静态路由', topic: 'ipv6', ios: 'ipv6 route 2001:db8:20::/64 2001:db8:10::2', vrp: 'ipv6 route-static 2001:db8:20:: 64 2001:db8:10::2', comware: 'ipv6 route-static 2001:db8:20:: 64 2001:db8:10::2', mode: 'Config' },
  { feature: '查看 IPv6 接口', topic: 'ipv6', ios: 'show ipv6 interface brief', vrp: 'display ipv6 interface brief', comware: 'display ipv6 interface brief', mode: 'Exec' },
  { feature: '查看 IPv6 路由表', topic: 'ipv6', ios: 'show ipv6 route', vrp: 'display ipv6 routing-table', comware: 'display ipv6 routing-table', mode: 'Exec' },

  // IA：网络服务与管理
  { feature: '创建本地管理员', topic: 'services', ios: 'username admin privilege 15 secret Admin@123', vrp: 'local-user admin password cipher Admin@123', comware: 'local-user admin password cipher Admin@123', mode: 'Config' },
  { feature: 'VTY 只允许 SSH', topic: 'services', ios: 'transport input ssh', vrp: 'protocol inbound ssh', comware: 'protocol inbound ssh', mode: 'Line' },
  { feature: '查看接口状态', topic: 'services', ios: 'show ip interface brief', vrp: 'display ip interface brief', comware: 'display ip interface brief', mode: 'Exec' },

  // IE：IS-IS
  { feature: '启动 IS-IS 进程', topic: 'isis', ios: 'router isis', vrp: 'isis 1', comware: 'isis 1', mode: 'Config' },
  { feature: '配置 NET', topic: 'isis', ios: 'net 49.0001.0000.0000.0001.00', vrp: 'network-entity 49.0001.0000.0000.0001.00', comware: 'network-entity 49.0001.0000.0000.0001.00', mode: 'Isis' },
  { feature: '接口启用 IS-IS', topic: 'isis', ios: 'ip router isis', vrp: 'isis enable', comware: 'isis enable', mode: 'If' },
  { feature: '查看 IS-IS 邻居', topic: 'isis', ios: 'show isis neighbors', vrp: 'display isis peer', comware: 'display isis peer', mode: 'Exec' },

  // IE：QoS
  { feature: '广播风暴抑制', topic: 'qos', ios: 'storm-control broadcast level 100', vrp: 'broadcast-suppression 100', comware: 'broadcast-suppression 100', mode: 'If' },
  { feature: '入方向限速', topic: 'qos', ios: 'rate-limit input 1000', vrp: 'qos car inbound cir 1000', comware: 'qos lr inbound cir 1000', mode: 'If' },
  { feature: '查看 QoS 统计', topic: 'qos', ios: 'show storm-control', vrp: 'display qos', comware: 'display qos', mode: 'Exec' },

  // IA：运维入门
  { feature: '连通性测试', topic: 'ops', ios: 'ping 10.0.0.2', vrp: 'ping 10.0.0.2', comware: 'ping 10.0.0.2', mode: 'Exec' },
  { feature: '查看接口摘要', topic: 'ops', ios: 'show ip interface brief', vrp: 'display ip interface brief', comware: 'display ip interface brief', mode: 'Exec' },
  { feature: '查看路由表', topic: 'ops', ios: 'show ip route', vrp: 'display ip routing-table', comware: 'display ip routing-table', mode: 'Exec' },

  // 同 VLAN 隔离
  { feature: '端口隔离', topic: 'isolation', ios: 'switchport protected', vrp: 'port-isolate enable group 1', comware: 'port-isolate enable group 1', mode: 'If' },
  { feature: 'MUX VLAN principal', topic: 'isolation', ios: 'mux-vlan principal', vrp: 'mux-vlan principal', comware: 'mux-vlan principal', mode: 'If' },
  { feature: 'MUX VLAN group', topic: 'isolation', ios: 'mux-vlan group 1', vrp: 'mux-vlan group 1', comware: 'mux-vlan group 1', mode: 'If' },
  { feature: '查看端口隔离', topic: 'isolation', ios: 'show port-isolate', vrp: 'display port-isolate', comware: 'display port-isolate', mode: 'Exec' },
  { feature: '查看 MUX VLAN', topic: 'isolation', ios: 'show mux-vlan', vrp: 'display mux-vlan', comware: 'display mux-vlan', mode: 'Exec' },

  // OSPFv3
  { feature: '启动 OSPFv3', topic: 'ospfv3', ios: 'ipv6 router ospf 1', vrp: 'ospfv3 1', comware: 'ospfv3 1', mode: 'Config' },
  { feature: '设置 Router ID', topic: 'ospfv3', ios: 'router-id 1.1.1.1', vrp: 'router-id 1.1.1.1', comware: 'router-id 1.1.1.1', mode: 'Ospfv3' },
  { feature: '接口启用 OSPFv3', topic: 'ospfv3', ios: 'ipv6 ospf 1 area 0', vrp: 'ospfv3 1 area 0', comware: 'ospfv3 1 area 0', mode: 'If' },
  { feature: '查看 OSPFv3 邻居', topic: 'ospfv3', ios: 'show ipv6 ospf neighbor', vrp: 'display ospfv3 peer', comware: 'display ospfv3 peer', mode: 'Exec' },

  // SRv6 BE
  { feature: '配置 Locator', topic: 'srv6', ios: 'srv6 locator 2001:db8:100::/64', vrp: 'srv6 locator 2001:db8:100::/64', comware: 'srv6 locator 2001:db8:100::/64', mode: 'Config' },
  { feature: '配置本地 SID', topic: 'srv6', ios: 'srv6 sid 2001:db8:100::1', vrp: 'srv6 sid 2001:db8:100::1', comware: 'srv6 sid 2001:db8:100::1', mode: 'Config' },
  { feature: '静态 SRv6 路由', topic: 'srv6', ios: 'srv6 route 2001:db8:2::/64 sid 2001:db8:200::1', vrp: 'srv6 route 2001:db8:2::/64 sid 2001:db8:200::1', comware: 'srv6 route 2001:db8:2::/64 sid 2001:db8:200::1', mode: 'Config' },
  { feature: '查看 SRv6', topic: 'srv6', ios: 'show srv6', vrp: 'display srv6', comware: 'display srv6', mode: 'Exec' },
  // ---------- 远程管理（VTY / SSH / 本地用户） ----------
  { feature: '进入 VTY 线路', topic: 'mgmt', ios: 'line vty 0 4', vrp: 'user-interface vty 0 4', comware: 'line vty 0 4', mode: 'Config' },
  { feature: '设置登录认证为本地', topic: 'mgmt', ios: 'login local', vrp: 'authentication-mode aaa', comware: 'authentication-mode aaa', mode: 'Line' },
  { feature: '限制登录协议为 SSH', topic: 'mgmt', ios: 'transport input ssh', vrp: 'protocol inbound ssh', comware: 'protocol inbound ssh', mode: 'Line' },
  { feature: '创建本地用户', topic: 'mgmt', ios: 'username admin secret cisco123', vrp: 'local-user admin password cipher cisco123', comware: 'local-user admin password cipher cisco123', mode: 'Config' },
  { feature: 'SSH 登录对端', topic: 'mgmt', ios: 'ssh 10.0.0.2', vrp: 'ssh 10.0.0.2', comware: 'ssh 10.0.0.2', mode: 'Exec' },
  { feature: '查看当前配置', topic: 'basics', ios: 'show running-config', vrp: 'display current-configuration', comware: 'display current-configuration', mode: 'Exec' },
];
