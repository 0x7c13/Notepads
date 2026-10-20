// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.DiffViewer;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsEditorTests;
using NotepadsWorkspaceTests;
using Windows.Graphics.Display;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using WinUIEditor;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class DiffSourceViewportTests
{
    internal static async Task RunAsync(FeatureTestHost host, StringBuilder log)
    {
        foreach (var (rtl, wrap, offset) in new[]
        {
            (true, false, 0d), (true, false, 120d), (true, true, 0d),
            (false, false, 120d), (false, true, 0d)
        })
        {
            var label = $"Diff return RTL={rtl}, wrap={wrap}, leading={offset}";
            var line = string.Concat(Enumerable.Repeat(rtl ? "שלום אבג " : "plain text ", 40));
            var text = string.Join('\r', Enumerable.Range(0, 200).Select(index => line + index));
            var editor = await DocumentFeatureAssertions.OpenAsync(host,
                $"diff-return-{rtl}-{wrap}-{offset}.txt", Encoding.UTF8.GetBytes(text));
            var core = editor.TestNativeCore;
            var view = (EditorBaseControl)((Grid)core.Content).Children[0];
            var native = view.Editor;
            const int targetLine = 30;
            var targetSubLine = wrap ? 1 : 0;
            var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
            await EditorTestLayout.AfterPaintAsync(native, () =>
            {
                core.Width = 600;
                core.RightToLeftReadingOrder = rtl;
                core.TextWrapping = wrap ? TextWrapping.Wrap : TextWrapping.NoWrap;
                core.DisplayLineNumbers = true;
                core.SetFontZoomFactor(100);
                native.GotoPos(0);
                editor.TypeText(rtl ? "ש" : "x");
                core.SetScrollViewerInitPosition(offset, 0, targetLine, targetSubLine);
            });
            var changedText = editor.GetText();
            var sequence = editor.DocumentSequence;
            var caret = native.CurrentPos;
            var oldRow = native.FirstVisibleLine;
            var oldX = native.XOffset;
            var surface = Descendants(view).OfType<Border>().Single(element => element.Name == "ImageTarget");

            (long Line, long SubLine, double Offset, bool Geometry) Observe()
            {
                var row = native.FirstVisibleLine;
                var x = native.XOffset;
                var documentLine = native.DocLineFromVisible(row);
                var expectedX = rtl
                    ? surface.ActualWidth * scale - native.MarginRight - native.GetMarginWidthN(0) - native.CaretWidth + offset * scale
                    : native.MarginLeft + native.GetMarginWidthN(0) - offset * scale;
                var geometry = wrap || Math.Abs(native.PointXFromPosition(native.PositionFromLine(targetLine)) - expectedX) <= 1;
                // Geometry queries must observe the painted state, not repair it.
                geometry &= native.FirstVisibleLine == row && native.XOffset == x;
                return (documentLine, row - native.VisibleFromDocLine(documentLine), view.DisplayHorizontalOffset, geometry);
            }
            bool Valid((long Line, long SubLine, double Offset, bool Geometry) state) =>
                !view.IsViewportRestorePending && state.Line == targetLine && state.SubLine == targetSubLine &&
                Math.Abs(state.Offset - offset) <= 1 / scale && state.Geometry &&
                native.CurrentPos == caret && native.Anchor == caret;

            Check(Valid(Observe()) && (!wrap || native.WrapCount(targetLine) > 2), label +
                $": source fixture has the requested visible geometry: observed={Observe()}, pending={view.IsViewportRestorePending}, " +
                $"row={native.FirstVisibleLine}, x={native.XOffset}, glyphX={native.PointXFromPosition(native.PositionFromLine(targetLine))}, " +
                $"width={surface.ActualWidth * scale}, caret={native.CurrentPos}/{native.Anchor}, expectedCaret={caret}.");
            await editor.OpenSideBySideDiffViewerAsync();
            try
            {
                var viewer = Named<ContentPresenter>(editor, "DiffPresenter").Content as SideBySideDiffViewer;
                Check(viewer != null, label + ": the actual source editor opens its diff.");
                var pane = Named<TextEditorCore>(viewer, "NewEditor");
                await UntilAsync(() => pane.IsLoaded && pane.ActualHeight > 0 && core.ActualHeight == 0,
                    label + ": the diff is arranged and the source is hidden.");
                await EditorTestLayout.AfterPaintAsync(pane.TestFeatureNative, () => pane.SetFontZoomFactor(160));
                Check(core.GetFontZoomFactor() == 160, label + ": actual diff zoom updates the hidden source.");

                var observations = new List<(long Line, long SubLine, double Offset, bool Geometry)>();
                var validPaints = new List<bool>();
                void Painted(WinUIEditor.Editor sender, PaintedEventArgs args)
                {
                    var state = Observe();
                    observations.Add(state);
                    validPaints.Add(Valid(state));
                }
                native.Painted += Painted;
                try
                {
                    await EditorTestLayout.AfterPaintAsync(native, editor.CloseSideBySideDiffViewer);
                    // Include a later focus/selection repaint without scrolling or repairing the viewport.
                    await EditorTestLayout.AfterPaintAsync(native, () => native.SetSelection(caret, caret));
                    Check(validPaints.Count >= 2 && validPaints.All(valid => valid),
                        label + ": every returning paint preserves the source anchor and leading geometry; observed " +
                        string.Join(", ", observations.Take(6)));
                    if (wrap)
                        Check(native.FirstVisibleLine != oldRow, label + ": zoom changes the display row behind the retained document anchor.");
                    else if (rtl)
                        Check(native.XOffset != oldX, label + ": zoom changes the physical X behind the retained leading offset.");
                    Check(editor.GetText() == changedText && editor.DocumentSequence == sequence,
                        label + ": diff zoom and return leave source bytes and journal sequence unchanged.");
                }
                finally { native.Painted -= Painted; }
            }
            finally { editor.CloseSideBySideDiffViewer(); }
            log.AppendLine($"PASS: {label} retains its anchor and geometry from the first source paint after zoom.");
        }
        await CheckPreferredColumnAsync(host, log, false);
        await CheckPreferredColumnAsync(host, log, true);
    }

    private static async Task CheckPreferredColumnAsync(FeatureTestHost host, StringBuilder log, bool rtl)
    {
        var character = rtl ? 'א' : 'a';
        var longLine = new string(character, 20);
        var editor = await DocumentFeatureAssertions.OpenAsync(host, $"diff-column-{rtl}.txt",
            Encoding.UTF8.GetBytes($"{longLine}\r{character}\r{longLine}"));
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        await EditorTestLayout.AfterPaintAsync(native, () =>
        {
            core.RightToLeftReadingOrder = rtl;
            core.TextWrapping = TextWrapping.NoWrap;
            core.SetFontZoomFactor(100);
            native.GotoPos(native.Length);
            editor.TypeText(character.ToString());
        });
        var bytesPerCharacter = rtl ? 2 : 1;
        var chosenPosition = 10 * bytesPerCharacter;
        native.GotoPos(chosenPosition);
        native.ChooseCaretX();
        native.LineDown();
        Check(native.CurrentPos == native.GetLineEndPosition(1),
            "The short line clamps the caret while retaining the long line's preferred column.");
        native.LineUp();
        Check(native.CurrentPos == chosenPosition,
            "Before Diff, an actual short-line round trip proves the fixture retained column 10.");
        native.LineDown();
        await editor.OpenSideBySideDiffViewerAsync();
        try
        {
            var viewer = Named<ContentPresenter>(editor, "DiffPresenter").Content as SideBySideDiffViewer;
            Check(viewer != null, "The preferred-column fixture opens the actual Diff viewer.");
            await UntilAsync(() => viewer.IsLoaded && core.ActualHeight == 0,
                "The source is hidden before returning without a zoom change.");
            await EditorTestLayout.AfterPaintAsync(native, editor.CloseSideBySideDiffViewer);
            // No GotoPos, ChooseCaretX or click may repair the retained column.
            native.LineDown();
            var expected = native.PositionFromLine(2) + chosenPosition;
            Check(native.CurrentPos == expected,
                $"Diff return RTL={rtl} keeps column 10 on the next long line; expected byte {expected}, actual {native.CurrentPos}.");
            native.LineUp();
            native.LineUp();
            Check(native.CurrentPos == chosenPosition && native.Anchor == chosenPosition,
                "Returning Up preserves the same preferred column and collapsed selection.");
        }
        finally { editor.CloseSideBySideDiffViewer(); }
        log.AppendLine($"PASS: Diff return RTL={rtl} preserves the preferred column across short lines.");
    }
}
