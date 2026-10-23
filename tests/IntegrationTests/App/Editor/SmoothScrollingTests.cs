// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.Storage;
using Notepads.Presentation.Controls.DiffViewer;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Graphics.Display;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Automation.Text;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Media;
using WinUIEditor;
using static NotepadsEditorTests.EditorTestLayout;

namespace NotepadsEditorTests;

internal static class SmoothScrollingTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var previous = Window.Current.Content;
        log.AppendLine($"SCROLL animationEnabled={new UISettings().AnimationsEnabled}");
        try
        {
            foreach (var rtl in new[] { false, true })
                foreach (var wrap in new[] { false, true })
                    await CheckViewportAsync(log, rtl, wrap);
            foreach (var rtl in new[] { false, true })
            {
                await CheckPendingVisibleRangesAsync(log, rtl);
                await CheckHorizontalAsync(log, rtl);
                foreach (var animate in new[] { false, true }) await CheckCanvasGrowthAsync(log, rtl, animate);
                await CheckDiffAsync(log, rtl);
            }
        }
        finally { Window.Current.Content = previous; }
    }

    private static async Task CheckViewportAsync(StringBuilder log, bool rtl, bool wrap)
    {
        using var core = new TextEditorCore
        {
            Width = 620, Height = 351, FontSize = 17,
            RightToLeftReadingOrder = rtl,
            TextWrapping = wrap ? TextWrapping.Wrap : TextWrapping.NoWrap,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        var view = (EditorBaseControl)((Grid)core.Content).Children[0];
        var native = view.Editor;
        var suffix = wrap ? string.Concat(Enumerable.Repeat("中文 אבג مرحبا abc ", 9)) : "中文 אבג مرحبا abc";
        core.SetText(string.Join('\r', Enumerable.Range(0, 300).Select(i => $"row{i:D3} " + suffix)));
        await AfterPaintAsync(native, () => Window.Current.Content = core);
        await UntilAsync(() => !view.IsViewportRestorePending);
        if (wrap)
        {
            await UntilAsync(() => native.WrapCount(0) > 1 &&
                native.VisibleFromDocLine(native.LineCount) == native.LineCount * native.WrapCount(0));
        }
        var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        var height = native.TextHeight(0);
        var offset = height / scale * 0.375;
        var pixels = Math.Round(offset * scale);
        var row = native.VisibleFromDocLine(100);
        var position = native.PositionFromLine(101);
        var provider = (ITextProvider)FrameworkElementAutomationPeer.CreatePeerForElement(view).GetPattern(PatternInterface.Text);
        var range = provider.DocumentRange.Clone();
        range.MoveEndpointByRange(TextPatternRangeEndpoint.End, range, TextPatternRangeEndpoint.Start);
        var characters = (int)native.CountCharacters(0, position);
        Check(range.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, TextUnit.Character, characters) == characters,
            "Retained UIA range reaches the independently measured document line");
        range.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 1);
        await AfterPaintAsync(native, () => view.SetVerticalViewport(row, 0));
        range.GetBoundingRectangles(out var before);
        Check(before.Length == 4, "Retained UIA sample is a visible glyph");
        await AfterPaintAsync(native, () => view.SetVerticalViewport(row, offset));
        Check(native.FirstVisibleLine == row && view.DisplayVerticalOffset == offset,
            $"A pixel-only scroll retains its display row and unrounded DIP offset: expected=({row},{offset}), actual=({native.FirstVisibleLine},{view.DisplayVerticalOffset})");
        Check(native.PointYFromPosition(native.PositionFromLine(100)) == -pixels,
            "Document geometry projects the partial first row exactly once");
        range.GetBoundingRectangles(out var after);
        Check(after.Length == 4 && Math.Abs(before[1] - after[1] - pixels) < 0.1,
            "The already-created UIA range follows the same pixel-only movement");
        var glyphStart = native.PointXFromPosition(position);
        var x = (int)Math.Round(glyphStart + (native.PointXFromPosition(native.PositionAfter(position)) - glyphStart) * 0.25);
        var y = native.PointYFromPosition(position) + height / 2;
        Check(native.PositionFromPointClose(x, y) == position,
            "Hit testing resolves the same visible glyph after fractional scrolling");
        var bar = Find<ScrollBar>(view, "VerticalScrollBar");
        Check(bar != null && Math.Abs(bar.Value - row - offset * scale / height) < 1e-8,
            "Scrollbar and text share the fractional row position");
        core.GetScrollViewerPosition(out var horizontal, out var vertical);
        core.GetViewportAnchor(out var documentLine, out var subLine);
        Check(Math.Abs(vertical - (row * height / scale + offset)) < 1e-8,
            "Session capture includes the local offset without dropping subpixel input");
        await AfterPaintAsync(native, () => core.SetScrollViewerInitPosition(horizontal, vertical,
            documentLine, subLine, rowOffset: offset));
        await UntilAsync(() => !view.IsViewportRestorePending);
        Check(native.FirstVisibleLine == row && view.DisplayVerticalOffset == offset,
            "Anchored restoration retains the same partial row");
        await CheckVisibleRangesAsync(core, view, provider, scale);
        native.SetEmptySelection(native.PositionFromLine(100));
        await AfterPaintAsync(native, () => native.ScrollCaret());
        Check(native.PointYFromPosition(native.CurrentPos) >= 0,
            "Caret reveal exposes a caret clipped above the partial first row");
        await CheckCaretBelowPartialRowAsync(native, view, scale);
        await CheckAnimationAsync(core, view, scale);
        await CheckCentreCaretAsync(native, view, scale);
        if (wrap)
        {
            var first = native.FirstVisibleLine;
            var previousOffset = view.DisplayVerticalOffset;
            view.ScrollBy(100, 0, true);
            Check(!view.IsScrollAnimating && native.XOffset == 0 && native.FirstVisibleLine == first &&
                view.DisplayVerticalOffset == previousOffset, "Wrap suppresses horizontal-only motion without touching Y");
        }
        await AfterPaintAsync(native, () => view.SetVerticalViewport(long.MaxValue, 0));
        var target = Find<FrameworkElement>(view, "ImageTarget");
        var bottom = native.PointYFromPosition(native.Length) + height;
        Check(target != null && Math.Abs(bottom - target.ActualHeight * scale) <= 1,
            $"EOF clamps to actual viewport pixels rather than an integer page height: bottom={bottom}, viewport={target?.ActualHeight * scale}");
        native.GotoPos(native.Length);
        native.PageDown();
        Check(Math.Abs(native.PointYFromPosition(native.Length) + height - target.ActualHeight * scale) <= 1,
            "PageDown at EOF keeps the precise viewport boundary before any repaint can repair it");
        native.EnsureVisibleEnforcePolicy(native.LineCount - 1);
        Check(Math.Abs(native.PointYFromPosition(native.Length) + height - target.ActualHeight * scale) <= 1,
            "Ensuring the last row is visible respects the same precise boundary");
        native.VerticalCentreCaret();
        Check(Math.Abs(native.PointYFromPosition(native.Length) + height - target.ActualHeight * scale) <= 1,
            "Centre at EOF clamps to the precise last viewport before repaint");
        await AfterPaintAsync(native, () => core.SetText(""));
        Check(native.FirstVisibleLine == 0 && view.DisplayVerticalOffset == 0 && core.GetText().Length == 0,
            "Document replacement clears the previous partial viewport");
        log.AppendLine($"PASS: pixel viewport, UIA, hit testing, scrollbar, restore, reveal and EOF: rtl={rtl}, wrap={wrap}.");
    }

    private static async Task CheckVisibleRangesAsync(TextEditorCore core, EditorBaseControl view,
        ITextProvider provider, double scale)
    {
        var native = view.Editor;
        var source = core.GetText().Split('\r');
        var target = Find<FrameworkElement>(view, "ImageTarget");
        Check(target?.ActualHeight > 0, "The visible-range fixture has a measured viewport");
        var viewport = target.ActualHeight * scale;
        var height = native.TextHeight(0);
        var row = native.VisibleFromDocLine(100);
        var offset = view.DisplayVerticalOffset;
        foreach (var fraction in new[] { 0.0, 0.375, 0.95, 1 - 0.25 / height })
        {
            await AfterPaintAsync(native, () => view.SetVerticalViewport(row, height * fraction / scale));
            // Independent line boxes from measured document geometry, including
            // wrapped continuations. A line is visible iff its box intersects.
            var candidates = Enumerable.Range(99, Math.Min(source.Length - 99, (int)native.LinesOnScreen + 4));
            var expected = candidates.Where(line =>
            {
                var top = native.PointYFromPosition(native.PositionFromLine(line));
                return top < viewport && top + native.WrapCount(line) * height > 0;
            }).Select(line => source[line]).ToArray();
            var actual = provider.GetVisibleRanges().Select(range => range.GetText(-1).TrimEnd('\r', '\n')).ToArray();
            Check(expected.Length > 0 && actual.SequenceEqual(expected),
                $"UIA enumerates exactly the document lines intersecting both viewport edges: fraction={fraction}, " +
                $"expected={string.Join(',', expected.Select(line => line[..6]))}, " +
                $"actual={string.Join(',', actual.Select(line => line.Length >= 6 ? line[..6] : line))}");
        }
        await AfterPaintAsync(native, () => view.SetVerticalViewport(row, offset));
    }

    private static async Task CheckCentreCaretAsync(Editor native, EditorBaseControl view, double scale)
    {
        var height = native.TextHeight(0);
        var caret = native.PositionFromLine(100);
        var halfPage = native.LinesOnScreen / 2;
        var row = native.VisibleFromDocLine(100) - halfPage;
        native.SetEmptySelection(caret);
        await AfterPaintAsync(native, () => view.SetVerticalViewport(row, height / (2 * scale)));
        await AfterPaintAsync(native, () =>
        {
            native.VerticalCentreCaret();
            Check(native.FirstVisibleLine == row && view.DisplayVerticalOffset == 0,
                "Centre clears a partial offset even when its target row is unchanged");
            Check(native.PointYFromPosition(caret) == halfPage * height,
                "Centre positions the caret at the measured half-page row");
        });
        if (new UISettings().AnimationsEnabled)
        {
            var x = native.XOffset;
            view.ScrollBy(35, 3.5 * height / scale, true);
            Check(view.IsScrollAnimating, "The centre fixture owns unfinished wheel motion");
            native.VerticalCentreCaret();
            Check(!view.IsScrollAnimating && native.FirstVisibleLine == row && view.DisplayVerticalOffset == 0 && native.XOffset == x,
                "An unchanged centre request synchronously retires both axis targets");
            await Window.Current.Dispatcher.RunAsync(Windows.UI.Core.CoreDispatcherPriority.Low, () => { });
            Check(native.FirstVisibleLine == row && view.DisplayVerticalOffset == 0 && native.XOffset == x,
                "A retired animation cannot move the centred viewport afterwards");
        }
    }

    private static async Task CheckPendingVisibleRangesAsync(StringBuilder log, bool rtl)
    {
        using var core = new TextEditorCore
        {
            Width = 620, Height = 351, FontSize = 17, RightToLeftReadingOrder = rtl,
            TextWrapping = TextWrapping.NoWrap,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        var view = (EditorBaseControl)((Grid)core.Content).Children[0];
        var native = view.Editor;
        // Keep viewport height fixed when wrap hides horizontal overflow, so
        // the first query can be checked against the subsequent real paint.
        native.HScrollBar = false;
        var suffix = string.Concat(Enumerable.Repeat("alpha 中文 אבג مرحبا 123 ", 12));
        core.SetText(string.Join('\r', Enumerable.Range(0, 250).Select(line => $"row{line:D3} " + suffix)));
        await AfterPaintAsync(native, () => Window.Current.Content = core);
        await UntilAsync(() => !view.IsViewportRestorePending);
        native.SetEmptySelection(native.PositionFromLine(100));
        var provider = (ITextProvider)FrameworkElementAutomationPeer.CreatePeerForElement(view).GetPattern(PatternInterface.Text);
        var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        foreach (var change in new[] { "wrap", "edit", "resize", "font" })
        {
            var row = native.VisibleFromDocLine(100);
            var offset = native.TextHeight(0) * 0.375 / scale;
            if (native.FirstVisibleLine != row || view.DisplayVerticalOffset != offset)
                await AfterPaintAsync(native, () => view.SetVerticalViewport(row, offset));
            double queryMs = 0;
            string[] immediate = null;
            await AfterPaintAsync(native, () =>
            {
                switch (change)
                {
                    case "wrap": core.TextWrapping = TextWrapping.Wrap; break;
                    case "edit": native.InsertText(native.PositionFromLine(100) + 7, suffix); break;
                    case "resize": core.Width = 420; core.UpdateLayout(); break;
                    case "font": core.FontSize = 21; break;
                }
                // No paint/yield or geometry query between the mutation and
                // GetVisibleRanges. Lazy reflow may renumber display rows, but
                // must preserve the document/subline anchor and local offset.
                core.GetViewportAnchor(out var beforeLine, out var beforeSubLine);
                var beforeOffset = view.DisplayVerticalOffset;
                var caret = native.CurrentPos;
                var anchor = native.Anchor;
                var start = Stopwatch.GetTimestamp();
                immediate = ReadRanges();
                queryMs = Stopwatch.GetElapsedTime(start).TotalMilliseconds;
                core.GetViewportAnchor(out var afterLine, out var afterSubLine);
                Check(beforeLine == afterLine && beforeSubLine == afterSubLine && beforeOffset == view.DisplayVerticalOffset,
                    $"UIA resolves pending {change} while preserving its logical viewport: rtl={rtl}");
                CheckRanges(immediate);
                var first = native.FirstVisibleLine;
                var rows = native.VisibleFromDocLine(native.LineCount);
                for (var poll = 0; poll < 8; ++poll)
                {
                    Check(ReadRanges().SequenceEqual(immediate) && native.FirstVisibleLine == first &&
                        view.DisplayVerticalOffset == beforeOffset && native.VisibleFromDocLine(native.LineCount) == rows &&
                        native.CurrentPos == caret && native.Anchor == anchor,
                        "Repeated visible-range polls keep the prepared layout, viewport and selection stable");
                }
            });
            CheckRanges(immediate);
            Check(ReadRanges().SequenceEqual(immediate),
                "The first synchronous visible-range query agrees with the subsequent real paint");
            log.AppendLine(FormattableString.Invariant($"PASS: pending-layout UIA ranges: rtl={rtl}, change={change}, queryMs={queryMs:F3}."));
        }
        if (new UISettings().AnimationsEnabled)
        {
            view.ScrollBy(0, native.TextHeight(0) / scale, true);
            var row = native.FirstVisibleLine;
            var offset = view.DisplayVerticalOffset;
            ReadRanges();
            Check(view.IsScrollAnimating && native.FirstVisibleLine == row && view.DisplayVerticalOffset == offset,
                "A stable UIA query neither advances nor cancels wheel animation");
            view.CancelScrollAnimation();
        }

        string[] ReadRanges() => provider.GetVisibleRanges().Select(range => range.GetText(-1).TrimEnd('\r', '\n')).ToArray();

        void CheckRanges(string[] actual)
        {
            var lines = core.GetText().Split('\r');
            core.GetViewportAnchor(out var first, out _);
            var target = Find<FrameworkElement>(view, "ImageTarget");
            var start = Math.Max(0, first.Value - 1);
            var count = Math.Min(lines.Length - start, (int)native.LinesOnScreen + 4);
            var expected = Enumerable.Range(start, count).Where(line =>
            {
                var y = native.PointYFromPosition(native.PositionFromLine(line));
                return y < target.ActualHeight * scale && y + native.WrapCount(line) * native.TextHeight(0) > 0;
            }).Select(line => lines[line]).ToArray();
            Check(expected.Length > 0 && actual.SequenceEqual(expected),
                $"Pending-layout UIA ranges match current measured line boxes: rtl={rtl}");
        }
    }

    // MoveCaretInsideView moves a caret below the viewport to the last full row.
    // DisplayRowFromY floors, so the normal reveal policy should preserve the
    // existing partial-row viewport.
    private static async Task CheckCaretBelowPartialRowAsync(Editor native, EditorBaseControl view, double scale)
    {
        const int anchor = 100;
        var target = Find<FrameworkElement>(view, "ImageTarget");
        Check(target?.ActualHeight > 0, "The partial-row fixture has a laid-out viewport");
        var viewport = target.ActualHeight * scale;
        var textHeight = native.TextHeight(0);
        var offset = textHeight / scale * 0.375;
        var rows = native.LinesOnScreen;
        Check(rows >= 4, "The partial-row fixture has at least four whole rows");
        var row = native.VisibleFromDocLine(anchor);
        await AfterPaintAsync(native, () => view.SetVerticalViewport(row, offset));
        await AfterPaintAsync(native, () => native.SetEmptySelection(
            native.PositionFromLine(native.DocLineFromVisible(row + rows + 3))));
        Check(native.PointYFromPosition(native.CurrentPos) >= viewport,
            "The partial-row fixture starts with the caret below the viewport");
        await AfterPaintAsync(native, () => native.MoveCaretInsideView());
        var y = native.PointYFromPosition(native.CurrentPos);
        Check(native.FirstVisibleLine == row && view.DisplayVerticalOffset == offset,
            $"A caret below a partial row reveals without scrolling: expected=({row},{offset}), actual=({native.FirstVisibleLine},{view.DisplayVerticalOffset})");
        Check(y >= 0 && y + textHeight <= viewport,
            $"A caret below a partial row ends wholly inside the viewport: y={y}, height={textHeight}, viewport={viewport}");
    }

    private static async Task CheckAnimationAsync(TextEditorCore core, EditorBaseControl view, double scale)
    {
        var native = view.Editor;
        var rowHeight = native.TextHeight(0) / scale;
        if (native.FirstVisibleLine != 100 || view.DisplayVerticalOffset != 0)
            await AfterPaintAsync(native, () => view.SetVerticalViewport(100, 0));
        else
            view.SetVerticalViewport(100, 0);
        var animated = new UISettings().AnimationsEnabled;
        var samples = 0;
        var valid = true;
        var previous = 100 * rowHeight;
        void Painted(Editor sender, PaintedEventArgs args)
        {
            var current = sender.FirstVisibleLine * rowHeight + view.DisplayVerticalOffset;
            valid &= current >= previous && current <= 103.5 * rowHeight + 1e-8;
            previous = current;
            ++samples;
        }
        native.Painted += Painted;
        try
        {
            await AfterPaintAsync(native, () =>
            {
                view.ScrollBy(0, 3.5 * rowHeight, true);
                Check(animated ? view.IsScrollAnimating && native.FirstVisibleLine == 100 && view.DisplayVerticalOffset == 0 :
                    !view.IsScrollAnimating, "Frame-driven input is deferred only when Windows animations are enabled");
            });
            await UntilAsync(() => !view.IsScrollAnimating);
            Check(native.FirstVisibleLine == 103 && Math.Abs(view.DisplayVerticalOffset - rowHeight / 2) < 1e-8,
                "Animated and immediate input reach the exact fractional target");
            Check(samples > 0 && valid, "Observed animation paints move monotonically without overshoot");
        }
        finally { native.Painted -= Painted; }
        if (animated)
        {
            view.ScrollBy(0, 20 * rowHeight, true);
            view.ScrollBy(0, -rowHeight / 2, true);
            await UntilAsync(() => !view.IsScrollAnimating);
            Check(native.FirstVisibleLine == 103 && Math.Abs(view.DisplayVerticalOffset) < 1e-8,
                "A reversal immediately replaces the old direction rather than its distant target");
            view.ScrollBy(0, 20 * rowHeight, true);
            await AfterPaintAsync(native, () => view.SetVerticalViewport(50, rowHeight / 4));
            Check(!view.IsScrollAnimating && native.FirstVisibleLine == 50 && view.DisplayVerticalOffset == rowHeight / 4,
                "Explicit viewport replacement cancels the older frame subscription");
            view.ScrollBy(0, 20 * rowHeight, true);
            await AfterPaintAsync(native, () => core.TypeText("x"));
            Check(!view.IsScrollAnimating, "Document edits retire old wheel motion");
        }
    }

    private static async Task CheckHorizontalAsync(StringBuilder log, bool rtl)
    {
        using var core = new TextEditorCore
        {
            Width = 620, Height = 350, FontSize = 17, RightToLeftReadingOrder = rtl,
            TextWrapping = TextWrapping.NoWrap,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        var view = (EditorBaseControl)((Grid)core.Content).Children[0];
        var native = view.Editor;
        var text = string.Join('\r', Enumerable.Repeat(string.Concat(Enumerable.Repeat("alpha 中文 אבג مرحبا 123 ", 24)), 80));
        core.SetText(text);
        await AfterPaintAsync(native, () => Window.Current.Content = core);
        await UntilAsync(() => !view.IsViewportRestorePending);
        var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        var height = native.TextHeight(0) / scale;
        var start = (int)Math.Round(80 * scale);
        var animated = new UISettings().AnimationsEnabled;
        await AfterPaintAsync(native, () => native.XOffset = start);
        await AfterPaintAsync(native, () => view.SetVerticalViewport(20, 0));
        var target = Find<FrameworkElement>(view, "ImageTarget");
        var sample = native.PositionFromPointClose((int)(target.ActualWidth * scale / 2),
            native.PointYFromPosition(native.PositionFromLine(21)) + native.TextHeight(0) / 2);
        Check(sample >= 0, "The horizontal geometry sample hits visible text");
        var provider = (ITextProvider)FrameworkElementAutomationPeer.CreatePeerForElement(view).GetPattern(PatternInterface.Text);
        var glyph = provider.DocumentRange.Clone();
        glyph.MoveEndpointByRange(TextPatternRangeEndpoint.End, glyph, TextPatternRangeEndpoint.Start);
        var characters = (int)native.CountCharacters(0, sample);
        Check(glyph.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, TextUnit.Character, characters) == characters &&
            glyph.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 1) == 1,
            "The horizontal UIA sample reaches the independently hit-tested glyph");
        glyph.GetBoundingRectangles(out var before);
        var previous = start;
        var paints = 0;
        var monotonic = true;
        var expected = (int)Math.Round(start + 83.375 * scale, MidpointRounding.AwayFromZero);
        void Painted(Editor sender, PaintedEventArgs args)
        {
            monotonic &= sender.XOffset >= previous && sender.XOffset <= expected;
            previous = sender.XOffset;
            ++paints;
        }
        native.Painted += Painted;
        try
        {
            await AfterPaintAsync(native, () =>
            {
                view.ScrollBy(83.375, 0, true);
                Check(!animated || view.IsScrollAnimating && native.XOffset == start,
                    "Horizontal animation starts from the displayed viewport without an immediate jump");
            });
            await UntilAsync(() => !view.IsScrollAnimating);
            Check(native.XOffset == expected && native.FirstVisibleLine == 20 && view.DisplayVerticalOffset == 0 &&
                paints > 0 && monotonic, "Horizontal paints reach the exact pixel target monotonically without moving Y");
        }
        finally { native.Painted -= Painted; }
        glyph.GetBoundingRectangles(out var after);
        Check(before.Length == 4 && after.Length == 4 && Math.Abs(before[0] - after[0] - (expected - start)) < 0.1 &&
            Math.Abs(before[1] - after[1]) < 0.1, "A retained UIA glyph follows exactly the horizontal pixel translation");
        var bar = Find<ScrollBar>(view, "HorizontalScrollBar");
        Check(bar != null && bar.Value == native.XOffset, "The horizontal thumb follows the actual animated pixels");
        await AfterPaintAsync(native, () => native.XOffset = start);
        view.ScrollBy(100, 0, true);
        view.ScrollBy(0, 3.5 * height, true);
        await UntilAsync(() => !view.IsScrollAnimating);
        Check(native.XOffset == (int)Math.Round(start + 100 * scale) && native.FirstVisibleLine == 23 &&
            Math.Abs(view.DisplayVerticalOffset - height / 2) < 1e-8,
            "Orthogonal input retains both independent axis targets");
        await AfterPaintAsync(native, () => native.XOffset = start);
        if (animated)
        {
            view.ScrollBy(300, 8 * height, true);
            view.ScrollBy(-10, 0, true);
            await UntilAsync(() => !view.IsScrollAnimating);
            Check(native.XOffset == (int)Math.Round(start - 10 * scale) && native.FirstVisibleLine == 31 &&
                Math.Abs(view.DisplayVerticalOffset - height / 2) < 1e-8,
                "Reversing X immediately replaces its old target while preserving Y motion");
        }
        foreach (var animate in new[] { false, true })
        {
            await AfterPaintAsync(native, () => native.XOffset = start + 10);
            await AfterPaintAsync(native, () => native.XOffset = start);
            for (var input = 0; input < 32; ++input)
            {
                view.ScrollBy(0.125 / scale, 0, animate);
                await UntilAsync(() => !view.IsScrollAnimating);
            }
            Check(native.XOffset == start + 4, "Subpixel horizontal inputs accumulate across completed and immediate requests");
        }
        view.ScrollBy(100, height, true);
        native.XOffset = native.XOffset;
        Check(!view.IsScrollAnimating, "Even unchanged explicit X positioning retires both old axis targets");
        view.ScrollBy(double.MaxValue, 0, false);
        Check(native.XOffset == (int)bar.Maximum && !view.IsScrollAnimating, "Horizontal input clamps exactly to the trailing boundary");
        view.ScrollBy(10, 0, true);
        Check(!view.IsScrollAnimating, "An outward input at the horizontal boundary leaves no frame subscription");
        view.ScrollBy(-double.MaxValue, 0, false);
        Check(native.XOffset == 0 && !view.IsScrollAnimating && core.GetText() == text,
            "Horizontal movement clamps at zero and preserves document content");
        log.AppendLine($"PASS: horizontal/diagonal targets, reversal, subpixel accumulation, cancellation and boundaries: rtl={rtl}.");
    }

    private static async Task CheckCanvasGrowthAsync(StringBuilder log, bool rtl, bool animate)
    {
        // ClearAll retains the measured width during ordinary edits. A fresh
        // editor ensures each fixture really discovers a previously unseen width.
        using var core = new TextEditorCore
        {
            Width = 620, Height = 350, FontSize = 17, RightToLeftReadingOrder = rtl,
            TextWrapping = TextWrapping.NoWrap,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        var view = (EditorBaseControl)((Grid)core.Content).Children[0];
        var native = view.Editor;
        var shortLine = string.Concat(Enumerable.Repeat("alpha 中文 אבג مرحبا 123 ", 5));
        var longLine = string.Concat(Enumerable.Repeat(shortLine, 8));
        core.SetText(string.Join('\r', Enumerable.Range(0, 80).Select(line => line < 40 ? shortLine : longLine)));
        await AfterPaintAsync(native, () => Window.Current.Content = core);
        await UntilAsync(() => !view.IsViewportRestorePending);
        await AfterPaintAsync(native, () => view.RestoreViewport(100, 5, 0, 0, false));
        await UntilAsync(() => !view.IsViewportRestorePending);
        var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        var height = native.TextHeight(0) / scale;
        var bar = Find<ScrollBar>(view, "HorizontalScrollBar");
        var originalMaximum = bar.Maximum;
        await AfterPaintAsync(native, () => view.ScrollBy(rtl ? -60 : 60, 40 * height, animate));
        await UntilAsync(() => !view.IsScrollAnimating);
        // Width publication is debounced independently of painting. Observe the
        // actual new range, then verify neither target was lost as it published.
        await UntilAsync(() => bar.Maximum > originalMaximum);
        Check(Math.Abs(view.DisplayHorizontalOffset - 160) <= 1 / scale && native.FirstVisibleLine == 45,
            $"Canvas growth retains both targets: animated={animate}, expected=(45,160), " +
            $"actual=({native.FirstVisibleLine},{view.DisplayHorizontalOffset})");
        log.AppendLine($"PASS: diagonal canvas growth: rtl={rtl}, animated={animate}.");
    }

    private static async Task CheckDiffAsync(StringBuilder log, bool rtl)
    {
        var text = string.Join('\r', Enumerable.Repeat(string.Concat(Enumerable.Repeat("alpha 中文 אבג مرحبا ", 16)), 100));
        using var baseline = await DocumentBaseline.FromTextAsync(text, Guid.NewGuid());
        using var source = new TextEditorCore { RightToLeftReadingOrder = rtl };
        source.SetText(text + " changed");
        using var viewer = new SideBySideDiffViewer { Width = 760, Height = 360 };
        Check(await viewer.PrepareAsync(baseline, source, DocumentLanguages.PlainText, "scroll.txt",
            System.Threading.CancellationToken.None), "The smooth-scroll fixture prepares a real Diff viewer");
        var old = Find<TextEditorCore>(viewer, "OldEditor");
        var current = Find<TextEditorCore>(viewer, "NewEditor");
        var left = (EditorBaseControl)((Grid)old.Content).Children[0];
        var right = (EditorBaseControl)((Grid)current.Content).Children[0];
        await AfterPaintAsync(right.Editor, () => Window.Current.Content = viewer);
        await UntilAsync(() => !left.IsViewportRestorePending && !right.IsViewportRestorePending);
        await AfterPaintAsync(left.Editor, () => left.SetVerticalViewport(50, 0));
        await UntilAsync(() => right.Editor.FirstVisibleLine == 50 && !right.IsViewportRestorePending);
        var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        var height = left.Editor.TextHeight(0) / scale;
        bool Synchronized() => !left.IsScrollAnimating && !right.IsScrollAnimating &&
            !left.IsViewportRestorePending && !right.IsViewportRestorePending &&
            left.Editor.FirstVisibleLine == right.Editor.FirstVisibleLine &&
            Math.Abs(left.DisplayVerticalOffset - right.DisplayVerticalOffset) < 1e-8;
        await AfterPaintAsync(left.Editor, () => left.SetDisplayViewport(50, 100, 0));
        await UntilAsync(Synchronized);
        left.ScrollBy(rtl ? -45 : 45, 3.5 * height, true);
        await UntilAsync(Synchronized);
        Check(left.Editor.FirstVisibleLine == 53 && Math.Abs(left.DisplayVerticalOffset - height / 2) < 1e-8,
            "Diff peer acknowledgements do not cancel the driving animation before its full target");
        Check(Math.Abs(left.DisplayHorizontalOffset - 145) <= 1 / scale &&
            left.DisplayHorizontalOffset == right.DisplayHorizontalOffset,
            "Diff panes share the diagonal leading offset without cancelling the driver's horizontal target");
        if (new UISettings().AnimationsEnabled)
        {
            left.ScrollBy(0, 10 * height, true);
            Check(left.IsScrollAnimating, "The original Diff pane owns an unfinished animation");
            right.ScrollBy(0, -height / 2, true);
            Check(!left.IsScrollAnimating, "The other pane's input synchronously retires the older animation");
            await UntilAsync(Synchronized);
            Check(right.Editor.FirstVisibleLine == 53 && Math.Abs(right.DisplayVerticalOffset) < 1e-8,
                "The most recent Diff interaction wins without an older frame overwriting it");
            await AfterPaintAsync(right.Editor, () => right.SetVerticalViewport(0, 0));
            await UntilAsync(Synchronized);
            left.ScrollBy(0, 10 * height, true);
            right.ScrollBy(0, -height, true);
            Check(!left.IsScrollAnimating && !right.IsScrollAnimating,
                "Even an input clamped at the leading boundary takes ownership from the older pane");
            Check(left.Editor.FirstVisibleLine == 0 && right.Editor.FirstVisibleLine == 0,
                "A clamped takeover leaves both Diff panes at the visible boundary");
            left.ScrollBy(rtl ? -100 : 100, 10 * height, true);
            right.ScrollBy(rtl ? -10 : 10, 0, true);
            Check(!left.IsScrollAnimating, "Horizontal-only peer input retires both axes of the older driver");
            await UntilAsync(Synchronized);
            Check(left.Editor.FirstVisibleLine == 0 && right.Editor.FirstVisibleLine == 0,
                "Horizontal-only takeover does not revive the old vertical target");
        }
        log.AppendLine($"PASS: shared fractional Diff viewport and animation policy: rtl={rtl}, handoffExercised={new UISettings().AnimationsEnabled}.");
    }

    private static T Find<T>(DependencyObject root, string name) where T : FrameworkElement
    {
        if (root is T result && result.Name == name) return result;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); ++i)
            if (Find<T>(VisualTreeHelper.GetChild(root, i), name) is { } child) return child;
        return null;
    }

    private static async Task UntilAsync(Func<bool> ready)
    {
        if (ready()) return;
        var completion = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        void Rendering(object sender, object args) { if (ready()) completion.TrySetResult(true); }
        CompositionTarget.Rendering += Rendering;
        try { await completion.Task.WaitAsync(TimeSpan.FromSeconds(10)); }
        finally { CompositionTarget.Rendering -= Rendering; }
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
