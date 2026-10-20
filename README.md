# 个人办公台

Rust 编写的 Windows 个人桌面工具。直接打开使用，集中管理文件、账号、文档、联系人和物品。当前版本：**0.3.5**。

## 使用

打开 `dist/个人办公台.exe`。已有数据位置由旁边的 `personal-desk-data-path.txt` 指定，移动程序时请一并保留。

### 开机自启动

进入 **设置 → 启动设置 → 开机自启动**，勾选后生效，取消勾选即可关闭。程序会在当前用户登录 Windows 时打开，不需要管理员权限。

自启动记录保存当前 EXE 的完整路径，支持中文及空格。已经开启自启动时，移动程序后手动打开一次正式版，会更新登记的启动路径；未开启时不会自动开启。状态读取或保存失败会显示错误。如果在 Windows 任务管理器中禁用了本程序，还需在那里重新启用。

### 更新

点击左下角 **更新 → 检查更新 → 安装更新并重启**。更新前先保存编辑并关闭终端和网页标签。

当前使用本机发布目录 `dist/updates/latest.json`，尚未配置远程更新服务。开发端修改代码后运行构建脚本发布，使用端从软件内安装；安装会重启程序。保留 `updates` 目录和数据位置文件。

## 功能

| 功能 | 支持的操作 |
| --- | --- |
| 快捷路径与文件 | 保存本机地址、台内浏览、双击打开、选择程序打开、复制、移动、拖入/拖出、删除撤回、类型筛选 |
| 账号密码 | 明文显示、原位修改、快捷复制、自定义标签 |
| 文档写作 | 台内直接新建和编辑、自动保存、冲突检查、恢复草稿、Markdown/TXT/Word 导出 |
| 图书 | 多页面、页面标签页、分类、版本记录及删除恢复 |
| 联系人 | 姓名、性别、生日、学校、专业、工作、手机号、身份证号、地址、爱好、照片及聊天记录 |
| 物品清单 | 数量、位置、分类、购买日期、购买价格、丢弃日期，列表内修改 |
| 网站收藏 | 名称、网址、标签、备注，在内置网页页面打开 |
| 大模型聊天 | WebView2 多标签页、DeepSeek/ChatGPT 入口、自填网址、前进/后退/刷新及加载提示 |
| 小工具与 PowerShell | 保存 EXE 入口、双击运行，在当前文件夹打开交互终端 |
| Codex | 本机原版窗口停靠，备用 app-server 聊天；需已安装相应 Codex 应用或 CLI |
| 小游戏 | 无尽贪吃蛇、暂停与最高分 |
| 设置 | 开机自启动、自定义颜色与背景图、数据位置、全部导出 |

记录页支持宫格/列表切换、当前页搜索、分类和收藏置顶。文件删除可通过“撤回”或 `Ctrl+Z` 恢复，撤回日志跨重启保存；聚焦文本输入或终端时使用对应的编辑/终端行为。

聊天记录可导入 TXT、Markdown、JSON、Excel（`.xlsx` / `.xls`），原件随联系人保存并可导出。网页依赖系统 WebView2 Runtime，网站的登录和兼容情况由网站本身决定。

## 数据与导出

- 默认数据目录：`%LOCALAPPDATA%/PersonalDesk`；程序旁的数据位置文件可覆盖此默认值。
- `desk.db` 保存记录、联系人照片、聊天附件、标签和设置；文档、草稿、主题背景及浏览器资料保存在数据目录中。
- 账号密码按普通文本保存；办公台没有登录、权限校验或锁定流程。
- 各模块可导出，全部导出包含 JSON、文档、草稿及数据库快照。**快捷路径只保存地址，不打包指向的外部文件夹。**浏览器登录资料不加入可读数据导出。
- 更新替换程序，不替换个人数据目录；安装前创建数据库备份。自动保存、冲突检查和备份不能代替定期导出。

也可使用命令行导入或导出：

```powershell
.\dist\个人办公台.exe --import-data 输入.json
.\dist\个人办公台.exe --export-all 输出.zip
```

导入 JSON 包含 `accounts` 和可选的 `websites` 数组。导入去重并使用事务，导出先验证临时压缩包再替换目标文件。

## 项目结构

```text
src/                  主程序、领域逻辑及同模块测试
  app/                页面、交互与界面回归测试
  db/                 数据访问
  ui_components/      共用界面组件
  tests.rs            数据、文件、导入导出等回归测试
scripts/              构建及更新发布脚本
docs/开发文档.md      设计、持久化与实现说明
Cargo.toml            包信息及依赖
Cargo.lock            固定依赖版本
rust-toolchain.toml   Rust 工具链配置
dist/                 本机运行及更新文件，不提交 Git
```

测试与源码就近组织，使用 `#[cfg(test)]` 隔离；`preview.rs` 和 `interaction_probe.rs` 是开发版隔离验证工具，不编入正式版。历史截图、临时修改脚本、验证副本、旧构建缓存及过期 Word 文档已移出项目归档。保留 MIT [许可证](LICENSE)。

## 开发与构建

需要 Windows x64、Rust 1.95 或更新版本、`x86_64-pc-windows-gnu` 目标、MSYS2 UCRT64 GCC，以及 WebView2 Runtime。

```powershell
rustup target add x86_64-pc-windows-gnu
# GCC 未在 PATH 中时，按实际安装位置设置：
$env:MSYS2_UCRT64_BIN = 'D:\msys64\ucrt64\bin'
.\scripts\build.ps1
```

构建脚本依次执行格式检查、Clippy、测试和发布编译，输出 `dist/个人办公台.exe`，并发布版本包与更新清单。默认使用临时目录中的英文构建路径，避免 GNU 链接器在中文目录中的兼容问题。程序正在运行时只发布更新包，由应用内安装完成替换。

单独运行开发与测试命令时，构建目录及 `TEMP`/`TMP` 也应使用不含中文的路径：

```powershell
$env:CARGO_TARGET_DIR = "$env:LOCALAPPDATA\PersonalDesk-build"
$env:TEMP = "$env:LOCALAPPDATA\PersonalDesk-build-tmp"
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force $env:TEMP | Out-Null
cargo run
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

测试覆盖记录持久化、迁移、标签、原位编辑、导出、文件撤回、草稿冲突、界面布局、终端输入、更新替换及自启动登记。自启动测试写入独立临时注册表键，验证开启、关闭及路径变化，不更改真实开机启动项；需真实 Codex 或外部交互的检查默认忽略。

详细实现见 [开发文档](docs/开发文档.md)。
