# 开发约定

项目坚持 **极致轻量，极致简约**。仓库是「根目录核心库 `personal-desk` + `desktop/` Tauri + React 客户端」的两段结构，只有一个桌面客户端。新增功能、代码边界、持久化、性能和验收要求统一见 [开发规范](docs/开发规范.md)。

开发环境为 Windows x64、Rust stable MSVC 工具链（`stable-x86_64-pc-windows-msvc`，最低 Rust 1.95）、Visual Studio C++ x64 生成工具，以及构建前端所需的 Node.js 与 npm。`scripts/windows-build-env.ps1` 初始化 MSVC 环境，其余脚本会先点源它。

提交前必须运行唯一检查入口 `./scripts/verify.ps1`：Rust 格式（根 crate 与 desktop 后端）、Clippy（含 release 库）、Rust 回归、`desktop` 前端构建 / lint / vitest 回归、Tauri 后端 Clippy 与 `scripts/tests` 的 Python 回归。该入口不发布程序、不修改个人数据；持续集成使用同一入口。前端 lint 目前只要求 0 error，既有 warning 仍会输出，不要把 lint 结果写成无警告通过。需要真实 MySQL、MongoDB 或发布权限的验证另行在对应环境执行，不把未运行的环境算作通过。

不要提交个人数据库、账号资料、模型、构建目录或本地验证日志。版本号五处必须一致（根 `Cargo.toml`、`desktop/src-tauri/Cargo.toml`、`desktop/src-tauri/tauri.conf.json`、`desktop/package.json`、`assets/release_history.json` 首项），发布流程见 [开发规范](docs/开发规范.md)。功能与测试对应关系见 [测试覆盖矩阵](docs/测试覆盖矩阵.md)。