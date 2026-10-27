# BOSS直聘自动投递

这是 `个人办公台` 中的 BOSS 直聘工具副本，保留完整 CLI、搜索、详情、投递、图片发送、投递队列和可选的 MCP 入口。

> **当前状态说明**
>
> 旧目录 `C:\Users\20886\Desktop\Boss工具\BossHunter` 里的投递队列仍在运行。本目录是复制件：
>
> - 没有复制 Chrome 登录态和浏览器 profile；
> - 没有复制旧版遗留的 `.config.credentials.yaml`；
> - 队列和状态是复制时的快照，不代表实时进度。
>
> 在旧队列停止前，**不要用本目录对同一个 BOSS 账号再跑一次队列**，否则可能重复投递。切换前先停旧队列、在本目录重新登录并核验账号。

## 能做什么

- 按关键词、城市、经验、学历筛选 BOSS 岗位，读取列表和详情。
- 按配置的招呼语和图片投递，支持文字 + 图片。
- 投递队列：增删改、批量执行、断点续投、失败重试。
- 安全状态：每日投递上限、页面预算、发送时间窗口、30–60 秒随机间隔、连续失败熔断、投递去重。
- 可选 MCP stdio 服务，但日常建议直接用本目录的 CLI。

## 目录

```
BOSS直聘/
├── bosshunter.bat          # 日常 CLI 入口（带 PYTHONPATH，不需要 pip install）
├── config.yaml             # 当前配置
├── config.example.yaml     # 配置模板
├── data/
│   ├── apply_queue.json    # 投递队列
│   ├── state.json          # 每日计数与安全状态
│   ├── bosshunter.db       # 历史记录
│   ├── resumes/            # 简历 MD/PDF/图片
│   └── chrome-profile/     # 首次登录后生成，不在副本中
├── src/bosshunter/         # 源码
└── tests/                  # 单元测试
```

## 环境

- Windows + Python 3.10+
- Google Chrome 或 Microsoft Edge
- 一个可登录 BOSS 直聘的账号
- 不需要安装本副本为 pip 包，`bosshunter.bat` 会设置 `PYTHONPATH` 指向 `src`

## 快速开始

```bat
cd /d C:\Users\20886\Desktop\个人办公台\BOSS直聘

bosshunter.bat status
bosshunter.bat login
bosshunter.bat --json check-login
```

### 首次登录与账号核验

本副本使用自己的专用 Chrome 配置目录 `data/chrome-profile`，不会影响日常浏览器。

1. 运行 `bosshunter.bat login`，在弹出的 Chrome 窗口扫码/短信登录。
2. 运行 `bosshunter.bat --json check-login`，确认 `logged_in` 为真。
3. **核对真实账号 userId**：`check-login` 只能证明登录态有效，不能区分昵称相同的账号。切换账号或首次登录后，必须在 BOSS 页面/接口核对当前账号的 userId，确认是要投递的账号后再跑队列。
4. 不要用昵称判断账号。

### 搜索与详情

```bat
bosshunter.bat search --city 上海 --keyword Java后端开发 --max-pages 2
bosshunter.bat --json search --details --max-details 10
bosshunter.bat detail "https://www.zhipin.com/job_detail/xxxx.html"
```

### 单岗位投递

```bat
:: 只预检，不点击、不发送
bosshunter.bat deliver "https://www.zhipin.com/job_detail/xxxx.html" --dry-run

:: 使用 config.yaml 里的招呼语和图片
bosshunter.bat deliver "https://www.zhipin.com/job_detail/xxxx.html"

:: 临时指定招呼语或图片
bosshunter.bat deliver "https://www.zhipin.com/job_detail/xxxx.html" --greeting "您好，..." --image "C:\path\to\image.png"

:: 只发文字
bosshunter.bat deliver "https://www.zhipin.com/job_detail/xxxx.html" --no-images
```

### 投递队列

```bat
bosshunter.bat queue list --status pending
bosshunter.bat queue add "https://www.zhipin.com/job_detail/xxxx.html" --title "Java后端实习" --company "某公司" --city 上海 --priority 10
bosshunter.bat queue update <job_id> --status skipped
bosshunter.bat queue run --limit 1 --dry-run
bosshunter.bat queue run --wait-window --max-minutes 620
```

队列文件是 `data/apply_queue.json`。执行日志写入 `data/`，状态和每日计数写入 `data/state.json`。队列默认按 `priority` 从大到小投递，单岗位可在队列表中覆盖招呼语和图片。

## 当前配置摘要

| 项目 | 当前值 |
| --- | --- |
| 每日投递上限 | 150 |
| 两次投递间隔 | 30–60 秒随机 |
| 发送窗口 | 09:00–19:00 |
| 每日页面预算 | 600（搜索 60 / 详情 400 / 聊天按实际） |
| 连续失败熔断 | 10 次 |
| 默认招呼语 | 已配置在 `config.yaml` |
| 默认图片 | `./data/resumes/resume.png`（按需替换为本人简历图片） |
| 搜索关键词 | Java后端开发、全栈开发、Java开发实习生、后端开发 |
| 城市 | 上海、深圳、杭州、成都、武汉、广州、苏州、无锡、宁波 |

修改配置后建议先跑：

```bat
bosshunter.bat --json status
bosshunter.bat queue run --limit 1 --dry-run
```

## 测试

```bat
set PYTHONPATH=%CD%\src
python -m pytest -q
```

## 安全与注意事项

- 浏览器 profile、登录 cookie 和旧密钥文件没有复制；本副本首次使用必须重新登录。
- 同一账号同一时间只跑一个队列；旧目录的队列仍在运行时不要启动本副本的队列。
- 发送窗口外运行会等待；达到每日上限会停止。连续失败会熔断，避免误投。
- 不要把 `data/`、简历、图片和登录态提交到公开仓库。
- MCP 入口仍可用：`bosshunter.bat mcp`。当前 Qoder 里注册的 MCP 仍指向旧目录；日常使用本副本时直接走 CLI 即可。
