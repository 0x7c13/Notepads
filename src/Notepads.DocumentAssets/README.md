# Notepads.DocumentAssets

文档相对资源读取 broker，供 App 的 `Infrastructure/DocumentAssets/DocumentAssetClient`
使用。此组件通过已授权文档 token 读取同目录及子目录中的图片和嵌入笔记。
App 解码笔记并把 UTF-8 文本交给 Markdown 控件。子笔记解析相对资源时仍使用原始文档授权。

- `Program.cpp`：固定 `Files.ReadDocumentAsset` 请求、会话 lease、路径/最终 handle
  边界、大小限制和响应。无任意进程控制或通用文件访问入口。
- `Notepads.DocumentAssets.vcxproj`：按 Win32/x64/ARM64 构建的小型桌面进程。
- `build/Notepads.DocumentAssets.Consumer.targets`：App 的构建与内容部署。

相对路径逐段从已验证的目录 handle 打开；各段不跟随符号链接、junction 或其他 reparse point，
普通子目录文件仍按同一最终 handle 校验并读取。

App 保留打包/激活策略，开发及离线布局在需要时提供此 broker。协议双方必须同步修改；
超时、取消、token 释放、会话退役和路径边界不能在目录整理时省略。
主许可证见仓根 [LICENSE.txt](../../LICENSE.txt)。
