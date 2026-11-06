# 个人办公台桌面客户端

桌面端使用 TypeScript、React、Ant Design、Tauri 与 Rust。前端页面调用 `src-tauri` 命令，Rust 复用项目的数据模型、SQLite 数据库、图片与文档处理、导出和更新器。

## 本地开发

```powershell
npm install
$env:PERSONAL_DESK_MIGRATION_TEST_DATA = "$env:TEMP\personaldesk-test"
npx tauri dev
```

开发模式必须指定隔离的数据目录。正式构建不读取此变量，使用程序旁的 `personal-desk-data-path.txt`、现有 Windows 用户数据目录设置或 `%LOCALAPPDATA%\PersonalDesk`。

## Windows 正式构建

使用项目脚本通过 Visual Studio MSVC x64 工具链构建。MSVC 会把 WebView2 加载器静态链接到程序，更新只替换 EXE 时也能启动。

```powershell
.\scripts\build-tauri-windows.ps1
```

版本号与根目录 `Cargo.toml`、本文件夹的 Tauri Cargo 配置和 `tauri.conf.json` 同步。正式发布包由项目根目录 `scripts/package-release.ps1`、`scripts/publish-update.ps1` 与 `scripts/publish-github-release.ps1` 生成并校验。
