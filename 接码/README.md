# 接码（本地短信验证码接收端）

独立的本机验证码接收服务：安卓手机用开源 **SmsForwarder** 把验证码短信通过 Webhook 转发到这台电脑，接收端只在局域网内提供取码接口。官网投递工具在本地读取验证码后直接填入页面，验证码不会返回给模型、不会打印到聊天、不会写入日志或资料 MD。

## 目录

```
接码/
├── start-receiver.bat      # 启动接收端（前台窗口，Ctrl+C 停止）
├── status.bat              # 查询运行状态（不显示验证码）
├── config.yaml             # 监听地址、端口、token 文件位置
├── data/
│   ├── otp_token.txt       # 访问 token（仅当前 Windows 用户可读）
│   ├── otp_url.txt         # 含 token 的完整 Webhook URL（勿分享）
│   ├── otp_runtime.json    # 运行时 host/port/PID（不含 token）
│   └── otp_receiver.log    # 可选日志（只记录脱敏信息）
├── src/otp_receiver/       # 接收端与 CLI 源码
└── tests/                  # 单元测试
```

## 启动与停止

```bat
cd /d C:\Users\20886\Desktop\个人办公台\接码
start-receiver.bat
```

启动成功后窗口会输出监听地址、网卡和**脱敏后的** URL；完整 URL 写入 `data/otp_url.txt`。在另一个窗口检查状态：

```bat
status.bat
```

停止：在接收端窗口按 `Ctrl+C`。

> 端口 `8765` 同一时间只能有一个接收端。旧目录 `Boss工具\OfficialApply` 的接收端如果还在运行，先停掉它，否则本工具会提示端口占用。本副本沿用了旧安装的 token，切换后手机端 Webhook URL 通常不需要修改。

## 手机端 SmsForwarder 配置

手机和电脑连同一个热点即可（谁开热点都行）。手机需要一张能接收验证码的 SIM 卡。

### 1. 发送通道（Webhook）

在 SmsForwarder 的“发送通道 → 新增 → Webhook”中填写：

| 字段 | 填写内容 |
| --- | --- |
| 通道名称 | 官网验证码 |
| 状态 | 启用 |
| 请求方式 | POST |
| Webhook Server | `data/otp_url.txt` 里的完整 URL（含 token，勿发到聊天） |
| 消息模板 | 建议 JSON：`{"from":"[from]","content":"[org_content]","timestamp":"[timestamp]"}` |
| Secret | 留空（token 已在 URL 路径中） |
| Headers | 留空 |
| 代理配置 | 不使用代理 |
| 成功应答关键词 | `"ok": true`（如果你们的版本会压缩空格，填 `"ok"` 也可以） |

接收端兼容 JSON 和表单两种请求，字段名支持：

- 发送方：`from` / `sender` / `phone`
- 短信正文：`content` / `message` / `body`
- 时间戳：`timestamp`（秒或毫秒；不传也接受）

如果你之前已经配置成功并能收到 `code_found=True`，保持原来的模板即可。

### 2. 转发规则

SmsForwarder v3.5 的规则页实测字段如下：

| 字段 | 填写内容 |
| --- | --- |
| 规则别名 | 官网验证码 |
| 发送通道 | 官网验证码 |
| 匹配卡槽 | 不限卡槽 |
| 匹配字段 | **短信内容**（必须选这一项，才会出现正则输入框） |
| 正则匹配 | `.*(验证码\|校验码\|动态码\|动态密码\|确认码\|验证代码).*` |
| 启用自定义模板 | 关闭 |
| 启用正则替换内容 | 关闭 |
| 启用该条转发规则 | 打开 |
| 免打扰（禁用转发）时间段 | `00:00 ~ 00:00`（界面提示相等即禁用） |

配置好后用规则页底部的“测试”发一条含验证码的样例短信。接收端窗口/日志出现 `code_found=True` 即全链路打通。接收端 POST 响应只返回 `{"ok":true,"code_found":true}`，**不会把验证码返回给手机或模型**。

## 网络与安全

- 默认 `bind_ip: auto`，只绑定 WLAN/热点网卡；找不到局域网网卡时退回 `127.0.0.1` 并警告。不要改成 `0.0.0.0`。
- 手机和电脑必须能互通；Windows 防火墙需允许 Python 在专用网络监听（首次启动时选择“允许”）。
- token 为 32 字节随机值，约 43 字符；token、完整 URL、runtime 文件均通过 ACL 限制为当前 Windows 用户可读。
- `GET /latest` 只允许本机访问；`GET /status` 和 `POST` 需要 URL 中的 token。
- POST 每个来源 IP 每分钟最多 60 次；请求体最大 64KB；时间戳偏差超过 600 秒拒绝。
- 日志只记录脱敏后的发送方和 `code_found`，不记录短信正文、验证码、token 或完整请求路径。
- 验证码只存在内存中，默认 120 秒过期，过期自动丢弃。
- 专用收码手机号只用于接收验证码，不要写进 `官网投递资料.md`、投递表单或日志。
- 更换 token：

```bat
cd /d C:\Users\20886\Desktop\个人办公台\接码
set PYTHONPATH=%CD%\src
python -m otp_receiver --config config.yaml serve --rotate-token
```

执行后必须把新的完整 URL 更新到手机 SmsForwarder，旧 token 立即失效。

## 接口

以下 `<token>` 均指 `data/otp_token.txt` 的内容；`<host>` 是启动时打印的电脑热点 IP。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| POST | `http://<host>:8765/otp/<token>` | SmsForwarder 上报短信，返回 `code_found` |
| GET | `http://<host>:8765/otp/<token>/status` | 接收计数、TTL、最近时间（不含验证码） |
| GET | `http://<host>:8765/otp/<token>/latest` | 取最近验证码；**仅本机可访问** |

POST 示例（仅用于本机调试，不要把真实验证码发到聊天）：

```json
{"from": "13800138000", "content": "您的验证码是 123456", "timestamp": 1792996252}
```

## 与官网投递工具配合

`个人办公台\官网投递\config.yaml` 已指向本工具的 `data/otp_token.txt`、`otp_url.txt`、`otp_runtime.json`，并设置 `auto_start: false`。使用顺序：

1. 启动 `..\接码\start-receiver.bat`；
2. 在官网页面点击“获取验证码”；
3. 运行 `..\官网投递\officialapply.bat otp-fill`。

`otp-fill` 只输出输入框数量、是否校验通过等元数据，不会输出验证码。

## 测试

```bat
cd /d C:\Users\20886\Desktop\个人办公台\接码
set PYTHONPATH=%CD%\src
python -m pytest -q
```

## 常见问题

- **403 forbidden**：token 不匹配。确认手机 URL 与 `data/otp_url.txt` 一致；如果刚轮换过 token，要同步更新手机。
- **手机连不上**：确认同一热点、电脑 IP 正确、防火墙允许 Python；看 `status.bat` 的监听地址是不是 `127.0.0.1`（回环地址手机访问不到）。
- **端口占用**：旧接收端还在运行。停掉旧接收端再启动本工具。
- **code_found 一直是 false**：短信正文没有“验证码/校验码/动态码…”等关键词，或验证码不是 4–8 位数字；检查 SmsForwarder 规则是否匹配“短信内容”。
- **stale_or_invalid_timestamp**：手机时间和电脑时间相差超过 600 秒；校准手机时间，或把短信模板里的 timestamp 去掉（接收端允许不传时间戳）。
