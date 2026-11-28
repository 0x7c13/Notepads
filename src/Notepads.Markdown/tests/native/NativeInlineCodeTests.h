// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include <string_view>

namespace Notepads::Markdown::Native::InlineCodeTests
{
    inline constexpr std::string_view FixtureSource =
        "`chip`    ordinary **`bold`** [`link`](target), `😀 é abc שלום xyz`.\n\n"
        "punct (`left`),`right`! space ` ` and `x`.\n\n"
        "> quoted `quote` tail\n\n"
        "- | A | B |\n  |---|---|\n  | `table` one two three four five six seven eight | cell |\n\n"
        "`wrap one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen` tail\n\n"
        "edge `e`́ and e`́`.\n\n"
        "`image` ![placeholder](img)\n\n"
        "```text:line-numbers=98 {1-2}\n"
        "fenced `literal` with a wrapped physical line\u2028after its Unicode separator\n\nplain code line\n```\n\n"
        "<div>raw `html`</div>\n\n"
        "==Gold note==  vs <mark>HTML note</mark>.\n\n"
        "<sup><mark>raised</mark></sup> tail.\n";
    inline constexpr std::wstring_view FixtureMain =
        L"chip    ordinary bold link, 😀 é abc שלום xyz.\n"
        L"punct (left),right! space   and x.\n"
        L"quoted quote tail\nA\tB\ntable one two three four five six seven eight\tcell\n"
        L"wrap one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen tail\n"
        L"edge é and é.\n"
        L"image \xfffc\nfenced `literal` with a wrapped physical line\u2028after its Unicode separator\n\n"
        L"plain code line\n\n<div>raw `html`</div>\n\n"
        L"Gold note  vs HTML note.\nraised tail.";
    inline constexpr std::string_view HtmlTableFixtureSource =
        "<table><caption></caption><tbody><tr><td></td></tr></tbody></table>\r\n\r\n"
        "<table>\r\n<caption>HTML 表格与跨列</caption>\r\n<thead>\r\n"
        "<tr><th scope=\"col\">列一</th><th scope=\"col\">列二</th></tr>\r\n</thead>\r\n<tbody>\r\n"
        "<tr><td colspan=\"2\">跨两列，&amp; 实体测试</td></tr>\r\n"
        "<tr><td>R2C1</td><td><code>x &lt; y</code></td></tr>\r\n</tbody>\r\n</table>\r\n\r\n"
        "<table><tbody><tr><td colspan=2 rowspan=0>跨行列😀</td><td>第一行</td></tr>"
        "<tr><td>第二行</td></tr></tbody><tbody><tr><td colspan=3>下一组</td></tr></tbody></table>\r\n\r\n"
        "<table><tr><td rowspan=2 colspan=2>尾组</td></tr><tr></tr></table>";
    inline constexpr std::wstring_view HtmlTableFixtureMain =
        L"\n\nHTML 表格与跨列\t\n列一\t列二\n跨两列，& 实体测试\t\nR2C1\tx < y\n"
        L"跨行列😀\t\t第一行\n\t\t第二行\n下一组\t\t\n尾组\t\n\t";

    struct Results
    {
        bool authoredRanges{};
        bool visualFragments{};
        bool fundedBounds{};
        bool freshWarmGeometry{};
        bool themedPixels{};
        bool selectionOverBackground{};
        bool highContrastOutline{};
        bool tallLineCapped{};
        bool spanEdgePadding{};
        bool roundedTable{};
        bool tableMarkersOutsideClip{};
        uint64_t ranges{}, fragments{}, wrappedFragments{}, bidiFragments{}, cacheHits{};
        uint64_t paintedFragments{}, zeroAdvanceFragments{};
        uint64_t lightPixels{}, darkPixels{}, selectedPixels{}, outlinePixels{};
        uint64_t tableGridPixels{};
        uint64_t tableMarkerPixels{};
        float plainChipWidth{}, paddedChipWidth{};
        std::wstring detail;
    };

    // Background-only; the supplied accepted model must contain FixtureSource.
    // The enclosing reading case verifies native/preview ownership retirement.
    Results Run(std::shared_ptr<ReadingMemory> const &memory);
    std::wstring RunHtmlTables(std::shared_ptr<MainTextSnapshot const> const &main, notepads_markdown_core_cancel *sourceFlag);
}
