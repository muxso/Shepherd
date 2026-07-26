# 需求模板

标题: 用户登录
描述: 支持邮箱 + 密码登录,签发会话令牌。

验收标准:
- 正确凭证返回 token
- 错误凭证返回 401
- 令牌过期后需重新登录

改好后录入(需先 `shepherd login`):

    shepherd req add --project <projectId> \
      --title "用户登录" \
      --description "支持邮箱 + 密码登录,签发会话令牌。" \
      --criteria "正确凭证返回 token,错误凭证返回 401,令牌过期后需重新登录"
