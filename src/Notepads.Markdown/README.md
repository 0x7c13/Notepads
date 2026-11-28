# Notepads.Markdown

Notepads 同仓维护的原生 Markdown 阅读控件。公开入口为
`Notepads.Markdown.MarkdownView`；保持 UWP、WinUI 2 和 .NET 10 Native AOT。

第三方来源、每个保留 fork 的具体修改/原因/调用/回归，以及已恢复 registry 的依赖，
见 [FORKS.md](FORKS.md) 和逐文件哈希清单 [vendor/forks.json](vendor/forks.json)。

## 分层与 review 单元

调用方向：**宿主适配 → WinRT 控件 → 私有 C ABI → Rust 引擎**。
Rust 核心不依赖应用、Scintilla 或 XAML；图片 stream 和业务操作由宿主提供。

| 单元 | 路径 | 职责 |
| --- | --- | --- |
| 文档引擎 | `crates/markdown-core/` | 源文档、解析、语义 IR、映射、布局和阅读模型 |
| 原生边界 | `crates/markdown-ffi/`、`include/notepads/markdown/` | `notepads_markdown_*` ABI、wire 类型、句柄和分配方释放 |
| 资源能力 | `crates/markdown-svg*/`、`markdown-math/`、`markdown-mermaid/` | SVG、配额、数学和图表，可按能力单独回归 |
| 一方内存接口 | `crates/markdown-memory/` | 图表共用的分配权限、收据、region 和受控集合；由本模块维护 |
| 图表平台适配 | `windows/markdown-diagram-platform/` | DirectWrite 字体测量，平台对象不进入核心 |
| 公开控件 | `windows/Notepads.Markdown/Control/` | WinRT API、XAML 生命周期、输入、选择和视口 |
| 核心与阅读 | 同工程 `Core/`、`Reading/` | 异步解析/发布、取消、阅读快照、查找和导航 |
| 绘制与文本 | 同工程 `Rendering/`、`Text/` | DirectWrite/D2D、测量、脚本和字体 |
| 资源与无障碍 | 同工程 `Resources/{Images,Svg,Math,Diagrams}/`、`Automation/` | 按能力组织 coordinator/worker/artifact/presentation；UIA |
| 托管投影 | `windows/Notepads.Markdown.Projection/` | 由本架构 WinMD 生成 AOT 安全投影 |
| 构建 | `build/`、`scripts/` | Rust/Native 构建、宿主部署和分层测试入口 |
| 回归资产 | `tests/native/`、`tests/fixtures/`、`tests/baselines/` | 原生断言、固定输入与独立 oracle |
| 第三方 | `vendor/`、`third-party-notices/` | 锁定依赖、定制 fork、上游归属和许可证 |

宿主在 `../Notepads.App/Presentation/Controls/Markdown/`，分为 `Control/`、
`Source/`、`Scrolling/`、`Images/`。编辑器适配位于相邻 `TextEditor/`。
相对图片和嵌入笔记由 `../Notepads.DocumentAssets/` 和宿主
`Infrastructure/DocumentAssets/` 读取。嵌入笔记以自身目录解析相对资源，并沿用已打开文档的授权。

目录界定职责，不在层间复制状态。修改源坐标、修订/代次、取消或资源所有权时，
同时核对 ABI、平台发布门和宿主；资源能力的改变通常限定在对应 crate、
`Resources` 子目录和该能力的测试。

## 构建

工具链由仓根 `global.json` 和本目录 `rust-toolchain.toml` 固定。
需要 Visual Studio 2026 UWP C++ 工具，支持 `x86`（原生 `Win32`）、`x64`、`ARM64`。
原生控件/Rust 使用 Release 动态 CRT，托管宿主按自身配置构建。

从仓根运行开发构建，或在 VS Developer PowerShell 执行：

```powershell
./scripts/Run-Dev.ps1 -Platform ARM64 -BuildOnly -NoInstall
msbuild src/Notepads.sln /restore /p:RestorePackagesConfig=true /p:Configuration=Release /p:Platform=x64
```

默认构建根为 `%USERPROFILE%/.notepads-build/Markdown/<模块路径指纹>`，
可用 `NotepadsMarkdownBuildRoot` 覆盖；不能放在 FileTracker 排除的 AppData/Temp。
`build/Notepads.Markdown.Consumer.targets` 统一部署 DLL、WinMD、PRI、模板和许可；
宿主引用 Projection 并导入此 targets，无需额外的 Markdown NuGet 包。

## 分层验证

在 VS Developer PowerShell 中从仓根执行：

```powershell
./src/Notepads.Markdown/scripts/Test-Rust.ps1 -Layer Core
./src/Notepads.Markdown/scripts/Test-Rust.ps1 -Layer Interop
./src/Notepads.Markdown/scripts/Test-Rust.ps1 -Layer Svg
./src/Notepads.Markdown/scripts/Test-Rust.ps1 -Layer Math
./src/Notepads.Markdown/scripts/Test-Rust.ps1 -Layer Diagrams
./src/Notepads.Markdown/scripts/Test-Rust.ps1 -Layer All
./scripts/Test.ps1 -Suite Integration -Group Markdown -Platform ARM64
```

Rust 入口支持 `-Target` 和 `-TargetDirectory`。`Core` / `All` 包含独立 integration
tests，`Interop` / `All` 启用 FFI `diagnostics`。Math 保持自己的锁定 workspace；第三方专用开发单测
不自动等于上述入口。真实 AOT/MSIX、UI、无障碍和设备性能分别验收。

默认产品不编入 `tests/native` 或 CoreValidation WinRT 类。App/Lifecycle 测试显式
设置 `NotepadsMarkdownEnableDiagnostics=true` 并导入
`tests/native/Notepads.Markdown.Tests.items`。产品/诊断的 native、managed、Cargo 和链接
输入分别进入 `production` / `diagnostics` 目录，避免跨变体复用投影或 DLL。
`MarkdownCoreImages` 覆盖 SVG worker 的正常执行、启动前取消与退出，以及 retained owner 的释放与恢复。
独立 SVG 字体会话和需要新进程的永久隔离故障用例仍需单独验证。

金样、字形、度量、像素和原 hash 保留。Fontations 共享载荷说明见
`tests/baselines/fontations-shared.json`。测试字体名称及 SVG family 已归入本模块；
仅名称表和必要校验和改变，当前清单同步新 hash，历史 provenance 保留原始身份。

当前支持选定 CommonMark/GFM、图片、数学、有限 Mermaid 和静态 SVG。
完整 HTML/CSS/JavaScript 与完整 Mermaid 语法不在当前支持范围。
主许可证为 [MIT](LICENSE)，第三方保留原作者、条款和上游来源。
