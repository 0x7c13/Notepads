# 许可、来源与独立性

本项目原创人工 source、数据、checker 与文档依根目录 MIT LICENSE，Copyright (c) 2026, Jiaqi (0x7c13) Liu。

11 个模型及 source 复用既有独立冻结 m1-provenance.1/.2 数据；本目录保持完整模型语义并固定 parent 原文件 SHA/selector。旧两个 oracle、旧 187 个 point 控制和五个 Link.kind sidecar 均未修改。

partial-tab source 来自 John MacFarlane 的 CommonMark 0.31.2 Tabs / Indented code blocks 规范示例适配，沿用 CC-BY-SA-4.0 及原归因。冻结 parent 的输入路径为 artifacts/evidence/m1/e04-b01/frozen-1/tests/fixtures/m1-provenance/inputs/e04.container.partial-tab-code.md；原 > 后两个 U+2192 tab placeholder 按规范约定替换为 U+0009，保留 LF。规范固定 commit 9103e341a973013013bb1a80e13567007c5cef6f，原 spec 与许可见仓库 tests/baselines/commonmark-0.31.2/ 及既有 NOTICE。该适配来源的许可不改写为仅本项目所有。

其余 source 为既有 project controls 或本次原创窄控制。范围关系依批准 research 合同手算，不复用 parser/renderer 的输出作为答案；本次数据集合许可标记为 MIT AND CC-BY-SA-4.0。

人工 disjoint-alt 有意让 ![aXb](u) 的 Alt flow 仅拥有 a 与 b 两个 domain。它不是 Markdown parser 对该输入的投影，不宣称 X 在真实 Markdown alt 中应消失。Main Object 完整拥有语法，Alt hole 只验证所选 scope 的范围合同。

人工 32-Hidden 与三-Synthetic 控制使用已批准的 ControlOnly / ControlSeparator 理由，不虚构 Origin/Unknown 或 native grammar facts。
