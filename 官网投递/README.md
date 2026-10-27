# 官网投递（OfficialApply）

这是 `个人办公台` 中的官网/网申投递工具副本。它打开公司招聘官网，识别页面模板，读取 `官网投递资料.md` 自动填表、上传材料，并在最终提交前停下；验证码由同级的 `接码` 工具提供。

> **副本说明**
>
> - 没有复制 Chrome 登录态、`data/session.json` 和旧验证码 token 日志；
> - 首次使用需要重新登录目标网站；
> - 浏览器 profile 会在本目录 `data/chrome-profile` 重新生成；
> - 当前 Qoder 里注册的 MCP 仍指向旧目录，本副本默认走 CLI。

## 工作流程

1. 启动 `接码` 接收端（需要手机验证码时）。
2. 用 `open` 打开招聘页，必要时用 `login` 扫码/完成滑块（人工）。
3. 用 `inspect` 查看字段、按钮和登录/验证码信号。
4. 用 `autofill` 一键填写，自动处理多步表单，但**不会点击最终提交**。
5. 用 `otp-fill` 从本地接码服务取验证码并直接填入页面，验证码不回显。
6. 检查页面，确认无误后用 `submit --dry-run` 预检，再由本人授权真正提交。

## 目录

```
官网投递/
├── officialapply.bat       # 日常 CLI 入口（带 PYTHONPATH，不需要 pip install）
├── config.yaml             # 当前配置（验证码指向 ../接码/data）
├── 官网投递资料.md          # 个人资料与材料路径
├── data/                   # 浏览器 profile、截图、投递记录（首次运行后生成）
├── src/officialapply/      # 源码
└── tests/                  # 单元测试与本地 E2E 脚本
```

## 环境

- Windows + Python 3.10+
- Google Chrome 或 Microsoft Edge
- 已安装依赖：`click`、`httpx`、`pyyaml`、`websocket-client`（旧安装环境已有）
- 使用本副本不需要 `pip install`，启动脚本会设置 `PYTHONPATH`

## 快速开始

```bat
cd /d C:\Users\20886\Desktop\个人办公台\官网投递

officialapply.bat status
officialapply.bat profile-check
officialapply.bat profile-show
```

### 资料文件

资料在 `官网投递资料.md`。当前仍有大量 `待补充`，常见缺项包括：

- 基本信息：民族、政治面貌、婚姻状况、身高、体重、身份证号、户籍所在地、现居城市、通讯地址、籍贯
- 紧急联系人：姓名、电话、与本人关系
- 求职意向：期望薪资、到岗时间、求职类型、是否接受调剂/出差/加班/实习/倒班/远程
- 教育：主修课程、学号
- 实习/项目：部门、工作地点、起止时间、项目链接
- 证书与通用问题：证书、是否有亲友在本公司任职、是否接受背景调查等

`待补充` 不会被填进网页。页面把某项标为必填时，工具会把它当作未处理必填并在提交前拦住。先补齐最常用的：**现居城市、到岗时间、身份证号、户籍、通讯地址、紧急联系人**。

材料路径在资料 MD 的“投递材料”一节，目前指向 `D:\示例公司\秋招\简历\...`。换电脑或移动文件后要同步修改这些路径。

### 打开与登录

```bat
officialapply.bat open "https://company.example.com/jobs/123"
officialapply.bat login "https://company.example.com/login"
officialapply.bat check-login
```

登录页的二维码、滑块、人机验证由本人完成；工具只负责打开页面、检测登录态和继续填表。不会破解或模拟拖动验证码。

### 检查页面

```bat
officialapply.bat inspect
officialapply.bat inspect --max-text 12000 --front
officialapply.bat plan
```

`inspect` 会列出字段 id、标签、类型、是否必填、选项、按钮和登录/验证码信号。

### 一键填写

```bat
officialapply.bat autofill --job-title "Java后端开发实习生" --job-company "某公司"

:: 临时覆盖某个字段
officialapply.bat autofill --job-title "Java后端开发实习生" --set 现居城市=上海

:: 不自动上传附件
officialapply.bat autofill --no-upload
```

`autofill` 会自动识别常见网申模板、勾选可确定的选项、上传配置的简历/照片等材料，并推进多步表单；默认停在最终提交按钮前，不点击提交。

### 验证码

先启动同级接码工具：

```bat
cd /d C:\Users\20886\Desktop\个人办公台\接码
start-receiver.bat
```

再回到本目录：

```bat
officialapply.bat otp-fill
```

本副本的 `config.yaml` 已配置：

```yaml
otp:
  token_file: ../接码/data/otp_token.txt
  url_file: ../接码/data/otp_url.txt
  runtime_file: ../接码/data/otp_runtime.json
  auto_start: false
```

所以官网工具不会再自己拉起第二个接收端。验证码只在本地读取后直接填入页面，不会打印到终端、不会进日志、不会写进资料 MD。

> 端口 8765 同一时间只能有一个接收端。如果提示端口占用，说明旧目录的接收端还在运行；先停旧接收端，再启动 `接码`。本副本的接码沿用了已有 token，SmsForwarder 的手机端 URL 通常不需要改；如果执行了 `--rotate-token`，必须同步更新手机 Webhook。

### 提交

```bat
:: 只预检，不点击
officialapply.bat submit --dry-run

:: 填写并检查完成后，本人确认再提交
officialapply.bat submit --accept-terms --title "Java后端开发实习生" --company "某公司"
```

提交安全闸门：

- 每日最多 20 份，两次提交至少间隔 30 秒；
- 存在未识别的必填项时禁止提交；
- 条款/隐私协议必须由本人确认后才加 `--accept-terms`；
- 相同链接默认去重，确需重投才加 `--force`；
- 测试模板时不要对真实公司提交申请。

### 记录与其他

```bat
officialapply.bat history
officialapply.bat templates
officialapply.bat screenshot "页面名"
```

投递记录默认写到 `data/applications.jsonl`，截图在 `data/screenshots`。浏览器使用独立调试端口 9444 和独立 profile，不会影响 BOSS 直聘的 Chrome。

## 测试

```bat
set PYTHONPATH=%CD%\src
python -m pytest -q
```

## 安全与注意事项

- `官网投递资料.md` 含身份证、联系方式等敏感信息，只保存在本机；不要上传或发到聊天。
- 材料目录中的身份证附件默认不自动上传；敏感字段只在页面明确要求时填写。
- 扫码、滑块、人机验证必须人工完成；反复触发验证就停止自动流程。
- 接码 token 和完整 URL 在 `../接码/data/`，仅当前 Windows 用户可读；不要分享。
- 登录态和 session 没有复制，首次使用要重新登录。
- 当前 Qoder 的 MCP 仍指向旧目录；本副本用 `officialapply.bat` 或 `python -m officialapply.cli` 驱动。
