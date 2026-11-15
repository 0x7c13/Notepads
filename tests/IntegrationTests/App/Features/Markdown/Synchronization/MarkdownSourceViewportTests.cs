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
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using WinUIEditor;
using static NotepadsEditorTests.EditorTestLayout;

namespace NotepadsIntegrationTests;

internal static class MarkdownSourceViewportTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var previous = Window.Current.Content;
        var root = new Grid();
        using var core = new TextEditorCore
        {
            Width = 620, Height = 351, TextWrapping = TextWrapping.Wrap,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        using var zero = new TextEditorCore { Width = 0, Height = 0 };
        var view = (EditorBaseControl)((Grid)core.Content).Children[0];
        var native = view.Editor;
        var zeroView = (EditorBaseControl)((Grid)zero.Content).Children[0];
        try
        {
            root.Children.Add(core);
            root.Children.Add(zero);
            var text = "prefix " + string.Join(' ', Enumerable.Range(0, 500).Select(index =>
                "中文 Ω😀 مرحبا " + index.ToString("D3"))) + "\rtail Ω😀";
            var bytes = Encoding.UTF8.GetBytes(text);
            core.SetText(text);
            await AfterPaintAsync(native, () =>
            {
                Window.Current.Content = root;
                Window.Current.Activate();
            });
            await UntilAsync(() => !view.IsViewportRestorePending && view.GetSourceViewport().IsAvailable);
            Check(!zeroView.GetSourceViewport().IsAvailable && !zero.TryGetMarkdownViewport(out _),
                "A loaded zero-size native source reports unavailable without a fabricated row.");

            foreach (var rtl in new[] { false, true })
            {
                core.RightToLeftReadingOrder = rtl;
                await AfterPaintAsync(native, () => view.SetVerticalViewport(2, 0));
                await UntilAsync(() => view.GetSourceViewport().IsAvailable);
                Check(native.WrapCount(0) > native.LinesOnScreen * 3,
                    "The first actual CJK/emoji/bidi logical line wraps across at least three source screens.");
                core.Focus(FocusState.Programmatic);
                native.SetSel(2, 6);
                var focus = FocusManager.GetFocusedElement();
                var selection = (native.Anchor, native.CurrentPos, native.SelectionRevision);
                var physicalHeight = native.TextHeight(0);
                var scale = view.XamlRoot.RasterizationScale;
                var dipHeight = physicalHeight / scale;
                SourceViewportSpan first = default;
                var previousFraction = -1.0;
                foreach (var requested in new[] { 0.25, 0.5, 0.75 })
                {
                    await AfterPaintAsync(native, () => view.SetVerticalViewport(2, dipHeight * requested));
                    await UntilAsync(() => view.GetSourceViewport().IsAvailable);
                    var span = view.GetSourceViewport();
                    Check(span.DisplayRow == 2 && span.StartByte > 0 && span.EndByte > span.StartByte &&
                        native.LineFromPosition(span.StartByte) == 0 && native.LineFromPosition(span.EndByte) == 0,
                        "The span preserves actual soft-wrap row identity within one logical line.");
                    Check(ScalarBoundary(bytes, span.StartByte) && ScalarBoundary(bytes, span.EndByte),
                        "Both actual CJK/emoji row endpoints are UTF-8 scalar boundaries.");
                    var top = native.PointYFromPosition(span.StartByte);
                    var next = native.PointYFromPosition(span.EndByte);
                    var rawPixels = dipHeight * requested * scale;
                    var expectedPixels = Math.Round(rawPixels, MidpointRounding.AwayFromZero);
                    log.AppendLine(FormattableString.Invariant($"OBS: Markdown.SourceViewportFraction rtl={rtl}, requested={requested:R}, heightPx={physicalHeight}, scale={scale:R}, dipHeight={dipHeight:R}, rawPixels={rawPixels:R}, expectedPixelsAway={expectedPixels:R}, actualFraction={span.Fraction:R}, previousFraction={previousFraction:R}, pointTop={top}, pointNext={next}, displayRow={span.DisplayRow}, rawDisplayRow={view.DisplayFirstVisibleRow}, actualOffsetDip={view.DisplayVerticalOffset:R}, pair={span.StartByte}..{span.EndByte}."));
                    Check(next - top == physicalHeight && Math.Abs(span.Fraction + top / (double)physicalHeight) < 0.000001,
                        "The reported fraction and next-row endpoint agree with independent native point geometry.");
                    Check(Math.Abs(span.Fraction - expectedPixels / physicalHeight) < 0.000001 &&
                        span.Fraction > previousFraction,
                        "Three same-row requests retain distinct actual physical-pixel fractions.");
                    if (previousFraction >= 0)
                        Check(span.StartByte == first.StartByte && span.EndByte == first.EndByte,
                            "Moving inside one source display row keeps the same semantic endpoint pair.");
                    else first = span;
                    Check(core.TryGetMarkdownViewport(out var managed) && managed.StartByte == span.StartByte &&
                        managed.EndByte == span.EndByte && managed.DisplayRow == span.DisplayRow && managed.Fraction == span.Fraction &&
                        managed.NativeRevision == native.DocumentRevision && managed.Length == native.Length &&
                        managed.ContentVersion == core.ContentVersion,
                        "The managed sample names the same native/content stamp and exact value-only viewport.");
                    previousFraction = span.Fraction;
                }
                await AfterPaintAsync(native, () => view.SetVerticalViewport(3, 0));
                await UntilAsync(() => view.GetSourceViewport().IsAvailable);
                var nextRow = view.GetSourceViewport();
                Check(nextRow.DisplayRow == 3 && nextRow.StartByte == first.EndByte && nextRow.Fraction == 0,
                    "Adjacent actual display-row spans share exactly the same source endpoint.");
                await AfterPaintAsync(native, () => view.SetVerticalViewport(2, dipHeight - 0.2 / scale));
                await UntilAsync(() => view.GetSourceViewport().IsAvailable);
                var roundedBoundary = view.GetSourceViewport();
                Check(roundedBoundary.DisplayRow == 3 && roundedBoundary.StartByte == nextRow.StartByte && roundedBoundary.Fraction == 0,
                    "Physical rounding at the row boundary reports the next actual row at fraction zero.");
                Check((native.Anchor, native.CurrentPos, native.SelectionRevision) == selection &&
                    ReferenceEquals(focus, FocusManager.GetFocusedElement()),
                    "Sampling/fractional viewport requests preserve selection revision and focus.");
                log.AppendLine($"OBS: Markdown.SourceViewportSpan rtl={rtl}, scale={scale}, rowHeightPx={physicalHeight}, pair={first.StartByte}..{first.EndByte}, shared={nextRow.StartByte}.");
            }

            var geometry = 0;
            var sourceRevision = native.DocumentRevision;
            var sourceVersion = core.ContentVersion;
            EventHandler changed = (_, _) => geometry++;
            core.MarkdownViewportGeometryChanged += changed;
            try
            {
                var before = geometry;
                core.TextWrapping = TextWrapping.NoWrap;
                await UntilAsync(() => geometry > before && view.GetSourceViewport().IsAvailable);
                before = geometry;
                core.FontSize += 2;
                await UntilAsync(() => geometry > before && view.GetSourceViewport().IsAvailable);
                Check(native.DocumentRevision == sourceRevision && core.ContentVersion == sourceVersion && core.MarkdownUtf8Length == bytes.Length,
                    "Wrap/font geometry wakes the bridge without changing source identity.");
            }
            finally { core.MarkdownViewportGeometryChanged -= changed; }

            core.RightToLeftReadingOrder = false;
            var preferredText = new string('a', 30) + "\ra\r" + new string('a', 30) + "\rtail";
            await AfterPaintAsync(native, () => core.SetText(preferredText));
            await UntilAsync(() => view.GetSourceViewport().IsAvailable);
            native.GotoPos(10);
            native.ChooseCaretX();
            native.LineDown();
            Check(native.CurrentPos == native.GetLineEndPosition(1), "The preferred-column fixture clamps on its short middle line.");
            var shortSelection = (native.Anchor, native.CurrentPos, native.SelectionRevision);
            for (var index = 0; index < 10; index++) Check(view.GetSourceViewport().IsAvailable, "A valid native sample remains available.");
            Check((native.Anchor, native.CurrentPos, native.SelectionRevision) == shortSelection,
                "Repeated getters never edit native selection state.");
            native.LineDown();
            Check(native.CurrentPos == native.PositionFromLine(2) + 10,
                "Repeated source sampling preserves the existing native preferred column.");

            native.EndAtLastLine = false;
            var last = native.LineCount - 1;
            await AfterPaintAsync(native, () => view.SetVerticalViewport(native.VisibleFromDocLine(last), 0));
            await UntilAsync(() => view.GetSourceViewport().IsAvailable);
            var eof = view.GetSourceViewport();
            Check(eof.StartByte == native.PositionFromLine(last) && eof.EndByte == native.Length && eof.Fraction == 0,
                "The last actual display row uses the counted document length as its next endpoint.");
            await AfterPaintAsync(native, () => core.SetText(""));
            await UntilAsync(() => view.GetSourceViewport().IsAvailable);
            var empty = view.GetSourceViewport();
            Check(empty.StartByte == 0 && empty.EndByte == 0 && empty.DisplayRow == 0 && empty.Fraction == 0 &&
                core.TryGetMarkdownViewport(out var emptyManaged) && emptyManaged.Length == 0,
                "An empty loaded source has the explicit available zero-length span.");
            log.AppendLine("PASS: Markdown.SourceViewportSpan — actual native wrap/bidi/CJK/emoji scalar endpoints, physical fractions and exact boundary, shared next row, managed stamp, geometry wake, zero size/empty/EOF, focus/selection/preferred column. Only the actual current DPI was exercised.");
        }
        finally
        {
            root.Children.Clear();
            Window.Current.Content = previous;
        }
    }

    private static bool ScalarBoundary(byte[] bytes, long position) => position >= 0 && position <= bytes.Length &&
        (position == bytes.Length || (bytes[checked((int)position)] & 0xc0) != 0x80);

    private static Task UntilAsync(Func<bool> condition) => NotepadsWorkspaceTests.WorkspaceTestUI.UntilAsync(condition,
        "The actual native source layout must make the expected viewport sample available.");

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
