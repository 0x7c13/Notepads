// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Storage;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using WinUIEditor;

namespace NotepadsEditorTests;

internal static class DiffViewTests
{
    private static Editor Native(TextEditorCore core) => ((EditorBaseControl)((Grid)core.Content).Children[0]).Editor;
    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException("DiffView: " + message);
    }

    internal static async Task RunAsync(StringBuilder log, TextEditorCore source)
    {
        using var old = new TextEditorCore();
        using var current = new TextEditorCore();
        old.ConfigureDiffPreview();
        current.ConfigureDiffPreview();
        var host = new Grid();
        host.ColumnDefinitions.Add(new ColumnDefinition());
        host.ColumnDefinitions.Add(new ColumnDefinition());
        host.Children.Add(old);
        host.Children.Add(current);
        Grid.SetColumn(current, 1);
        Window.Current.Content = host;
        Window.Current.Activate();
        await Task.Delay(100);

        foreach (var unchangedText in new[] { string.Empty, "unchanged\r你好😀\r" })
        {
            source.SetText(unchangedText);
            using var unchangedBaseline = unchangedText.Length == 0 ? DocumentBaseline.CreateEmpty() : await DocumentBaseline.FromTextAsync(unchangedText);
            await source.LoadDiffSnapshotsAsync(unchangedBaseline, old, current, CancellationToken.None);
            old.MakeDiffReadOnly();
            current.MakeDiffReadOnly();
            var unchanged = await old.CompareAsync(current, CancellationToken.None);
            Require(unchanged.IsAvailable && !unchanged.IsCoarse && unchanged.ChangeRows.Length == 0,
                "unchanged snapshots produce an available empty comparison");
            Require(unchanged.AddedLines == 0 && unchanged.DeletedLines == 0, "unchanged line totals");
            old.ApplyDiffPresentation(unchanged, true);
            current.ApplyDiffPresentation(unchanged, false);
            Require(old.GetText() == unchangedText && current.GetText() == unchangedText,
                "empty diff presentation preserves both snapshots");
        }

        source.SetText("one\rtwo\rthree\r");
        source.SetTextSelectionPosition(0, 0);
        source.TypeText("inserted\r");
        var originalText = source.GetText();
        var originalRevision = Native(source).DocumentRevision;
        var originalUndo = source.CanUndo;
        using var baseline = await DocumentBaseline.FromTextAsync("one\rtwo\rthree\r");
        await source.LoadDiffSnapshotsAsync(baseline, old, current, CancellationToken.None);
        Require(!Native(source).ReadOnly, "original capture lease drained on UI");
        old.MakeDiffReadOnly();
        current.MakeDiffReadOnly();
        var result = await old.CompareAsync(current, CancellationToken.None);
        Require(result.IsAvailable && !result.IsCoarse && result.ChangeRows.Length == 1, "exact insertion result");
        Require(result.AddedLines == 1 && result.DeletedLines == 0, "inserted source line total");
        old.ApplyDiffPresentation(result, true);
        current.ApplyDiffPresentation(result, false);
        var palette = DiffColorPalette.ForTheme(ElementTheme.Dark, highContrast: false);
        old.ApplyDiffPalette(palette, true);
        current.ApplyDiffPalette(palette, false);
        await Task.Delay(100); // Exercise the native repeating gap brush during paint.
        Require(Native(old).VisibleFromDocLine(0) == Native(current).VisibleFromDocLine(1), "BOF display-only row alignment");
        Require(old.GetText() == "one\rtwo\rthree\r" && current.GetText() == originalText, "source bytes unchanged by alignment");
        old.SelectAll();
        Require(old.GetSelectedText() == "one\rtwo\rthree\r", "selection excludes display gap");
        Native(old).SetSel(Native(old).PositionFromLine(1), Native(old).PositionFromLine(1));
        Native(old).LineUp();
        Require(Native(old).CurrentPos == 0, "up skips BOF padding");
        Require(source.GetText() == originalText && Native(source).DocumentRevision == originalRevision && source.CanUndo == originalUndo,
            "preview preserves original text/revision/undo");
        source.Undo();
        Require(source.GetText() == "one\rtwo\rthree\r", "original undo still works");

        await LoadPairAsync(old, current, "a\rb\rc", "a\rx\rb\rc");
        var oldNative = Native(old);
        oldNative.SetSel(0, 0);
        oldNative.ChooseCaretX();
        oldNative.LineDown();
        Require(oldNative.CurrentPos == oldNative.PositionFromLine(1), "Down crosses an internal gap");
        oldNative.LineUp();
        Require(oldNative.CurrentPos == 0, "Up crosses an internal gap");
        oldNative.LineDownExtend();
        Require(oldNative.SelectionStart == 0 && oldNative.SelectionEnd == oldNative.PositionFromLine(1),
            "Shift+Down selects source text across an internal gap");

        await LoadPairAsync(old, current, "a\rb\rc", "a\r" + string.Concat(System.Linq.Enumerable.Repeat("x\r", 100)) + "b\rc");
        oldNative.SetSel(0, 0);
        oldNative.ChooseCaretX();
        oldNative.PageDown();
        Require(oldNative.CurrentPos >= oldNative.PositionFromLine(1), "PageDown crosses a screen-sized gap");
        oldNative.PageUp();
        Require(oldNative.CurrentPos == 0, "PageUp crosses a screen-sized gap");
        oldNative.PageDownExtend();
        Require(oldNative.SelectionStart == 0 && oldNative.SelectionEnd >= oldNative.PositionFromLine(1),
            "Shift+PageDown selects source text across a gap");
        oldNative.SetSel(oldNative.PositionFromLine(1), oldNative.PositionFromLine(1));
        oldNative.StutteredPageUp();
        Require(oldNative.CurrentPos == 0, "stuttered PageUp resolves a gap in its movement direction");
        oldNative.StutteredPageDown();
        Require(oldNative.CurrentPos >= oldNative.PositionFromLine(1), "stuttered PageDown crosses an internal gap");

        await LoadPairAsync(old, current, "abc", "abc\rx\ry");
        oldNative.FirstVisibleLine = 0;
        var gapY = oldNative.PointYFromPosition(0) + oldNative.TextHeight(0) + 2;
        var gapX = oldNative.PointXFromPosition(0);
        Require(oldNative.PositionFromPoint(gapX, gapY) == oldNative.Length, "EOF gap hit maps to the document end");
        Require(oldNative.PositionFromPointClose(gapX, gapY) == -1, "gap has no text hit");
        oldNative.SetSel(0, oldNative.PositionFromPoint(gapX, gapY));
        Require(old.GetSelectedText() == "abc", "drag endpoint in EOF gap includes the final line");
        oldNative.SetSel(0, 0);
        oldNative.ChooseCaretX();
        oldNative.LineDown();
        Require(oldNative.CurrentPos == 3, "Down into EOF padding reaches document end");
        oldNative.SetSel(0, 0);
        oldNative.LineDownExtend();
        Require(old.GetSelectedText() == "abc", "Shift+Down into EOF padding selects through document end");

        var largePadding = string.Concat(System.Linq.Enumerable.Repeat("x\r", 100));
        await LoadPairAsync(old, current, "abc", "abc\r" + largePadding + "y");
        oldNative.SetSel(0, 0);
        oldNative.ChooseCaretX();
        oldNative.PageDownExtend();
        Require(old.GetSelectedText() == "abc", "Shift+PageDown through large EOF padding selects document end");
        await LoadPairAsync(old, current, "abc", largePadding + "abc");
        oldNative.SetSel(2, 2);
        oldNative.ChooseCaretX();
        oldNative.PageUp();
        Require(oldNative.CurrentPos == 0, "PageUp through large BOF padding reaches document start");
        oldNative.SetSel(2, 2);
        oldNative.ChooseCaretX();
        oldNative.PageUpExtend();
        Require(oldNative.CurrentPos == 0 && old.GetSelectedText() == "ab", "Shift+PageUp through BOF padding selects document start");

        // UpdateUI arrives from paint/idle after the synchronous guard has reset.
        // Both native scroll setters must settle after equal values are mirrored.
        var viewportEvents = 0;
        EventHandler oldViewport = (_, _) =>
        {
            viewportEvents++;
            current.SetDiffViewport(old.FirstVisibleRow, old.HorizontalScrollOffset);
        };
        EventHandler newViewport = (_, _) =>
        {
            viewportEvents++;
            old.SetDiffViewport(current.FirstVisibleRow, current.HorizontalScrollOffset);
        };
        old.ViewportChanged += oldViewport;
        current.ViewportChanged += newViewport;
        try
        {
            old.SetDiffViewport(0, 20);
            await Task.Delay(200);
            var settledCount = viewportEvents;
            await Task.Delay(200);
            Require(viewportEvents == settledCount && settledCount is > 0 and < 20, "delayed paired scroll notifications converge");
        }
        finally
        {
            old.ViewportChanged -= oldViewport;
            current.ViewportChanged -= newViewport;
        }

        // Existing preview documents can load another snapshot before read-only
        // comparison. Their previous alignment is retired with the native document.
        await old.LoadTextAsync("a");
        await current.LoadTextAsync("a\r");
        old.MakeDiffReadOnly(); current.MakeDiffReadOnly();
        result = await old.CompareAsync(current, CancellationToken.None);
        Require(result.ChangeRows.Length != 0, "final terminator is a change");
        Require(result.AddedLines == 1 && result.DeletedLines == 1, "terminal logical empty row excluded from totals");
        old.ApplyDiffPresentation(result, true); current.ApplyDiffPresentation(result, false);
        Require(old.GetText() == "a" && current.GetText() == "a\r", "EOF alignment preserves terminators");

        await old.LoadTextAsync(string.Empty);
        await current.LoadTextAsync("\0你好😀\r");
        old.MakeDiffReadOnly(); current.MakeDiffReadOnly();
        result = await old.CompareAsync(current, CancellationToken.None);
        old.ApplyDiffPresentation(result, true); current.ApplyDiffPresentation(result, false);
        current.SelectAll();
        Require(current.GetText() == "\0你好😀\r", "snapshot preserves Unicode/NUL bytes");
        // Scintilla's established clipboard selection contract sanitizes NUL to
        // a space; display-only alignment must add no other bytes.
        Require(current.GetSelectedText() == " 你好😀\r", "clipboard-safe selection excludes display gaps");

        using (var cancellation = new CancellationTokenSource())
        {
            var comparison = old.CompareAsync(current, cancellation.Token);
            cancellation.Cancel();
            try { await comparison; } catch (OperationCanceledException) { }
            // Mutation is possible only after completion really released both
            // native leases; the old snapshot was intentionally read-only.
            Native(old).ReadOnly = false;
            Native(old).AddText(1, "x");
            Require(old.GetText() == "x", "canceled compare drained leases");
        }
        await CheckOrdinaryPageNavigationAsync();
        log.AppendLine("PASS Scintilla DiffView: native snapshot, aligned read-only panes, Unicode/NUL, terminators, undo isolation and cancellation drain.");
    }

    private static async Task CheckOrdinaryPageNavigationAsync()
    {
        using var editor = new TextEditorCore();
        Window.Current.Content = editor;
        await Task.Delay(100);
        editor.TextWrapping = TextWrapping.Wrap;
        await editor.LoadTextAsync(new string('a', 20000));
        await Task.Delay(100);
        var native = Native(editor);
        Require(native.WrapCount(0) > native.LinesOnScreen * 2, "ordinary wrapped page fixture spans screens");
        native.SetSel(0, 0);
        native.ChooseCaretX();
        native.PageDown();
        var pageDownPosition = native.CurrentPos;
        Require(native.CurrentPos > 0 && native.LineFromPosition(native.CurrentPos) == 0, "ordinary PageDown traverses display rows of a wrapped source line");
        native.PageUp();
        Require(native.PointYFromPosition(native.CurrentPos) == native.PointYFromPosition(0),
            $"ordinary PageUp returns to the first wrapped display row (Down={pageDownPosition}, Up={native.CurrentPos}, Top={native.FirstVisibleLine})");
        native.StutteredPageDown();
        var downPosition = native.CurrentPos;
        Require(downPosition > 0, "ordinary stuttered PageDown moves within a wrapped line");
        native.StutteredPageUp();
        Require(native.CurrentPos < downPosition, "ordinary stuttered PageUp uses display rows");
    }

    private static async Task LoadPairAsync(TextEditorCore old, TextEditorCore current, string oldText, string newText)
    {
        await old.LoadTextAsync(oldText);
        await current.LoadTextAsync(newText);
        old.MakeDiffReadOnly();
        current.MakeDiffReadOnly();
        var result = await old.CompareAsync(current, CancellationToken.None);
        Require(result.IsAvailable && !result.IsCoarse, "interaction fixture has an exact comparison");
        old.ApplyDiffPresentation(result, true);
        current.ApplyDiffPresentation(result, false);
    }
}
