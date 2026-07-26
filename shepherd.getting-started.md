# Shepherd 上手

前置:一个运行中的 server(默认 http://localhost:8088)。

1. 配置认证:`shepherd login --url http://localhost:8088 --api-key sak_…`
   (API key 在 个人中心 → API KEY 或 `POST /system/apikey` 签发;也可设环境变量 SHEPHERD_API_KEY)
2. 录入需求:见 `requirements/example.md`
3. 拆分任务:`shepherd decompose --req <requirementId> --version 1`
4. 派发执行:`shepherd dispatch --decomp <decompositionId> --task <taskId> --executor CLAUDE_CODE`
5. 验证 / 复查:`shepherd verify --help`、`shepherd decomposition --help`

各命令的完整参数见 `shepherd <命令> --help`。
