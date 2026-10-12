// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Media;

namespace NotepadsEditorTests;

internal static class ScrollRestorationTests
{
    private const int LineCount = 2000;

    internal static async Task RunAsync(StringBuilder log)
    {
        foreach (var wrapping in new[] { TextWrapping.NoWrap, TextWrapping.Wrap })
            await CheckFirstPaintAsync(log, wrapping);
        await CheckFirstPaintAsync(log, TextWrapping.NoWrap, atEnd: true);
        await CheckFirstPaintAsync(log, TextWrapping.NoWrap, shortTarget: true);
        await CheckFirstPaintAsync(log, TextWrapping.Wrap, atEnd: true, singleLine: true);
        await CheckWrapTransitionAsync(log);
        await CheckLongPrecedingLinesAsync(log);
        await CheckLongPrecedingLinesAsync(log, atEnd: true);
        foreach (var wrapping in new[] { TextWrapping.Wrap, TextWrapping.NoWrap })
            await CheckV1CaretAsync(log, wrapping);
        foreach (var wrapping in new[] { TextWrapping.NoWrap, TextWrapping.Wrap })
        {
            await CheckLogAppendAsync(log, wrapping, pending: true);
            await CheckLogAppendAsync(log, wrapping, pending: false);
        }
        await CheckBlockedLogAppendAsync(log, readOnly: true);
        await CheckBlockedLogAppendAsync(log, readOnly: false);
    }

    private static WinUIEditor.EditorBaseControl View(TextEditorCore core) =>
        (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];

    private static TextEditorCore Create(TextWrapping wrapping) => new()
    {
        Width = 600, Height = 320, TextWrapping = wrapping,
        HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top,
        Padding = new Thickness(0, 4, 0, 0)
    };

    private static (long Line, long SubLine, long X) Position(WinUIEditor.Editor native)
    {
        var row = native.FirstVisibleLine;
        var line = native.DocLineFromVisible(row);
        return (line, row - native.VisibleFromDocLine(line), native.XOffset);
    }

    private static Task WaitForWrapAsync(WinUIEditor.Editor native) => UntilFrameAsync(
        () => native.VisibleFromDocLine(native.LineCount) == native.LineCount * native.WrapCount(0));

    private static async Task CheckFirstPaintAsync(StringBuilder log, TextWrapping wrapping,
        bool atEnd = false, bool shortTarget = false, bool singleLine = false)
    {
        var scenario = $"{wrapping}, {(atEnd ? "EOF" : shortTarget ? "short visible lines" : "middle")} document anchor";
        var text = singleLine ? new string('x', 4000) : string.Join('\r', Enumerable.Range(0, LineCount)
            .Select(line => new string('x', shortTarget && line > 0 ? 8 : 120)));
        double horizontal, vertical;
        int? documentLine;
        int subLine;
        (long Line, long SubLine, long X) expected;
        using (var original = Create(wrapping))
        {
            await original.LoadTextAsync(text);
            original.SetFontZoomFactor(150);
            var native = View(original).Editor;
            await EditorTestLayout.AfterPaintAsync(native, () =>
            {
                Window.Current.Content = original;
                Window.Current.Activate();
            });
            await WaitForWrapAsync(native);
            if (wrapping == TextWrapping.NoWrap)
            {
                var bar = FindNamed<ScrollBar>(View(original), "HorizontalScrollBar");
                var surface = FindNamed<Border>(View(original), "ImageTarget");
                await UntilFrameAsync(() => bar.Visibility == Visibility.Visible && bar.ActualHeight > 0 &&
                    surface.ActualHeight + bar.ActualHeight <= View(original).ActualHeight + 0.5);
            }
            await EditorTestLayout.AfterPaintAsync(native, () =>
            {
                native.FirstVisibleLine = atEnd ? native.VisibleFromDocLine(native.LineCount) :
                    native.VisibleFromDocLine(800) + (wrapping == TextWrapping.Wrap ? 1 : 0);
                if (wrapping == TextWrapping.NoWrap) native.XOffset = 120;
            });
            original.GetScrollViewerPosition(out horizontal, out vertical);
            original.GetViewportAnchor(out documentLine, out subLine);
            expected = Position(native);
            Check(singleLine ? expected.Line == 0 && expected.SubLine > 0 :
                (atEnd ? expected.Line > LineCount - 30 : expected.Line == 800) &&
                expected.SubLine == (wrapping == TextWrapping.Wrap ? 1 : 0),
                scenario + ": source viewport established");
            Window.Current.Content = null;
        }

        using var restored = Create(wrapping);
        await restored.LoadTextAsync(text);
        restored.SetFontZoomFactor(150); // Same order as production ResetEditorState.
        restored.SetTextSelectionPosition(100, 100); // Deliberately outside the saved viewport.
        restored.SetScrollViewerInitPosition(horizontal, vertical, documentLine, subLine);
        restored.GetScrollViewerPosition(out var pendingX, out var pendingY);
        restored.GetViewportAnchor(out var pendingLine, out var pendingSubLine);
        Check(pendingX == horizontal && pendingY == vertical && pendingLine == documentLine &&
            pendingSubLine == subLine, scenario + ": never-presented metadata preserved");
        var editor = View(restored).Editor;
        var observed = new List<(long Line, long SubLine, long X)>();
        void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args) => observed.Add(Position(sender));
        void Loaded(object sender, RoutedEventArgs args) => restored.Focus(FocusState.Programmatic);
        editor.Painted += Painted;
        restored.Loaded += Loaded;
        try
        {
            await EditorTestLayout.AfterPaintAsync(editor, () => Window.Current.Content = restored);
            await WaitForWrapAsync(editor);
            await UntilFrameAsync(() => !View(restored).IsViewportRestorePending);
            if (shortTarget)
            {
                // Cross the native 50 ms widen tick, then force another paint.
                // The assertion concerns a delayed timer, not readiness by sleep.
                await Task.Delay(100);
                await EditorTestLayout.AfterPaintAsync(editor, () => restored.Width += 1);
            }
            Check(observed.Count > 0 && observed.All(position => position == expected),
                $"{scenario}: every paint must show {expected}; observed {string.Join(", ", observed.Take(12))}");
            Check(editor.CurrentPos == 100 && editor.Anchor == 100, scenario + ": selection preserved");

            // Once restored, later tab switches must use the user's new viewport.
            editor.Painted -= Painted;
            await EditorTestLayout.AfterPaintAsync(editor, () => editor.FirstVisibleLine -= 5);
            var moved = Position(editor);
            Window.Current.Content = new Grid();
            await UntilFrameAsync(() => !restored.IsLoaded);
            restored.GetViewportAnchor(out var unloadedLine, out var unloadedSubLine);
            Check(unloadedLine == moved.Line && unloadedSubLine == moved.SubLine, scenario + ": unloaded metadata is current");
            observed.Clear();
            editor.Painted += Painted;
            await EditorTestLayout.AfterPaintAsync(editor, () => Window.Current.Content = restored);
            await UntilFrameAsync(() => restored.IsLoaded);
            Check(observed.Count > 0 && observed.All(position => position == moved), scenario + ": tab reattach does not replay restore");
        }
        finally
        {
            editor.Painted -= Painted;
            restored.Loaded -= Loaded;
            Window.Current.Content = null;
        }
        log.AppendLine($"PASS: {scenario} restores before first paint and survives focus, idle wrapping and tab reattachment.");
    }

    private static async Task CheckWrapTransitionAsync(StringBuilder log)
    {
        using var core = Create(TextWrapping.Wrap);
        await core.LoadTextAsync(string.Join('\r', Enumerable.Repeat(new string('x', 120), LineCount)));
        var native = View(core).Editor;
        try
        {
            await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = core);
            await WaitForWrapAsync(native);
            var positions = new List<(long Line, long SubLine, long X)>();
            void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args) => positions.Add(Position(sender));
            native.Painted += Painted;
            try
            {
                await EditorTestLayout.AfterPaintAsync(native, () =>
                {
                    core.TextWrapping = TextWrapping.NoWrap;
                    core.SetScrollViewerInitPosition(0, 0, 800);
                });
                await UntilFrameAsync(() => !View(core).IsViewportRestorePending);
                Check(positions.Count > 0 && positions.All(position => position == (800, 0, 0)),
                    "disabling wrapping restores against unwrapped document lines");
            }
            finally { native.Painted -= Painted; }
        }
        finally { Window.Current.Content = null; }
        log.AppendLine("PASS: restoring while disabling Word Wrap discards obsolete wrapped row heights.");
    }

    private static async Task UntilFrameAsync(Func<bool> ready)
    {
        var completion = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        void Rendering(object sender, object args)
        {
            if (ready()) completion.TrySetResult(true);
        }
        CompositionTarget.Rendering += Rendering;
        try { await completion.Task.WaitAsync(TimeSpan.FromSeconds(10)); }
        finally { CompositionTarget.Rendering -= Rendering; }
    }

    private static async Task CheckLongPrecedingLinesAsync(StringBuilder log, bool atEnd = false)
    {
        // The five lines before a distant anchor exceed the visible-wrap budget.
        // Both the anchor's subline and following rows must be ready at first paint.
        const int anchor = 105;
        using var core = Create(TextWrapping.Wrap);
        core.SetFontZoomFactor(150);
        var preceding = string.Join('\r', Enumerable.Repeat(new string('x', 20), 100)
            .Concat(Enumerable.Repeat(new string('x', 430_000), 5)));
        var tail = atEnd ? new string('x', 120) + '\r' + new string('x', 2000) :
            string.Join('\r', Enumerable.Repeat(new string('x', 120), 100));
        await core.LoadTextAsync(preceding + '\r' + tail);
        core.SetScrollViewerInitPosition(0, 0, anchor, 1);
        var native = View(core).Editor;
        var observed = new List<(long Line, long SubLine, long X)>();
        var followingRowsReady = new List<bool>();
        void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args)
        {
            observed.Add(Position(sender));
            followingRowsReady.Add(sender.VisibleFromDocLine(anchor + 2) - sender.VisibleFromDocLine(anchor + 1) ==
                sender.WrapCount(anchor + 1));
        }
        native.Painted += Painted;
        try
        {
            await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = core);
            // Equal-length source lines give an independent completion condition
            // for the prefix's idle wrapping, without a fixed delay.
            var prefixRows = 100 * native.WrapCount(0) + 5 * native.WrapCount(100);
            await UntilFrameAsync(() => native.VisibleFromDocLine(anchor) == prefixRows);
            Check(observed.Count > 0 && observed.All(position => position == (anchor, 1, 0)) && followingRowsReady.All(ready => ready),
                "long preceding lines must not discard the saved subline or paint incomplete following rows");
        }
        finally
        {
            native.Painted -= Painted;
            Window.Current.Content = null;
        }
        log.AppendLine($"PASS: a wrapped anchor survives long preceding lines{(atEnd ? " and an unresolved EOF tail" : "")}.");
    }

    private static async Task CheckV1CaretAsync(StringBuilder log, TextWrapping wrapping)
    {
        using var core = Create(wrapping);
        await core.LoadTextAsync(new string('x', 12000));
        core.SetFontZoomFactor(150);
        core.SetTextSelectionPosition(8000, 8000);
        core.SetScrollViewerInitPosition(0, 0); // V1 has no native viewport anchor.
        var native = View(core).Editor;
        var observed = new List<bool>();
        void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args)
        {
            var y = sender.PointYFromPosition(sender.CurrentPos);
            var x = sender.PointXFromPosition(sender.CurrentPos);
            var surface = FindNamed<Border>(View(core), "ImageTarget");
            var scale = Windows.Graphics.Display.DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
            observed.Add(y >= 0 && y < sender.LinesOnScreen * sender.TextHeight(0) &&
                x >= 0 && x < surface.ActualWidth * scale);
        }
        native.Painted += Painted;
        try
        {
            await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = core);
            await UntilFrameAsync(() => !View(core).IsViewportRestorePending);
            Check(native.CurrentPos == 8000 && native.Anchor == 8000 && observed.Count > 0 && observed.All(visible => visible),
                $"V1 import shows a saved caret inside a long {wrapping} paragraph from the first paint");
        }
        finally
        {
            native.Painted -= Painted;
            Window.Current.Content = null;
        }
        log.AppendLine($"PASS: V1 import preserves and shows the saved caret within a long {wrapping} paragraph.");
    }

    private static string LongLogText() => ".LOG\r" +
        string.Join('\r', Enumerable.Repeat(new string('x', 120), 160));

    private static bool CaretVisible(TextEditorCore core)
    {
        var native = View(core).Editor;
        var y = native.PointYFromPosition(native.CurrentPos);
        var x = native.PointXFromPosition(native.CurrentPos);
        var surface = FindNamed<Border>(View(core), "ImageTarget");
        var scale = Windows.Graphics.Display.DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        return y >= 0 && y < native.LinesOnScreen * native.TextHeight(0) &&
            x >= 0 && x < surface.ActualWidth * scale;
    }

    private static async Task CheckLogAppendAsync(StringBuilder log, TextWrapping wrapping, bool pending)
    {
        var scenario = $".LOG {wrapping}, {(pending ? "pending" : "completed")} restore";
        using var core = Create(wrapping);
        var before = LongLogText();
        await core.LoadTextAsync(before);
        core.SetFontZoomFactor(150);
        core.SetScrollViewerInitPosition(60, 800, 40);
        var view = View(core);
        var native = view.Editor;
        var observed = new List<bool>();
        var positions = new List<(long Row, long X, long Y)>();
        void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args)
        {
            observed.Add(CaretVisible(core));
            positions.Add((sender.FirstVisibleLine, sender.PointXFromPosition(sender.CurrentPos),
                sender.PointYFromPosition(sender.CurrentPos)));
        }
        try
        {
            if (!pending)
            {
                await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = core);
                await UntilFrameAsync(() => !view.IsViewportRestorePending);
            }
            int? pendingLine = null;
            var pendingSubLine = 0;
            double pendingX = 0, pendingY = 0;
            var after = string.Empty;
            native.Painted += Painted;
            await EditorTestLayout.AfterPaintAsync(native, () =>
            {
                Check(view.IsViewportRestorePending == pending, scenario + ": initial restore state");
                // No await between this assertion and the append: force the ordering
                // that can occur when Loaded runs during scrollbar layout deferral.
                core.TryInsertNewLogEntry();
                core.GetViewportAnchor(out pendingLine, out pendingSubLine);
                core.GetScrollViewerPosition(out pendingX, out pendingY);
                after = core.GetText();
                Check(after.StartsWith(before + "\r", StringComparison.Ordinal) && after.EndsWith('\r') &&
                    after.Length > before.Length + 2, scenario + ": timestamp appended without changing existing text");
                Check(native.CurrentPos == native.Length && native.Anchor == native.Length, scenario + ": caret at EOF");
                Check(view.IsViewportRestorePending, scenario + ": append waits for final layout before following EOF");
                if (pending) Window.Current.Content = core;
            });
            await UntilFrameAsync(() => !view.IsViewportRestorePending);
            // .LOG, timestamp and final empty line are short; the 160 identical
            // history lines give an independent idle-wrap completion condition.
            await UntilFrameAsync(() => native.VisibleFromDocLine(native.LineCount) == 3 + 160 * native.WrapCount(1));
            Check(observed.Count > 0 && observed.All(visible => visible),
                scenario + $": every paint must show the appended EOF caret; row/X/Y: {string.Join(", ", positions.Take(12))}");
            Check(pendingLine == null && pendingSubLine == 0 && pendingX == 0 && pendingY == 0,
                scenario + ": metadata must not retain the superseded anchor before first paint");
            core.TryInsertNewLogEntry();
            Check(core.GetText() == after && !view.IsViewportRestorePending,
                scenario + ": only one automatic append and caret-follow request");
        }
        finally
        {
            native.Painted -= Painted;
            Window.Current.Content = null;
        }
        log.AppendLine($"PASS: {scenario} shows the new EOF caret from the first paint.");
    }

    private static async Task CheckBlockedLogAppendAsync(StringBuilder log, bool readOnly)
    {
        using var core = Create(TextWrapping.NoWrap);
        var before = LongLogText();
        await core.LoadTextAsync(before);
        core.SetScrollViewerInitPosition(0, 800, 40);
        var view = View(core);
        if (readOnly) view.Editor.ReadOnly = true;
        else core.IsEnabled = false;
        Check(view.IsViewportRestorePending, "Blocked .LOG starts with a pending restore");
        core.TryInsertNewLogEntry();
        Check(core.GetText() == before, "Blocked .LOG must not append text");
        try
        {
            await EditorTestLayout.AfterPaintAsync(view.Editor, () => Window.Current.Content = core);
            await UntilFrameAsync(() => !view.IsViewportRestorePending);
            Check(Position(view.Editor).Line == 40, "Skipped .LOG append must preserve the saved viewport");
        }
        finally { Window.Current.Content = null; }
        log.AppendLine($"PASS: {(readOnly ? "read-only" : "disabled")} .LOG preserves its pending viewport.");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static T FindNamed<T>(DependencyObject root, string name) where T : FrameworkElement
    {
        if (root is T element && element.Name == name) return element;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
            if (FindNamed<T>(VisualTreeHelper.GetChild(root, i), name) is T child) return child;
        return null;
    }
}
