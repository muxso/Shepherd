# README 资源 / README assets

放置 README 首页引用的图片。文件名固定,README 里已写好占位,放好后取消对应注释行即可。

Drop the images referenced by the homepage README here. The filenames are fixed and the
README already has the placeholders — just uncomment the matching line once the file exists.

| 文件 / file | 用途 / used for | 建议 / suggested |
|---|---|---|
| `demo.gif` | 首屏 ≤20s 闭环动图 / hero loop GIF | ≤ 8 MB,宽 ≤ 1000px,12–15 fps |
| `dashboard.png` | Web 控制台 / 机群视图截图 / console & fleet view | 深色主题,1600×1000 左右 |

## 怎么录 demo.gif / how to record demo.gif

目标:20 秒内讲完一条闭环 —— **提需求 → AI 起草设计 → 你批准 → 自动拆分 → 执行机认领 → 交付 → 过验证门 → 签收**。

A 20-second story: **file a requirement → AI drafts a design → you approve → auto-breakdown
→ executor claims → delivery → clears the verification gate → sign off.**

1. **起栈 / bring the stack up**(用 mock 执行机,免真实 CLI、快且可复现):

   ```bash
   docker run -d --name shep-pg \
     -e POSTGRES_USER=msuser -e POSTGRES_PASSWORD=mspass -e POSTGRES_DB=mstest \
     -p 55432:5432 postgres:16-alpine

   DATABASE_URL=postgres://msuser:mspass@localhost:55432/mstest \
   SHEPHERD_ADMIN_PASSWORD=s3cret SHEPHERD_AGENT_FLEET=1 cargo run        # server :8088

   AGENT_MOCK=1 SHEPHERD_BASE=http://127.0.0.1:8088 \
   SHEPHERD_CAPS=CLAUDE_CODE cargo run -p agent-runtime             # mock executor

   cd web && npm install && npm run dev                            # console
   ```

2. **录屏 / record**:走一遍控制台里的需求→设计审批→拆分→交付→验证;突出**两道门**那两次"批准"点击。
   Walk the console through requirement → design approval → breakdown → delivery → verification;
   highlight the two **"Approve"** clicks at the gates.

3. **转 GIF / convert**(任选其一 / pick one):

   ```bash
   # macOS 屏录(.mov)→ GIF
   ffmpeg -i screen.mov -vf "fps=14,scale=1000:-1:flags=lanczos" -loop 0 demo.gif
   # 体积大就再压一道 / shrink further with gifsicle
   gifsicle -O3 --lossy=60 demo.gif -o demo.gif
   ```

4. 放到本目录,把 `README.md` / `README.zh-CN.md` 里对应的 `<!-- ![..](docs/assets/..) -->` 取消注释、删掉占位块。
   Drop it here, then uncomment the `<!-- ![..] -->` line in both READMEs and remove the placeholder block.

> 提示:GitHub 首屏动图 ≤ 10 MB 才会自动播放;超了会变成需点击。
> Tip: GitHub auto-plays README GIFs only under ~10 MB; larger ones become click-to-load.
