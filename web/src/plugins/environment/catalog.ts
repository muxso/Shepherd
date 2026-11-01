// Display categories are shared by the catalog and environment configuration.
// Runtime descriptors remain the authority for whether a provider is available.
export const environmentGroups = [
  { key: 'data', name: '数据连接', items: [
    { section: 'database', name: '数据库', description: 'PostgreSQL、MySQL 连接与查询' },
    { section: 'redis', name: 'Redis 配置', description: 'Redis 连接与命令执行' },
    { section: 'mongodb', name: 'MongoDB 配置', description: '文档数据库连接与查询' },
  ] },
  { key: 'network', name: '网络与协议', items: [
    { section: 'host', name: 'HOST', description: '服务端 HTTP 调试域名解析覆盖' },
    { section: 'ssh', name: 'SSH配置', description: '主机连接、固定主机指纹与远程命令' },
    { section: 'amqp', name: 'AMQP配置', description: '消息代理连接与消息操作' },
    { section: 'tcp', name: 'TCP配置', description: 'TCP 连接与报文操作' },
    { section: 'grpc', name: 'GRPC配置', description: 'gRPC 连接与一元 RPC 调用' },
  ] },
  { key: 'processing', name: '处理与校验', items: [
    { section: 'pre', name: '前置', description: '服务端 HTTP 调试前置脚本与准备步骤' },
    { section: 'post', name: '后置', description: '请求后的处理与清理' },
    { section: 'assert', name: '断言', description: '响应与业务结果校验' },
  ] },
  { key: 'display', name: '界面扩展', items: [
    { section: 'display', name: '显示设置', description: '插件结果的展示偏好' },
  ] },
] as const
export const environmentSections = environmentGroups.flatMap(group => [...group.items])
