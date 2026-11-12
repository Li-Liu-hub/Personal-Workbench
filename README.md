# 个人办公台

一个本地优先的 Windows 桌面工作台：文档写作、每日随笔、联系人、账号密码、物品清单、网站收藏、快捷路径和小工具启动都放在一个客户端里。数据默认保存在本机 SQLite，不依赖云端账号。

- 客户端：Tauri 2 + React 19 + TypeScript + Ant Design 6
- 后端：Rust，业务与数据逻辑、Tauri 命令同在一个工程
- 数据：SQLite（`desk.db`），可选 MySQL 私网加密库与 MongoDB 文档库
- 平台：Windows x64

## 功能

| 模块 | 说明 |
|---|---|
| 文档写作 | 标题与 Markdown 正文，随窗口展开；支持代码块、粘贴图片、自动保存、草稿恢复、Markdown 导出与博客发布 |
| 每日随笔 | 每日主题、换题、记录本与全文搜索；与文档共用自动保存的编辑器 |
| 联系人 | 分类、置顶、照片、聊天记录导入与导出 |
| 账号密码 | 分类、置顶、复制字段、一键生成强密码、可选私网加密库同步 |
| 物品清单 | 数量、位置、购买与处置信息、分类与置顶 |
| 网站收藏 | 分类、置顶、宫格/列表视图、一键打开 |
| 快捷路径 | 文件和文件夹索引、分类、置顶、撤销移除 |
| 小工具 | 记录 EXE 或文件地址，单击启动；移除入口不会删除原程序 |
| 设置 | 主题、缩放、数据目录、开机自启动、检查更新、MySQL / MongoDB 连接 |

小游戏、刷题王、每日单词、百科全书和求职工具是 `D:\MyProject` 下的独立项目，不在本客户端内，也不由本仓库构建。

## 下载与更新

正式版本从 GitHub Releases 获取：

<https://github.com/Li-Liu-hub/Personal-Workbench/releases/latest>

日常入口是 `dist/个人办公台.exe`，旁边保留 `personal-desk-data-path.txt` 指向原数据目录。客户端默认读取 Release 中的 `latest.json` 检查更新。

## 数据位置

优先使用程序旁的 `personal-desk-data-path.txt`；没有时使用已记录的 Windows 用户数据目录，最后回退到 `%LOCALAPPDATA%\PersonalDesk`。核心数据库文件是 `desk.db`。

开发模式必须显式设置隔离数据目录，避免误用真实数据：

```powershell
$env:PERSONAL_DESK_TEST_DATA = "$env:TEMP\personaldesk-test"
```

不要提交 `desk.db`、个人账号资料、文档、图片或模型文件。

## 目录结构

```
个人办公台/
├── desktop/
│   ├── src/                    React 前端（页面、组件、hooks、vitest）
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── core/           业务与数据层：SQLite、领域规则、导入导出、加密、更新
│   │   │   ├── commands/       Tauri IPC 命令，按领域拆分
│   │   │   ├── state.rs        数据目录、数据库句柄、窗口与后台同步
│   │   │   ├── lib.rs          应用生命周期与命令注册
│   │   │   └── main.rs         图形入口与无窗口命令行入口
│   │   ├── tests/              迁移回归与版本一致性测试
│   │   └── Cargo.toml
│   └── package.json
├── assets/                     图标与版本历史
├── scripts/                    检查、构建、打包、发布脚本
├── packaging/                  Windows 安装包配置
└── dist/                       本机构建产物与更新清单（不提交）
```

前端通过 `@tauri-apps/api` 的 `invoke` 调用命令，不直接访问数据库或文件系统。命令层只做参数转换，业务校验统一放在 `core/`。

## 接口文档

命令层是 Tauri IPC，不走 HTTP，但同样是“入参 → 返回值”。项目从 Rust 源码生成 OpenAPI 3.1 描述，方便查看每个命令传什么参数、返回什么结构：

- `docs/API.md`：中文速查，含每个命令的参数表、请求/响应示例和源码位置
- `docs/api.html`：Swagger UI 可视化（需联网加载界面资源；Execute 已禁用）
- `docs/openapi.json`：可导入 Postman、Apifox 或 Swagger Editor

```powershell
cd desktop
npm run docs:api         # 重新生成
npm run docs:api:check   # 校验文档是否与源码同步
```

新增或修改 `#[tauri::command]` 后运行 `npm run docs:api`；`scripts/verify.ps1` 会检查文档是否过期。

## 开发

环境要求：

- Windows x64
- Rust `stable-x86_64-pc-windows-msvc`，最低 1.95
- Visual Studio C++ x64 生成工具
- Node.js 与 npm

```powershell
cd desktop
npm install
$env:PERSONAL_DESK_TEST_DATA = "$env:TEMP\personaldesk-test"
npx tauri dev
```

`npx tauri dev` 会按 `tauri.conf.json` 的 `beforeDevCommand` 自动启动 Vite，不需要另开 `npm run dev`。只调试前端界面时可以单独运行 `npm run dev`，但此时没有 Tauri 命令。

前端单独检查：

```powershell
npm run build
npm run lint
npm run test
```

界面资源：

- 标题字体是内置的 Noto Serif SC SemiBold 子集（OFL，见 `desktop/public/fonts/OFL.txt`）。需要重新生成时运行 `python scripts/build-font-subset.py`。
- 应用图标由 `python scripts/build-icon.py` 生成母版，再在 `desktop` 目录执行 `npx tauri icon ../assets/icon-design-v2/personaldesk-master.png` 生成各尺寸。
- 设计变量与页面样式在 `desktop/src/styles/`，页面组件继续使用 `desktop/src/ui/` 下的共享外壳。

## 检查与构建

统一检查入口：

```powershell
./scripts/verify.ps1
```

它覆盖 Rust 格式、Clippy、Rust 回归、前端构建 / lint / vitest 回归和 Python 回归。正式构建必须先通过同一入口：

```powershell
./scripts/build.ps1
```

构建产物复制到 `dist/个人办公台.exe`，并写入 `dist/updates/latest.json`。发布包与 GitHub Release 分别使用：

```powershell
./scripts/package-release.ps1
./scripts/publish-update.ps1
./scripts/publish-github-release.ps1
```

版本号需在四处保持一致：

- `desktop/src-tauri/Cargo.toml`
- `desktop/src-tauri/tauri.conf.json`
- `desktop/package.json`
- `assets/release_history.json` 首项

回归测试：`desktop/src-tauri/tests/version_consistency.rs`。

## 许可

MIT
