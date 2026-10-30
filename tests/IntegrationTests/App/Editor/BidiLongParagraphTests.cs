// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Automation.Text;
using WinUIEditor;
using static NotepadsEditorTests.EditorTestLayout;

namespace NotepadsEditorTests;

internal static partial class BidiTests
{
    private static async Task CheckLongParagraphAsync(TextEditorCore core, Editor native, EditorBaseControl view)
    {
        const string unit = "abc אבג def ";
        var text = string.Concat(Enumerable.Repeat(unit, 2000));
        var character = unit.Length * 1800 + 4;
        var start = Encoding.UTF8.GetByteCount(text.AsSpan(0, character));
        var end = start + Encoding.UTF8.GetByteCount("אבג");
        var provider = (ITextProvider)FrameworkElementAutomationPeer.CreatePeerForElement(view).GetPattern(PatternInterface.Text);
        foreach (var rtl in new[] { false, true })
        {
            await LoadAsync(core, native, text, rtl);
            await AfterPaintAsync(native, () => native.GotoPos(start));
            var originalX = native.PointXFromPosition(start);
            var range = provider.DocumentRange.Clone();
            range.MoveEndpointByRange(TextPatternRangeEndpoint.End, range, TextPatternRangeEndpoint.Start);
            Check(range.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, TextUnit.Character, character) == character &&
                range.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 3) == 3,
                "Long paragraph UIA fixture reaches its distant Hebrew word");
            Equal("אבג", range.GetText(-1), "Long paragraph UIA range uses exact logical text");
            CheckRectangles(Rectangles(range), false);
            range.Select();
            Check(native.SelectionStart == start && native.SelectionEnd == end,
                "Distant UIA selection preserves exact UTF-8 bounds");
            native.EmptyUndoBuffer();
            await AfterPaintAsync(native, () => core.TypeText("X"));
            Equal(text.Remove(character, 3).Insert(character, "X"), core.GetText(), "Distant replacement changes only the selected word");
            await AfterPaintAsync(native, core.Undo);
            Equal(text, core.GetText(), "One undo restores the complete long paragraph");
            Check(!core.CanUndo, "Long replacement has a single undo boundary");
            await AfterPaintAsync(native, () => native.GotoPos(start));
            Check(Math.Abs(native.PointXFromPosition(start) - originalX) <= 1,
                "Undo restores distant caret geometry on the same canvas");
            native.EmptyUndoBuffer();
            await AfterPaintAsync(native, () => native.GotoPos(native.Length));
            await AfterPaintAsync(native, () => core.TypeText("\u200c"));
            Equal(text + "\u200c", core.GetText(), "Adding a zero-width tail safely switches to native paragraph layout");
            await AfterPaintAsync(native, core.Undo);
            Equal(text, core.GetText(), "Undo safely restores the eligible dense paragraph");
            Check(!core.CanUndo, "Backend transition retains one undo boundary");
            await AfterPaintAsync(native, () => native.GotoPos(start));
            Check(Math.Abs(native.PointXFromPosition(start) - originalX) <= 1,
                "Returning from zero-width native fallback restores distant caret geometry");
            await AfterPaintAsync(native, () => core.TextWrapping = TextWrapping.Wrap);
            Equal(text, core.GetText(), "Native wrapped fallback retains the exact long document");
            await AfterPaintAsync(native, () => core.TextWrapping = TextWrapping.NoWrap);
            await AfterPaintAsync(native, () => native.GotoPos(start));
            Check(Math.Abs(native.PointXFromPosition(start) - originalX) <= 1,
                "Returning from wrapped fallback restores distant unwrapped geometry");
        }
    }
}
