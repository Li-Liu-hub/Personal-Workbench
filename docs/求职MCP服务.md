# 求职 MCP 服务

三个求职工具同时作为供 AI 助手调用的 Rust MCP 服务提供。日常入口仍为 `dist/个人办公台.exe`，Codex 以 `--mcp` 参数启动同一程序的后台 stdio 模式，不创建桌面窗口，也不启动 Python 或 Node。MCP 模式在桌面单实例检查之前分流，可与桌面同时运行。

## 注册和连接

运行 `scripts/register-hiring-mcp.ps1`，会备份现有 Codex 配置，再注册 `personaldesk_hiring`。等效命令是：

```powershell
codex mcp add personaldesk_hiring -- "C:\Users\20886\Desktop\个人办公台\dist\个人办公台.exe" --mcp
codex mcp get personaldesk_hiring --json
```

服务沿用程序旁的 `personal-desk-data-path.txt`，在其中的 `hiring/` 读写配置、资料和投递记录。测试或其他客户端可以使用 `--mcp --data-dir <绝对数据目录>`，这里是办公台数据目录，程序会追加 `hiring/`。

已有会话若尚未显示新增工具，重新加载 MCP 连接或重新打开会话。配置文件注册成功和当前会话已经装载工具是两个独立状态，以客户端实际显示为准。

## 调用方式

首次使用：调用 `hiring_import`，参数 `source_directory` 指向包含原三个工具的文件夹；它保留原始文件，复制资料、配置和历史状态，旧待投队列默认跳过。`hiring_status` 可检查资料和队列状态。

| 能力 | 工具 |
| --- | --- |
| 状态与资料 | `hiring_status`、`hiring_import`、`hiring_settings_get`、`hiring_settings_update`、`hiring_history` |
| BOSS 登录和搜索 | `boss_open_login`、`boss_check_login`、`boss_search_jobs`、`boss_scroll_jobs`、`boss_job_detail` |
| BOSS 投递队列 | `boss_apply`、`boss_queue_list`、`boss_queue_add`、`boss_queue_update`、`boss_queue_run`、`boss_queue_stop` |
| 官网填写与投递 | `official_open`、`official_inspect`、`official_profile_check`、`official_plan`、`official_autofill`、`official_submit`、`official_otp_fill` |
| 接码 | `otp_start`、`otp_status`、`otp_stop` |
| 多账号 | `hiring_accounts_list`、`hiring_account_create`、`hiring_account_update`、`hiring_account_select` |
| 共享去重与分配 | `hiring_dedupe_check`、`hiring_attempt_resolve`、`boss_pool_plan`、`boss_pool_run` |

## 三个账号与15天共享去重

服务提供34个工具。最多创建三个固定账号槽位，各有独立浏览器目录、登录态、每日计数与30–60秒投递间隔。原登录态保留为“账号1”；新增账号必须分别扫码登录，程序不复制登录态。使用 `boss_open_login`、`boss_check_login` 的 `account_id` 指定账号，先检查每个账号登录，再运行队列。账号名称可改、账号可停用，历史与额度不会因更名、切换或重新启用而重置。

每个账号每日最多150项，三个账号合计最多450项；实际可投数量取决于剩余额度、合格岗位和网站状态。预留后结果不明或部分发送的记录仍占用当日额度，避免重启后重复发送。遇到网站风险提示或发送结果不明确，整组请求停止，其他账号不用于继续该受阻任务。

岗位指纹由“公司名称、岗位名称、完整岗位简介”三项共同生成。规范化会处理大小写、空白、零宽字符与全角ASCII字符，保留标点和语言名称差异。同公司不同岗位、不同简介不因公司名称相同而直接合并；同一详情链接也作为重复依据。已确认成功的投递在三个账号之间共享15×24小时冷却，严格超过15天后才允许重新入队。历史保留，新一次投递另写记录与当时的岗位快照。

旧数据迁移只增加账号、身份与快照表，保留原队列和投递记录。旧记录若缺少有效秒级时间，采用原日期的日末；连日期也缺失时从首次迁移时间保守计算，不能把未知时间当成1970年立即重投。旧简介缺失时仍以原链接阻止重复；无法恢复的数据不会伪造成完整简介。

`reserved`、`ambiguous`、`partial` 不会因超过15天自动解除。先在对应账号网页核验；只有实际核验过才能调用 `hiring_attempt_resolve` 并传 `verified_on_site=true, confirmed=true`。文字已确认发送的部分记录不能标成“未发送”来重发文字。

先调用 `boss_pool_plan`，传三个 `account_ids`、`limit=450`、`dry_run=true`，查看去重后各账号分配数量；预览不改队列归属、不发送。已获用户明确授权时调用 `boss_pool_run` 并传 `confirmed=true`，各项执行前仍预检。岗位在数据库事务中领取、预留和记账；两个账号同时遇到同一岗位只会有一个获得发送资格。实际执行可能少于预览数量，进度以已确认的文字与图片发送结果为准。

`boss_queue_stop` 会向共享数据库写停止标记，可停止其他MCP会话或桌面启动的同组队列；停止等待每个账号当前项核验结束。桌面账号管理提供同样的切换、登录、分配预览、执行与待核验记录处理入口。工具数据导出包含账号和去重历史，不含各账号浏览器登录态、短信或接码密钥。

`boss_apply` 和 `official_submit` 默认只预检，只有用户已明确授权时，助手才应传 `dry_run=false, confirmed=true`。BOSS 发送沿用招呼语、图片、时段、每日限额、最小间隔、岗位核验和重复发送保护。官网正式提交必须有五分钟内的有效预检，用户需先检查页面资料和条款。结果不明确时保留待核验记录，不能自动重试。

后台队列通过 `boss_queue_run` 启动后立即返回任务编号，使用 `hiring_status` 查询进度，`boss_queue_stop` 请求停止。停止会等待当前一项的发送核验完成，退出 MCP 时也会请求停止。桌面与 MCP 使用同一浏览器互斥文件，一个任务持有期间其他浏览器任务返回占用提示；操作系统在进程退出后释放占用，不靠删除锁文件解锁。

接码服务可由桌面或 MCP 启动，另一方在同一数据目录内可使用已有服务。`otp_stop` 只停止当前 MCP 会话启动的服务。手机号端 Webhook 地址保存在权限受限的 `otp_url.txt`，工具只返回其路径；短信正文、验证码和 token 不进入 MCP 返回。`official_otp_fill` 在进程内取码后直接填入网页。

## 协议和验证边界

使用标准 MCP JSON-RPC stdio，以换行分隔 UTF-8 消息，支持初始化、工具发现、调用及 ping。参数按公开 schema 校验，业务失败返回 `isError=true`，协议错误返回 JSON-RPC 错误。浏览器子进程的标准输出不继承协议通道。

没有暴露任意 JavaScript、任意点击、原始验证码读取或绕过限额的接口。网页文字作为不可信数据处理，表单查看接口只返回字段结构和已填状态。登录、滑块、扫码、跨域表单和歧义字段仍需在浏览器处理。本地模拟投递成功不等于真实招聘网站验收。
