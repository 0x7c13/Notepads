# Notepads Markdown math

有界原生数学适配：固定 RaTeX fork、KaTeX 字体和数值路径输出。
产品 FFI 使用此 crate；不通过 HTML、浏览器或 JavaScript 渲染。

`render` 接收执行控制、配额、字体资产和可信 worker 栈约束，返回拥有路径/指标及其
资源 lease 的 artifact。取消、deadline、深度和内存配额保持显式；无字体/layout
对象穿过 FFI。Windows worker/coordinator/presentation 在控件的 `Resources/Math/`。

此 crate 保持独立 Cargo workspace/lock，统一测试入口为：

```powershell
./src/Notepads.Markdown/scripts/Test-Rust.ps1 -Layer Math
```

固定字体与原 hash 见 `assets/ASSETS.json`，保留 RaTeX/KaTeX 和 SIL OFL 等原条款。
原始 oracle 位于模块 `tests/fixtures/math-upstream-oracle-v1/`，不可用当前输出重生成。
支持范围和平台资格以实际测试为准；解析/路径单测不代替真实 Native AOT/UI 验收。
