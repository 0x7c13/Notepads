// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Graphics.Display;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Automation.Text;
using WinUIEditor;
using SelectionMode = WinUIEditor.SelectionMode;
using static NotepadsEditorTests.EditorTestLayout;

namespace NotepadsEditorTests;

internal static partial class BidiTests
{
    private static async Task CheckPlainWrapAffinityAsync(TextEditorCore core, Editor native, EditorBaseControl view)
    {
        var originalWidth = core.Width;
        var originalHeight = core.Height;
        var originalWrap = core.TextWrapping;
        var originalRtl = core.RightToLeftReadingOrder;
        var provider = (ITextProvider2)FrameworkElementAutomationPeer.CreatePeerForElement(view)
            .GetPattern(PatternInterface.Text);
        var source = new string('a', 200);
        var inserted = source + "a";
        try
        {
            foreach (var width in new[] { 220d, 300d })
            {
                await AfterPaintAsync(native, () =>
                {
                    core.Width = width;
                    core.Height = 160;
                    core.TextWrapping = TextWrapping.Wrap;
                    core.RightToLeftReadingOrder = false;
                    native.SelectionMode = SelectionMode.Stream;
                    core.SetText(source);
                    native.SetEmptySelection(0);
                    native.FirstVisibleLine = 0;
                    core.UpdateLayout();
                });
                Check(native.WrapCount(0) > native.LinesOnScreen,
                    "Plain wrap fixture requires a vertical scrollbar");
                // The first paint can reveal the scrollbar and narrow the text
                // area. Arrange it and repaint before measuring the wrap offset.
                await AfterPaintAsync(native, () =>
                {
                    core.UpdateLayout();
                    native.FirstVisibleLine = 1;
                    native.FirstVisibleLine = 0;
                });
                var firstY = native.PointYFromPosition(0);
                var boundary = Enumerable.Range(1, source.Length - 1).Select(position => (long)position)
                    .First(position => native.PointYFromPosition(position) > firstY);
                Check(boundary > 1 && native.PointYFromPosition(boundary - 1) == firstY,
                    $"Plain wrap fixture has a measured first-row boundary: width={width}, boundary={boundary}");
                native.SetEmptySelection(boundary - 1);
                native.EmptyUndoBuffer();
                await AfterPaintAsync(native, () => core.TypeText("a"));
                Equal(inserted, core.GetText(), "Plain wrap insertion preserves every existing character");
                Check(native.PointYFromPosition(boundary - 1) < native.PointYFromPosition(boundary),
                    "Inserting an equal-width glyph leaves the measured wrap boundary unchanged");

                double[] GlyphBounds(long position)
                {
                    var glyph = provider.DocumentRange.Clone();
                    glyph.MoveEndpointByRange(TextPatternRangeEndpoint.End, glyph, TextPatternRangeEndpoint.Start);
                    Check(glyph.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, TextUnit.Character, (int)position) == position &&
                        glyph.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 1) == 1,
                        "Plain wrap UIA fixture addresses exactly one character");
                    return Rectangles(glyph);
                }

                void CheckBoundaryCaret(string operation)
                {
                    var caret = Rectangles(provider.GetCaretRange(out _));
                    var before = GlyphBounds(boundary - 1);
                    var after = GlyphBounds(boundary);
                    Check(native.CurrentPos == boundary && native.Anchor == boundary && caret.Length == 4 &&
                        before.Length == 4 && after.Length == 4 && after[1] > before[1] &&
                        Math.Abs(caret[0] - after[0]) < 1 && Math.Abs(caret[1] - after[1]) < 1,
                        $"{operation} keeps the plain caret at the next row's start: width={width}, boundary={boundary}");
                }

                CheckBoundaryCaret("Insertion");
                await AfterPaintAsync(native, () =>
                {
                    core.Height = native.TextHeight(0) / DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
                    core.UpdateLayout();
                });
                Check(native.LinesOnScreen == 1 &&
                    native.PointYFromPosition(boundary - 1) < native.PointYFromPosition(boundary),
                    "One-row viewport preserves the plain wrap boundary");
                native.FirstVisibleLine = 0;
                for (var repeat = 0; repeat < 3; repeat++) native.ScrollCaret();
                Check(native.FirstVisibleLine == 1,
                    $"ScrollCaret follows the plain caret onto the next row: width={width}, first={native.FirstVisibleLine}");
                await AfterPaintAsync(native, () =>
                {
                    core.Height = 160;
                    core.UpdateLayout();
                    native.FirstVisibleLine = 0;
                });
                CheckBoundaryCaret("Viewport resizing");

                // Keep the insertion's trailing affinity: resetting the caret
                // here would conceal the row disagreement this test exercises.
                native.CharRightRectExtend();
                Check(native.SelectionMode == SelectionMode.Rectangle && native.Selections == 1 &&
                    native.GetSelectionNStart(0) == boundary && native.GetSelectionNEnd(0) == boundary + 1,
                    $"Rectangle extension selects only the next-row character: width={width}, boundary={boundary}, selections={native.Selections}");
                core.TypeText("X");
                var replaced = inserted.Remove((int)boundary, 1).Insert((int)boundary, "X");
                Equal(replaced, core.GetText(), "Plain wrapped rectangle replacement changes only its selected character");
                core.Undo();
                Equal(inserted, core.GetText(), "One undo restores only the rectangular replacement");
                core.Undo();
                Equal(source, core.GetText(), "The second undo removes only the initial wrap-boundary insertion");
                Check(!core.CanUndo, "Plain wrap insertion and rectangle replacement create exactly two undo actions");
                core.Redo();
                Equal(inserted, core.GetText(), "Redo restores the initial insertion exactly");
                CheckBoundaryCaret("Redo insertion");
                core.Redo();
                Equal(replaced, core.GetText(), "Redo restores only the selected rectangular character");
            }
        }
        finally
        {
            native.SelectionMode = SelectionMode.Stream;
            await AfterPaintAsync(native, () =>
            {
                core.Width = originalWidth;
                core.Height = originalHeight;
                core.TextWrapping = originalWrap;
                core.RightToLeftReadingOrder = originalRtl;
                core.UpdateLayout();
            });
        }
    }

    private static async Task CheckWordTransitionsAsync(TextEditorCore core, Editor native)
    {
        // Fixed byte positions exercise the native command, including its
        // physical-line transition. No geometry query repairs the chosen home.
        var cases = new (string Text, bool Rtl, long Start, long Expected, bool Right)[]
        {
            ("abc\r    xyz def", false, 3, 8, true),
            ("abc\r    אבג def", false, 3, 8, true),
            ("אבג abc\r    xyz def", false, 10, 15, true),
            ("abc uvw xyz    \rdef", false, 16, 15, false),
            ("abc אבג xyz    \rdef", false, 19, 18, false),
            ("אבג\r    דהו זחט", true, 6, 11, false),
            ("אבג דהו    \rזחט", true, 18, 17, true),
            ("abc\r\r   \r    xyz", false, 3, 8, true),
            ("abc\r\r   \r    אבג", false, 3, 8, true),
            ("אבג\r\r   \r    דהו", true, 6, 11, false)
        };
        foreach (var fixture in cases)
        foreach (var extend in new[] { false, true })
        {
            await LoadAsync(core, native, fixture.Text, fixture.Rtl);
            native.SetEmptySelection(fixture.Start);
            if (fixture.Right)
            {
                if (extend) native.WordRightExtend(); else native.WordRight();
            }
            else
            {
                if (extend) native.WordLeftExtend(); else native.WordLeft();
            }
            Check(native.CurrentPos == fixture.Expected &&
                native.Anchor == (extend ? fixture.Start : fixture.Expected),
                $"Word transition preserves its boundary and anchor: rtl={fixture.Rtl}, right={fixture.Right}, " +
                $"extend={extend}, start={fixture.Start}, expected={fixture.Expected}, caret={native.CurrentPos}, anchor={native.Anchor}");
            Equal(fixture.Text, core.GetText(), "Cross-paragraph navigation preserves every document byte");
        }

        // A word command starting inside a paragraph first reaches its edge;
        // only the next invocation crosses to the following paragraph's word.
        var edgeCases = new (string Text, bool Rtl, long Start, long Edge, long Next)[]
        {
            ("abc xyz\r    uvw", false, 4, 7, 12),
            ("אבג abc\r    xyz", false, 7, 10, 15),
            ("אבג דהו\r    זחט", true, 7, 13, 18)
        };
        foreach (var fixture in edgeCases)
        foreach (var extend in new[] { false, true })
        {
            await LoadAsync(core, native, fixture.Text, fixture.Rtl);
            native.SetEmptySelection(fixture.Start);
            foreach (var expected in new[] { fixture.Edge, fixture.Next })
            {
                if (fixture.Rtl)
                {
                    if (extend) native.WordLeftExtend(); else native.WordLeft();
                }
                else
                {
                    if (extend) native.WordRightExtend(); else native.WordRight();
                }
                Check(native.CurrentPos == expected && native.Anchor == (extend ? fixture.Start : expected),
                    $"Word movement preserves the source paragraph edge: rtl={fixture.Rtl}, extend={extend}, " +
                    $"expected={expected}, caret={native.CurrentPos}, anchor={native.Anchor}");
            }
        }

        foreach (var text in new[] { "abc\r    xyz def", "abc\r    אבג def" })
        {
            await LoadAsync(core, native, text, false);
            native.SetEmptySelection(3);
            native.CharRight();
            Check(native.CurrentPos == 4, "A character command crosses the newline without consuming indentation");
            native.SetEmptySelection(3);
            native.WordRightEnd();
            Check(native.CurrentPos == 4,
                $"Word-end movement preserves the newline's separate character class: caret={native.CurrentPos}");
        }

        await LoadAsync(core, native, "abc\r    fooBar אבג", false);
        native.SetEmptySelection(3);
        native.WordPartRight();
        Check(native.CurrentPos == 8, "Word-part navigation consumes cross-paragraph whitespace");
        native.WordPartRight();
        Check(native.CurrentPos == 11, "Word-part navigation still stops at the camel-case boundary");
        native.WordPartLeft();
        Check(native.CurrentPos == 8, "Reverse word-part movement returns to the camel-case prefix");
    }

    private static async Task CheckRectangleTabAsync(TextEditorCore core, Editor native)
    {
        var originalTabs = native.UseTabs;
        var originalTabIndents = native.TabIndents;
        var originalTabWidth = native.TabWidth;
        var originalIndent = native.Indent;
        var originalWidth = core.Width;
        var originalWrap = core.TextWrapping;
        try
        {
            native.TabWidth = native.Indent = 4;
            native.TabIndents = true;
            foreach (var rtl in new[] { false, true })
            foreach (var tabs in new[] { false, true })
            foreach (var reverse in new[] { false, true })
            {
                var line = rtl ? "אבג ABC" : "abc אבג def";
                var source = line + "\r" + line;
                var lastLine = Encoding.UTF8.GetByteCount(line) + 1;
                var left = rtl ? 0 : 1;
                var right = lastLine + 8;
                await LoadAsync(core, native, source, rtl);
                native.UseTabs = tabs;
                native.EmptyUndoBuffer();
                native.SelectionMode = SelectionMode.Rectangle;
                native.RectangularSelectionAnchor = reverse ? right : left;
                native.RectangularSelectionCaret = reverse ? left : right;
                Check(native.Selections == 4, "Tab fixture has two logical islands on each of two visual rows");
                var indentation = tabs ? "\t" : new string(' ', rtl ? 4 : 3);
                string Expected(string suffix) => string.Join('\r', Enumerable.Repeat(
                    rtl ? indentation + suffix + "A" : "a" + indentation + suffix + "אב def", 2));

                var selections = Enumerable.Range(0, native.Selections).Select(index => (
                    Anchor: native.GetSelectionNAnchor(index), Caret: native.GetSelectionNCaret(index),
                    AnchorVirtualSpace: native.GetSelectionNAnchorVirtualSpace(index),
                    CaretVirtualSpace: native.GetSelectionNCaretVirtualSpace(index))).ToArray();
                var mainSelection = native.MainSelection;
                var rectangle = (native.RectangularSelectionAnchor, native.RectangularSelectionCaret,
                    native.RectangularSelectionAnchorVirtualSpace, native.RectangularSelectionCaretVirtualSpace);
                native.ReadOnly = true;
                native.Tab();
                Equal(source, core.GetText(), "Read-only rectangular Tab leaves every selected island intact");
                Check(native.SelectionMode == SelectionMode.Rectangle && native.MainSelection == mainSelection &&
                    native.Selections == selections.Length,
                    "Read-only Tab preserves rectangle mode, the main selection and every logical island");
                Check(rectangle == (native.RectangularSelectionAnchor, native.RectangularSelectionCaret,
                    native.RectangularSelectionAnchorVirtualSpace, native.RectangularSelectionCaretVirtualSpace),
                    "Read-only Tab preserves both rectangular corners and their virtual columns");
                for (var index = 0; index < selections.Length; index++)
                {
                    Check(selections[index] == (native.GetSelectionNAnchor(index), native.GetSelectionNCaret(index),
                        native.GetSelectionNAnchorVirtualSpace(index), native.GetSelectionNCaretVirtualSpace(index)),
                        $"Read-only Tab preserves the oriented endpoints of island {index}: rtl={rtl}, tabs={tabs}, reverse={reverse}");
                }
                native.ReadOnly = false;
                // CanUndo is suppressed while read-only. Check the empty-history
                // fixture after restoring write access so that guard cannot hide an edit.
                Check(!native.CanUndo() && !native.CanRedo(), "Read-only Tab adds no undo or redo action");
                native.Tab();
                Equal(Expected(string.Empty), core.GetText(),
                    $"Tab inserts once per visual row: rtl={rtl}, tabs={tabs}, reverse={reverse}");
                Check(native.SelectionEmpty && native.Selections == 2,
                    "Tab retains one empty insertion anchor per visual row");
                core.TypeText("X");
                Equal(Expected("X"), core.GetText(), "Typing after rectangular Tab uses its two insertion anchors");
                core.Undo();
                Equal(Expected(string.Empty), core.GetText(), "One undo removes only subsequent rectangle typing");
                core.Undo();
                Equal(source, core.GetText(), "One undo restores all original islands and removes every inserted tab/space");
                Check(!core.CanUndo, "Rectangle Tab is exactly one undo action");
                core.Redo();
                Equal(Expected(string.Empty), core.GetText(), "Redo replays the exact rectangular Tab edit");
            }

            // Wrapped rows share one physical line. Reflow during insertion
            // must not move the second row's pre-edit logical anchor.
            native.UseTabs = true;
            const string glyph = "א";
            var wrapped = string.Concat(Enumerable.Repeat(glyph, 64));
            await AfterPaintAsync(native, () =>
            {
                core.Width = 220;
                core.TextWrapping = TextWrapping.Wrap;
                core.RightToLeftReadingOrder = true;
                core.SetText(wrapped);
                native.SetEmptySelection(0);
                core.UpdateLayout();
            });
            var firstY = native.PointYFromPosition(0);
            var secondRow = Enumerable.Range(1, 63).Select(index => (long)index * 2)
                .First(position => native.PointYFromPosition(position) > firstY);
            native.EmptyUndoBuffer();
            native.SelectionMode = SelectionMode.Rectangle;
            native.RectangularSelectionAnchor = 2;
            native.RectangularSelectionCaret = secondRow + 4;
            Check(native.Selections == 2 &&
                Enumerable.Range(0, 2).Select(i => native.GetSelectionNStart(i)).Order().SequenceEqual(new[] { 2L, secondRow + 2 }),
                "Wrapped Tab selects the second glyph on each of two display rows");
            var expected = wrapped.Remove((int)(secondRow / 2) + 1, 1).Insert((int)(secondRow / 2) + 1, "\t")
                .Remove(1, 1).Insert(1, "\t");
            native.Tab();
            Equal(expected, core.GetText(), "Wrapped rectangular Tab replaces each selected glyph once without adding physical lines");
            core.Undo();
            Equal(wrapped, core.GetText(), "Wrapped rectangular Tab is undone as one operation");
            Check(!core.CanUndo, "Wrapped Tab adds no extra undo boundary");
        }
        finally
        {
            native.ReadOnly = false;
            native.SelectionMode = SelectionMode.Stream;
            native.UseTabs = originalTabs;
            native.TabIndents = originalTabIndents;
            native.TabWidth = originalTabWidth;
            native.Indent = originalIndent;
            await AfterPaintAsync(native, () =>
            {
                core.Width = originalWidth;
                core.TextWrapping = originalWrap;
                core.UpdateLayout();
            });
        }
    }
}
