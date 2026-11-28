// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;

namespace NotepadsIntegrationTests;

internal sealed partial class MarkdownKeyboardFixture
{
    // Fixed authored CASE 02-A/02-C inputs from the user's Rendering Lab. This
    // fixture owns only the isolated test document; the user's file is untouched.
    private const string QuoteListSource = "# 列表与引用对齐\r\r" +
        "- 宽松列表第一项\r\r  同一列表项的第二个段落。\r\r" +
        "  > 列表项内的引用。\r\r  ```text\r  列表项内的围栏代码\r  ```\r\r" +
        "- 宽松列表第二项。\r";
    private const string QuoteNestedSource = "# 多级引用与组合结构\r\r" +
        "> 第一层引用，下一源行省略引用符。\r这是惰性续行。\r>\r" +
        "> > 第二层引用\r> >\r> > - 引用里的列表\r> > - 引用里的 [锚点](#06-math)\r> >\r" +
        "> > > 第三层引用\r>\r> 引用里的行内公式 $a^2+b^2=c^2$。\r\r" +
        "普通段落，引用背景在此之前结束。\r\r> 独立的同级引用。\r\r## 06-math\r\r锚点目标。\r";

    internal async Task PrepareQuotesAsync(string scenario)
    {
        var source = scenario switch
        {
            "list" => QuoteListSource,
            "nested" => QuoteNestedSource,
            "mixed" => QuoteListSource + "\r" + QuoteNestedSource +
                "\r- > 列表以引用开头 Ω😀\r\r- [x] \r\r  > 任务项内的引用\r",
            _ => throw new ArgumentException("Declared quote fixture scenarios only.")
        };
        _editor.TestNativeCore.SetText(source);
        _editor.TestNativeCore.TestFeatureNative.SetEmptySelection(0);
        if (!_editor.TestMarkdownPanelOpen) _editor.ShowHideContentPreview();
        var extension = await MarkdownFeatureTests.CurrentAsync(_editor);
        extension.FollowSource = false;
        extension.PreviewView.TrySetVerticalScrollProgress(0);
    }
}
