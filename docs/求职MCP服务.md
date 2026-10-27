# 求职 MCP 服务

求职套件同时作为供 AI 助手调用的 Rust MCP 服务提供。日常入口仍为 `dist/个人办公台.exe`，Codex 以 `--mcp` 参数启动同一程序的后台 stdio 模式，不创建桌面窗口，也不启动 Python 或 Node。MCP 模式在桌面单实例检查之前分流，可与桌面同时运行。

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

服务提供49个工具。最多创建三个固定账号槽位，各有独立浏览器目录、登录态、每日计数与30–60秒投递间隔。原登录态保留为“账号1”；新增账号必须分别扫码登录，程序不复制登录态。使用 `boss_open_login`、`boss_check_login` 的 `account_id` 指定账号，先检查每个账号登录，再运行队列。账号名称可改、账号可停用，历史与额度不会因更名、切换或重新启用而重置。

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

## 前程无忧、智联招聘、实习僧与统一队列（0.15.32）

所有平台使用同一个 `hiring/hiring.sqlite`：`queue` 是唯一岗位队列表，`platform` 标识来源；`attempts` 是统一投递记录表，`site` 标识实际投递来源。平台页仅筛选同一张表，未拆分平台数据库或业务队列表。升级给原队列表增加带默认值的来源列，原BOSS记录和历史保持；全量导出仍保存这一份数据库。

`hiring_platforms` 列出平台，`platform` 参数为 `51job`、`zhaopin`、`shixiseng`。`hiring_queue_list` 返回全平台队列，可按 `platform`（含 `boss`）和 `status` 筛选。`hiring_status` 的队列数量包含全部平台，返回共用表名和15天规则。

| 能力 | 接口 |
| --- | --- |
| 登录与状态 | `platform_open_login`、`platform_check_login` |
| 搜索与详情 | `platform_search_jobs`、`platform_scroll_jobs`、`platform_job_detail` |
| 单项投递 | `platform_apply` |
| 共用队列的来源筛选与维护 | `platform_queue_list`、`platform_queue_add`、`platform_queue_update` |
| 后台执行与停止 | `platform_queue_run`、`platform_queue_stop` |
| 全平台核验 | `hiring_dedupe_check`、`hiring_attempt_resolve` |

先打开相应平台浏览器，由用户登录、完善并核对在线简历。三个平台的浏览器登录态分别保存在各自的目录；BOSS原三账号保持。搜索覆盖关键词、城市、页码，其余条件在网页筛选后调用 `platform_scroll_jobs` 读取；未配置城市编码会明确报错，可以选全国再在网页选择。遇到登录、验证码、网页加密字体或结构变化时停止，不把读取失败当作零岗位。

`platform_apply` 默认 `dry_run=true`：读取完整岗位、检查去重和表单、保存五分钟内有效的网页摘要，不发送。用户明确授权后才传 `dry_run=false,confirmed=true`，并要求网页、岗位、简历选择及可见输入内容未变化。使用当前网页中的简历投递，程序不替用户选择不明确的简历或同意条款。如果点击后出现简历选择、多步确认或无法验证成功的窗口，会保留待核验记录并停止；请在网页处理及核验，禁止自动重试。正式投递没有在用户真实账号上自动验收。

15天窗口在所有平台和账号间共用。公司、岗位、完整简介规范化后的指纹及原链接共同比较；同公司同名岗位在另一平台简介不同、或旧记录简介缺失时，保守拦截并要求核验。预留和判断在同一个SQLite写事务中完成，多平台同时请求同一岗位只能有一个成功预留。`reserved/ambiguous/partial` 不自动过期；成功投递严格超过15天才可再次申请，旧历史不删除。不得将一个平台的未知发送结果转移到另一个平台重试。

各平台从同一设置读取投递时段、每日上限及最短间隔；同一个账号槽位跨平台合计额度，不可切换平台来清零额度或缩短间隔。既有BOSS独立账号仍分别计数。`platform_queue_stop` 和 `boss_queue_stop` 写同一停止标记，均可停止桌面或MCP后台队列。

验证：`cargo test --locked --workspace --all-targets`；真实浏览器的本地拦截验证使用 `python scripts/verify-hiring-platforms.py --executable <隔离构建程序>`，需要 `websocket-client` 与Chrome，全部招聘页面请求在测试浏览器内被本地HTML拦截，真实网站提交次数为零。

0.15.32验证范围：格式与Clippy检查通过；完整Rust回归370项通过、24项因需要模型、字体或真实网站环境而忽略，辅助脚本13项通过。招聘测试覆盖四线程并行采集、三轮补足总额度、来源账号入库、逆序入库仍保留BOSS、四档来源优先级、15天边界、跨平台并发预留与重启后保护；三个新平台的本地浏览器验证覆盖搜索、详情、预检、简历选择变化、确认投递、重复拦截和验证码暂停。未在真实招聘账号上发送申请，真实网站改版和多步投递仍需登录后验收。

### 一次启动四个平台的自动循环

主入口为“小工具 → 求职套件 → 开始自动求职”，AI入口为 `hiring_run`。默认 `dry_run=true` 只并行搜索、读取详情、入库及筛选，不发送申请；正式自动循环必须先有用户明确授权，再传 `dry_run=false,confirmed=true`。`daily_limit` 覆盖本轮今日总上限（1–150），未传则沿用同一份设置的 `daily_limit`；`max_pages` 为每个平台本次搜索页数保护（1–50）。任务立即返回编号，通过 `hiring_status` 查看进展，用 `hiring_stop` 停止。

每轮同时启动四个平台的采集，读取详情并汇总后，在唯一的 `queue` 表中保存来源 `platform` 和 `queue_identity.account_id`，序列化岗位中也携带 `platform/account_id`。BOSS使用当前所选独立账号，其余平台使用各自的账号1登录态；“平台+账号槽位”标识来源。新发现记录为 `candidate`，统一筛选后分别标成 `pending` 或 `duplicate`；重复来源保留并注明保留链接。

固定保留顺序：**BOSS直聘 → 智联招聘 → 前程无忧 → 实习僧**。每轮全部结果汇总后才选来源，与网络返回速度及入库顺序无关。同公司同名跨平台岗位文案不同也先合并候选，已投或待核验记录继续全平台拦截。较低优先级的单项投递也会检查队列里的优先来源，不能绕开统一筛选。

投递前逐项预检，统一累计当天 `attempts` 中的已占用次数（包括结果不明的预留），达到 `daily_limit` 即停止。候选不够就搜索下一页并重复入库、筛选和投递；连续两轮无新岗位、搜索到页数保护、日期变化、平台未完成搜索或发生人工验证时暂停并说明原因，不无限重复同一页，也不声称额度一定能填满。搜索失败时保留已采集来源，停止该轮投递以免破坏来源优先级。
