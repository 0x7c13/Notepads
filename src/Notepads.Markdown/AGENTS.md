# Notepads.Markdown 开发约定

先读本目录 README、相关层源码和当前 diff。本模块在 Notepads 同仓维护，
不依赖外部源码仓、submodule 或私有 Markdown NuGet；分层和测试入口见 README。

- 保持 Rust → 窄 C ABI → C++/WinRT → C# 投影。Rust 核心不引用应用业务、
  Scintilla 或 XAML；Windows 对象归平台层，宿主提供图片 stream 与业务操作。
- 保持 UWP/WinUI 2、.NET Native AOT 和三架构，不默认迁移 WinUI 3、Windows App SDK
  或浏览器。公开 namespace 为 `Notepads.Markdown`；私有 ABI 使用 `notepads_markdown_*`，
  C++ 内部实现位于 `Notepads::Markdown::Native`。
- 按 README 的层/资源能力组织改动，不让通用核心、PCH 或产品构建依赖测试实现。
  产品与诊断输出隔离；集成测试才启用 `NotepadsMarkdownEnableDiagnostics`。
- 分配方释放，异常不穿 ABI；异步结果按修订/代次发布。UTF-8 源坐标与 UTF-16
  呈现坐标分别映射。重构不顺带改变预算、取消、所有权和发布门语义。
- 重要设计/实现由干净上下文 `gpt-6.1-sol`、`xhigh` 子代理独立审查。
  可按文件所有权并行；Cargo/lock、FFI、IDL 和共享构建入口由集成者统一修改。
- 新 UI/排版能力先让用户实际查看再做最终独审；纯改名/结构整理不重开已通过的
  视觉验证，也不自动关闭既有设备、性能和用户体验待验。
- 保留有效失败回归、固定开源语料、上游 commit、版权和原 hash，不重生成金样。
  研究中间文件可按用户授权清理，先保存仓外原件；保留历史 provenance 和原始 hash。
  字体命名已按用户要求归入本模块；后续改名须证明名称表及必要校验和以外的表未变，
  并同步 SVG、断言和当前 hash。实际 AOT/MSIX/UI 结果只授予对应范围的验收资格。
- 大改前和重要里程碑后保存源码、私有文档与 Git 历史并逐 SHA 校验。
  缓存与备份放 C 盘仓外；拒绝过的路径删除操作及其父目录不重试、不绕过。
- Rust stable/rustfmt，unsafe 说明边界；C++20、RAII、Allman、4 空格；C# 使用
  file-scoped namespace。C#/C++/IDL/XAML/MSBuild/PowerShell 为 UTF-8 CRLF，Rust LF。
- 主许可证 MIT，新原创文件署名 `2026, Jiaqi (0x7c13) Liu` 并引用本模块 LICENSE。
  保留第三方作者、许可与修改来源；依赖升级单独核对平台兼容和许可。
- 提交、推送、发布依当前用户指令，不隐式操作普通用户数据。真实 UI 使用隔离身份，
  桌面操作按会话授权执行；结束及时释放 capture 资源。
