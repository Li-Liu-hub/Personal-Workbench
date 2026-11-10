# 个人办公台桌面客户端

`desktop/` 是个人办公台唯一的桌面客户端：前端使用 TypeScript、React 19、Ant Design 6 与 Vite，窗口与系统能力由 Tauri 2 提供；Rust 侧复用根目录核心库 `personal-desk` 的数据访问、领域规则、导入导出、更新器、图片、私网账号库与 MongoDB 文档发布等 API，并用 rfd、arboard 等库处理系统文件对话框与剪贴板。

## 目录职责与分层

- `src/`：前端页面（`*Page.tsx`）、组件、hooks 与 vitest 回归测试。页面通过 `@tauri-apps/api` 的 `invoke` 调用命令，不直接访问数据库或文件系统。
- `src-tauri/src/state.rs`：数据目录解析、数据库初始化与访问、单实例、主窗口（含 WebView2 用户数据目录）与后台私网同步。
- `src-tauri/src/lib.rs`：只保留应用生命周期与命令注册；`src-tauri/src/main.rs`：图形入口与 `--apply-update`、`--import-data`、`--export-all`、`--import-writing-bundle` 命令行模式。
- `src-tauri/src/commands/*.rs`：按领域拆分的 Tauri 命令，只做参数转换与调用核心库 API；业务校验统一放在核心库，不在命令里重复写规则。
- `src-tauri/capabilities/default.json`：窗口权限；`src-tauri/tauri.conf.json`：产品名、版本与前端构建入口。

## 本地开发

开发模式必须显式指定隔离数据目录，避免误用真实数据。变量新名为 `PERSONAL_DESK_TEST_DATA`；旧名 `PERSONAL_DESK_MIGRATION_TEST_DATA` 仅作为历史兼容继续读取，新脚本与新代码请使用新名。开发版解析不到测试目录变量时会拒绝打开数据库。

```powershell
npm install
$env:PERSONAL_DESK_TEST_DATA = "$env:TEMP\personaldesk-test"
npx tauri dev
```

正式构建不要求设置该变量：未设置时按程序旁的 `personal-desk-data-path.txt`、已记录的 Windows 用户数据目录或 `%LOCALAPPDATA%\PersonalDesk` 解析；显式设置时优先使用，便于隔离验证。

## 检查与构建

前端构建、lint 与回归可在本目录单独执行：

```powershell
npm run build
npm run lint
npm run test
```

完整检查与正式构建统一使用项目根目录脚本（Visual Studio MSVC x64 工具链）：`scripts/verify.ps1` 是唯一检查入口（含本目录前端 build / lint / test、desktop 后端 fmt / clippy 与 Rust 回归），`scripts/build.ps1` 必须先通过同一入口，再构建 Tauri 发布程序、复制程序到 `dist/个人办公台.exe` 并发布本机更新清单。正式发布包由 `scripts/package-release.ps1`、`scripts/publish-update.ps1` 与 `scripts/publish-github-release.ps1` 生成并校验。

构建产物导入 `WebView2Loader.dll` 时会依赖该 DLL；`scripts/build.ps1` 会检查构建产物是否真的导入，需要时复制到 `dist`，不需要时清理旧 DLL。分发时该 DLL 必须与 `PersonalDesk.exe` 放在同一目录，免安装 ZIP 已包含它。

版本号与根 `Cargo.toml`、`desktop/src-tauri/Cargo.toml`、`desktop/src-tauri/tauri.conf.json`、`desktop/package.json`、`assets/release_history.json` 首项一致；`tests/version_consistency.rs` 做回归，构建脚本还会核对 EXE 版本资源。