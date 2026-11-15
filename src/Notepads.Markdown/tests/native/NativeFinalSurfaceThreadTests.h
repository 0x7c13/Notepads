// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.UI.Core.h>
#include <exception>
#include <string>
#include <string_view>

namespace Notepads::Markdown::Native::FinalSurfaceThreadTests
{
    // Frozen verbatim from cold-table-packet-v3/tests-source NativeTextBitmapCacheTests.h.
    // That header SHA-256: 0b5c028e20f1c83eae37bb5ca6837799f1ca25cd35c313a8cc91ebb7671ca423.
    // Original CRLF excerpt SHA-256:
    // 9d932c8aef359db8ec4a5265b2f772335dc98682a0cb5a98acd52e04132434b7.
    // Exact original Markdown Rendering Lab.md lines 1013-1026 (including the blank line).
    // Full original UTF-8 file SHA-256:
    // 05efaaff58a1738c3c29a934b3f4e1ec791f03846173aeee5d1e2d30f602cd21.
    // Independent Main below is authored explicitly, never produced by the parser/oracle.
    // Source escapes spell the original TAB/SHY/format controls; no normalization is permitted.
    inline constexpr std::string_view TablePacketFixtureSource =
        "| 码点 | 实际样本 | 观察 |\r\n"
        "| --- | --- | --- |\r\n"
        "| U+0009 TAB | [A\tB] | 比较正文与下方等宽代码 |\r\n"
        "| U+0020 / U+00A0 / U+202F | [12 345] [12\u00a0345] [12\u202f345] | 普通、不断行、窄不断行 |\r\n"
        "| U+2002 / U+2003 / U+3000 | [A\u2002B] [A\u2003B] [A\u3000B] | en、em、表意空格 |\r\n"
        "| U+200B ZWSP | [micro\u200bservice] | 零宽可断点 |\r\n"
        "| U+2060 WJ | [micro\u2060service] | 零宽禁断点 |\r\n"
        "| U+00AD SHY | [hy\u00adphen\u00adation] | 软连字符在窄栏的处理 |\r\n"
        "| U+200C ZWNJ / U+200D ZWJ | [می\u200cروم] [👩\u200d💻] | 断连与连接 |\r\n"
        "| U+200E LRM / U+200F RLM | [A\u200e-12] [ا\u200f-12] | 标点和数字方向 |\r\n"
        "| U+2067 RLI / U+2069 PDI | [ABC \u2067עברית 123\u2069 XYZ] | 有边界的 RTL 隔离 |\r\n"
        "| U+FEFF | [A\ufeffB] | 正文里的零宽字符 |\r\n"
        "| U+FE0E / U+FE0F | [✈\ufe0e] [✈\ufe0f] | 文本和 emoji 变体选择 |\r\n"
        "\r\n";
    inline constexpr std::wstring_view TablePacketFixtureMain =
        L"码点\t实际样本\t观察\n"
        L"U+0009 TAB\t[A\tB]\t比较正文与下方等宽代码\n"
        L"U+0020 / U+00A0 / U+202F\t[12 345] [12\u00a0345] [12\u202f345]\t普通、不断行、窄不断行\n"
        L"U+2002 / U+2003 / U+3000\t[A\u2002B] [A\u2003B] [A\u3000B]\ten、em、表意空格\n"
        L"U+200B ZWSP\t[micro\u200bservice]\t零宽可断点\n"
        L"U+2060 WJ\t[micro\u2060service]\t零宽禁断点\n"
        L"U+00AD SHY\t[hy\u00adphen\u00adation]\t软连字符在窄栏的处理\n"
        L"U+200C ZWNJ / U+200D ZWJ\t[می\u200cروم] [👩\u200d💻]\t断连与连接\n"
        L"U+200E LRM / U+200F RLM\t[A\u200e-12] [ا\u200f-12]\t标点和数字方向\n"
        L"U+2067 RLI / U+2069 PDI\t[ABC \u2067עברית 123\u2069 XYZ]\t有边界的 RTL 隔离\n"
        L"U+FEFF\t[A\ufeffB]\t正文里的零宽字符\n"
        L"U+FE0E / U+FE0F\t[✈\ufe0e] [✈\ufe0f]\t文本和 emoji 变体选择";

    // Diagnostic only: UI-begun, worker-resumed same-lease SIS byte equivalence.
    // Call on a live, stable UI dispatcher and await before destroying detail/failure.
    // Owning coroutine parameters are values. Authored failures are transported in
    // failure as TextBitmapCacheTests::Failure; the caller rethrows on its native path.
    // No worker-Begin, production publication, shutdown, screen or performance qualification.
    winrt::Windows::Foundation::IAsyncAction RunAsync(
        std::shared_ptr<ReadingMemory> memory,
        winrt::Windows::UI::Core::CoreDispatcher dispatcher,
        std::wstring &detail, std::exception_ptr &failure);
}
