// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Graphics.Display;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Automation.Text;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Media;
using WinUIEditor;
using SelectionMode = WinUIEditor.SelectionMode;
using static NotepadsEditorTests.EditorTestLayout;

namespace NotepadsEditorTests;

// These checks exercise the real Scintilla control. Exact text/byte expectations
// are deliberately fixed; the native helper separately verifies shaping/raster
// output against an independent whole-paragraph DirectWrite layout.
internal static partial class BidiTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        Progress("construct editor");
        var previousContent = Window.Current.Content;
        using var core = new TextEditorCore
        {
            Width = 620, Height = 420, FontSize = 24,
            FontFamily = new FontFamily("Consolas"),
            DisplayLineNumbers = false, TextWrapping = TextWrapping.NoWrap,
            RequestedTheme = ElementTheme.Light,
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Top
        };
        var view = (EditorBaseControl)((Grid)core.Content).Children[0];
        var native = view.Editor;
        var host = new Grid();
        host.Children.Add(core);
        try
        {
            Progress("attach editor");
            await AfterPaintAsync(native, () =>
            {
                Window.Current.Content = host;
                Window.Current.Activate();
            });
            Progress("live RTL viewport");
            await CheckLiveRtlViewportAsync();
            log.AppendLine("PASS: lazy RTL canvas growth preserves the live leading offset while explicit physical scrolling remains authoritative.");
            Progress("direction/alignment");
            await CheckDirectionAsync(core, native, host, view);
            log.AppendLine("PASS: reading direction changes alignment without mirroring UI or changing text/undo.");
            await CheckGutterPlacementAsync(core, native, view);
            log.AppendLine("PASS: document direction moves native line numbers, scrollbar inset and hover edge together.");
            await CheckGutterPreferredColumnAsync(core, native);
            log.AppendLine("PASS: vertical navigation retains its chosen column after gutter and spacing changes.");
            await CheckPreferredColumnCanvasGrowthAsync();
            log.AppendLine("PASS: vertical navigation preserves its chosen column across previously unmeasured wider rows.");
            await CheckGutterWrapTransitionAsync(core, native);
            log.AppendLine("PASS: an immediate gutter and wrap-mode change preserves rectangle columns and undo.");
            Progress("LTR leading origins");
            await CheckLeadingOriginsAsync(core, native);
            log.AppendLine("PASS: LTR line origins remain at the text margin across Unicode clusters, controls and wrapping.");
            Progress("Hebrew editing");
            await CheckHebrewEditingAsync(core, native);
            log.AppendLine("PASS: RTL arrows, Shift selection, replacement, deletion and undo use exact logical text.");
            await CheckUndoCaretAsync(core, native, view);
            log.AppendLine("PASS: undo/redo retain mixed-boundary caret geometry across insertion, deletion and grouped history.");
            Progress("line and word navigation");
            await CheckLineNavigationAsync(core, native);
            log.AppendLine("PASS: RTL Home/End, word movement, selection collapse and vertical navigation.");
            await CheckWordTransitionsAsync(core, native);
            log.AppendLine("PASS: word commands retain their granularity across plain, mixed and empty paragraphs.");
            Progress("plain wrap affinity");
            await CheckPlainWrapAffinityAsync(core, native, view);
            log.AppendLine("PASS: plain wrap-boundary insertion keeps caret geometry, scrolling and rectangular replacement on the next row, with exact undo/redo.");
            Progress("wrapped page navigation");
            await CheckPageNavigationAsync(core, native, view);
            log.AppendLine("PASS: PageUp/Down preserve the display row and trailing affinity at wrap boundaries.");
            Progress("mixed editing");
            await CheckMixedEditingAsync(core, native);
            log.AppendLine("PASS: mixed-direction click positions, styled runs and logical replacement remain aligned.");
            await CheckDrawingByteBoundariesAsync();
            log.AppendLine("PASS: byte-split styles, noncharacters and truncated UTF-8 retain their bytes and complete real-control layout and paint.");
            await CheckStreamSelectionEdgesAsync(core, native, view);
            log.AppendLine("PASS: mixed stream selections collapse to their outer visual edge in either direction.");
            Progress("rectangle editing");
            await CheckColdRectangleCanvasAsync(core, native);
            await CheckRectangleZoomCollapseAsync(core, native);
            log.AppendLine("PASS: rectangle collapse retains its physical corners after synchronous font and zoom changes.");
            await CheckRectangleMetricReplacementAsync(core, native);
            log.AppendLine("PASS: rectangle replacement retains logical insertion anchors across font changes and repeated typing.");
            await CheckRectanglesAsync(core, native);
            log.AppendLine("PASS: visual rectangles copy/delete/replace only their logical islands with grouped undo.");
            await CheckRectangleTabAsync(core, native);
            log.AppendLine("PASS: rectangular Tab inserts once per visual row, with exact undo and subsequent typing.");
            await CheckRectanglePasteGrowthAsync(core, native, view);
            log.AppendLine("PASS: rectangular paste preserves later insertion columns when an earlier row widens the canvas.");
            Progress("virtual-space hit targets");
            await CheckVirtualSpaceAsync(core, native);
            log.AppendLine("PASS: opposite-direction glyphs remain real hit targets and pure RTL virtual padding is preserved.");
            Progress("wrapping");
            await CheckWrappingAsync(core, native);
            log.AppendLine("PASS: wrapped RTL paragraphs refresh after resize, zoom and direction changes.");
            Progress("UIA geometry");
            await CheckAutomationAsync(core, native, view);
            log.AppendLine("PASS: UIA RTL and disjoint mixed selections expose positive visual rectangles and logical text.");
            Progress("completed");
        }
        finally { Window.Current.Content = previousContent; }
    }

    private static async Task LoadAsync(TextEditorCore core, Editor native, string text, bool rtl)
    {
        await AfterPaintAsync(native, () =>
        {
            core.RightToLeftReadingOrder = rtl;
            native.SelectionMode = SelectionMode.Stream;
            core.SetText(text);
            native.SetEmptySelection(0);
            native.XOffset = 0;
            native.FirstVisibleLine = 0;
        });
    }

    private static T FindElement<T>(DependencyObject root, string name) where T : FrameworkElement
    {
        if (root is T element && element.Name == name) return element;
        for (var index = 0; index < VisualTreeHelper.GetChildrenCount(root); index++)
            if (FindElement<T>(VisualTreeHelper.GetChild(root, index), name) is { } found) return found;
        return null;
    }

    private static async Task CheckDrawingByteBoundariesAsync()
    {
        var previousContent = Window.Current.Content;
        using var core = new TextEditorCore
        {
            Width = 620, Height = 420, FontSize = 24,
            FontFamily = new FontFamily("Consolas"),
            DisplayLineNumbers = false, TextWrapping = TextWrapping.NoWrap
        };
        var native = ((EditorBaseControl)((Grid)core.Content).Children[0]).Editor;
        var fixtures = new (string Name, byte[] Bytes)[]
        {
            ("scalar-split styles", Encoding.UTF8.GetBytes("Aא😀Z")),
            ("noncharacters", Encoding.UTF8.GetBytes("A\uFDD0\uFDEF\uFFFE\uFFFF\U0001FFFE\U0010FFFFאZ")),
            ("truncated UTF-8", [.. Encoding.UTF8.GetBytes("Aא "), 0xf0, 0x9f])
        };
        try
        {
            await AfterPaintAsync(native, () => Window.Current.Content = core);
            foreach (var rtl in new[] { false, true })
            {
                foreach (var (name, bytes) in fixtures)
                {
                    await AfterPaintAsync(native, () =>
                    {
                        core.RightToLeftReadingOrder = rtl;
                        core.SetText(string.Empty);
                        native.AddTextFromBuffer(bytes.Length, bytes.AsBuffer());
                        native.StyleSetSizeFractional(1, 3000);
                        native.StyleSetSizeFractional(2, 1800);
                        native.StartStyling(0, 0);
                        // Deliberately split scalar bytes across font styles. The
                        // bridge must choose complete drawing units for each run.
                        for (var index = 0; index < bytes.Length; index++)
                            native.SetStyling(1, index % 2 + 1);
                        native.SetSel(0, bytes.Length);
                        _ = native.PointXFromPosition(bytes.Length);
                    });
                    // SCI_GETCHARAT exposes C++ char's signed value through the
                    // integer ABI. Compare the original eight bits as bytes.
                    Check(native.Length == bytes.Length && bytes.Select((value, index) =>
                        unchecked((byte)native.GetCharAt(index)) == value).All(matches => matches),
                        $"{name}, RTL={rtl}: layout and paint preserve all source bytes");
                    for (long position = 0; position < native.Length;)
                    {
                        var next = native.PositionAfter(position);
                        Check(next > position && next <= native.Length,
                            $"{name}, RTL={rtl}: drawing positions make bounded progress");
                        Check(native.PointYFromPosition(next) == native.PointYFromPosition(0),
                            $"{name}, RTL={rtl}: caret geometry remains on the unwrapped row");
                        _ = native.PointXFromPosition(next);
                        position = next;
                    }
                }
            }
        }
        finally { Window.Current.Content = previousContent; }
    }

    private static async Task CheckLiveRtlViewportAsync()
    {
        var previousContent = Window.Current.Content;
        var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;

        async Task WithDocumentAsync(bool rtl, bool shortRows,
            Func<TextEditorCore, Editor, EditorBaseControl, long[], Task> inspect)
        {
            using var editor = new TextEditorCore
            {
                Width = 620, Height = 420, FontSize = 24,
                FontFamily = new FontFamily("Consolas"),
                DisplayLineNumbers = false, TextWrapping = TextWrapping.NoWrap,
                RightToLeftReadingOrder = rtl, RequestedTheme = ElementTheme.Light,
                HorizontalAlignment = HorizontalAlignment.Left,
                VerticalAlignment = VerticalAlignment.Top
            };
            var glyph = rtl ? 'א' : 'a';
            var rows = Enumerable.Repeat(new string(glyph, shortRows ? 3 : 120), shortRows ? 100 : 200).ToArray();
            rows[80] = new string(glyph, shortRows ? 600 : 300);
            if (!shortRows)
            {
                rows[120] = new string(glyph, 600);
                rows[160] = new string(glyph, 900);
            }
            var starts = new long[rows.Length];
            for (var line = 1; line < rows.Length; line++)
                starts[line] = starts[line - 1] + Encoding.UTF8.GetByteCount(rows[line - 1]) + 1;
            var text = string.Join('\r', rows);
            await editor.LoadTextAsync(text);
            var view = (EditorBaseControl)((Grid)editor.Content).Children[0];
            var native = view.Editor;
            native.CaretWidth = 2;
            try
            {
                await AfterPaintAsync(native, () => Window.Current.Content = editor);
                Check(!view.IsViewportRestorePending && native.FirstVisibleLine == 0 && native.LinesOnScreen < 70 &&
                    double.IsFinite(RightEdge(view)),
                    "Live-canvas fixture starts with only the initial rows measured and its initial viewport restored");
                if (rtl)
                {
                    // Establish the actual text edge independently of caret
                    // scrolling's visibility slack. Only visible row zero is
                    // queried; none of the distant long rows is warmed.
                    await AfterPaintAsync(native, () =>
                    {
                        native.XOffset += checked((int)Math.Round(native.PointXFromPosition(0) -
                            (RightEdge(view) - native.CaretWidth)));
                        native.SetSelection(0, 0);
                    });
                    Check(!shortRows || native.XOffset == 0,
                        "The short-row fixture has not measured its distant long row before the operation");
                }
                await inspect(editor, native, view, starts);
                Check(editor.GetText() == text && native.CurrentPos == 0 && native.Anchor == 0 && native.Selections == 1,
                    "Live viewport changes preserve the exact document and independent caret");
            }
            finally { Window.Current.Content = previousContent; }
        }

        double RightEdge(EditorBaseControl view)
        {
            var surface = FindElement<Border>(view, "ImageTarget");
            // Painted callbacks record failed observations instead of throwing
            // across the native event boundary if layout is unexpectedly absent.
            if (surface is not { ActualWidth: > 0 }) return double.NaN;
            var native = view.Editor;
            double gutter = 0;
            for (var margin = 0; margin < native.Margins; margin++) gutter += native.GetMarginWidthN(margin);
            return surface.ActualWidth * scale - native.MarginRight -
                (native.Bidirectional == Bidirectional.R2l ? gutter : 0);
        }

        double LeadingOffset(TextEditorCore editor)
        {
            editor.GetScrollViewerPosition(out var horizontal, out _);
            return horizontal;
        }

        await WithDocumentAsync(true, true, async (editor, native, view, starts) =>
        {
            var distance = RightEdge(view) - native.PointXFromPosition(2);
            var initialX = native.XOffset;
            Check(distance > 0 && distance < RightEdge(view) && Math.Abs(LeadingOffset(editor)) <= 1 / scale,
                "The short RTL source is visible at its leading edge before the distant long row is measured");
            var paints = new List<bool>();
            var expectedLine = 78;
            void Painted(Editor sender, PaintedEventArgs args)
            {
                var physicalX = sender.XOffset;
                var leading = LeadingOffset(editor);
                var valid = sender.FirstVisibleLine == expectedLine &&
                    Math.Abs(RightEdge(view) - sender.PointXFromPosition(starts[expectedLine] + 2) - distance) <= 1 &&
                    Math.Abs(leading) <= 1 / scale;
                // Geometry queries may reconcile a newly measured canvas.
                // Observing an already-painted frame must not repair it.
                paints.Add(valid && sender.XOffset == physicalX);
            }
            native.Painted += Painted;
            try
            {
                await AfterPaintAsync(native, () => native.FirstVisibleLine = 78);
                await AfterPaintAsync(native, () => native.SetSelection(0, 0));
                Check(paints.Count > 0 && paints.All(valid => valid) && native.XOffset > initialX + 1000,
                    "Every paint revealing the distant long RTL row keeps neighboring short rows at the same visible right edge");
                expectedLine = 0;
                await AfterPaintAsync(native, () => native.FirstVisibleLine = 0);
                await AfterPaintAsync(native, () => native.SetSelection(0, 0));
                Check(paints.All(valid => valid),
                    "Scrolling back and repainting the original short RTL rows retain their leading edge");
            }
            finally { native.Painted -= Painted; }
        });

        await WithDocumentAsync(true, false, async (editor, native, view, starts) =>
        {
            const double pan = 60;
            var startDistance = RightEdge(view) - native.PointXFromPosition(0);
            var sampleDistance = RightEdge(view) - native.PointXFromPosition(32);
            Check(native.XOffset > pan * scale && sampleDistance > pan * scale,
                "The wide source permits a nonzero RTL leading pan with a visible sample");
            await AfterPaintAsync(native, () => native.XOffset -= checked((int)Math.Round(pan * scale)));
            var expectedDistance = sampleDistance - pan * scale;
            Check(Math.Abs(LeadingOffset(editor) - pan) <= 1 / scale,
                "Physical horizontal scrolling establishes the nonzero leading offset");

            async Task PreserveAsync(Action action, int line, string label)
            {
                var paints = new List<bool>();
                void Painted(Editor sender, PaintedEventArgs args)
                {
                    var physicalX = sender.XOffset;
                    var leading = LeadingOffset(editor);
                    var valid = sender.FirstVisibleLine == line &&
                        Math.Abs(RightEdge(view) - sender.PointXFromPosition(starts[line] + 32) - expectedDistance) <= 1 &&
                        Math.Abs(leading - pan) <= 1 / scale;
                    paints.Add(valid && sender.XOffset == physicalX);
                }
                native.Painted += Painted;
                try
                {
                    await AfterPaintAsync(native, action);
                    Check(paints.Count > 0 && paints.All(valid => valid), label);
                }
                finally { native.Painted -= Painted; }
            }

            await PreserveAsync(() => native.FirstVisibleLine = 78, 78,
                "Revealing the first wider RTL row preserves a nonzero leading pan");
            await PreserveAsync(() => native.FirstVisibleLine = 118, 118,
                "A second wider RTL row preserves the same leading pan");
            await PreserveAsync(() => { editor.Width = 480; editor.UpdateLayout(); }, 118,
                "Narrowing the RTL text surface preserves the leading pan and visible glyph positions");
            await PreserveAsync(() => editor.DisplayLineNumbers = true, 118,
                "Adding the right gutter preserves the leading pan relative to the text surface");

            var beforeQueryX = native.XOffset;
            // Query the distant long row only now. Its returned point and the
            // already-visible row must belong to the same newly grown canvas,
            // immediately, without a future paint repairing either coordinate.
            var distantX = native.PointXFromPosition(starts[160]);
            Check(Math.Abs(RightEdge(view) - distantX - (startDistance - pan * scale)) <= 1 &&
                Math.Abs(RightEdge(view) - native.PointXFromPosition(starts[118] + 32) - expectedDistance) <= 1 &&
                Math.Abs(LeadingOffset(editor) - pan) <= 1 / scale && native.XOffset > beforeQueryX + 1000,
                "An offscreen geometry query grows the RTL canvas without moving visible text or returning an obsolete point");
            await PreserveAsync(() => native.SetSelection(0, 0), 118,
                "The paint following an offscreen geometry query retains the same leading pan");

            var widthBeforeShrink = native.ScrollWidth;
            var shrinkPaints = new List<bool>();
            void ShrunkPainted(Editor sender, PaintedEventArgs args)
            {
                var physicalX = sender.XOffset;
                var leading = LeadingOffset(editor);
                var valid = sender.FirstVisibleLine == 118 &&
                    Math.Abs(RightEdge(view) - sender.PointXFromPosition(starts[118]) -
                        (startDistance - pan * scale)) <= 1 &&
                    Math.Abs(leading - pan) <= 1 / scale;
                shrinkPaints.Add(valid && sender.XOffset == physicalX);
            }
            native.Painted += ShrunkPainted;
            try
            {
                // Font changes invalidate the historical extent. Compare a
                // logical line origin: interior glyph advances must shrink,
                // while the retained leading pan must not change with them.
                await AfterPaintAsync(native, () => editor.FontSize = 18);
                for (var frame = 0; frame < 3 && native.ScrollWidth >= widthBeforeShrink; frame++)
                    await AfterPaintAsync(native, () => native.SetSelection(0, 0));
                Check(shrinkPaints.Count > 0 && shrinkPaints.All(valid => valid) && native.ScrollWidth < widthBeforeShrink,
                    "Shrinking the RTL font publishes a smaller scroll extent without moving the retained leading pan");
            }
            finally { native.Painted -= ShrunkPainted; }
        });

        foreach (var sameValue in new[] { true, false })
        {
            await WithDocumentAsync(true, true, async (editor, native, view, starts) =>
            {
                var requested = sameValue ? native.XOffset : 37;
                var paints = new List<int>();
                void Painted(Editor sender, PaintedEventArgs args) => paints.Add(sender.XOffset);
                native.Painted += Painted;
                try
                {
                    await AfterPaintAsync(native, () =>
                    {
                        native.FirstVisibleLine = 78;
                        // An equal numeric setter is still an explicit physical
                        // request, not permission to replay the old leading pan.
                        native.XOffset = requested;
                    });
                    Check(paints.Count > 0 && paints.All(value => value == requested) &&
                        native.PointXFromPosition(starts[78] + 2) > RightEdge(view) + 1000 && native.XOffset == requested,
                        $"Explicit physical XOffset remains authoritative during RTL canvas growth: sameValue={sameValue}");
                }
                finally { native.Painted -= Painted; }
            });
        }

        await WithDocumentAsync(false, false, async (editor, native, view, starts) =>
        {
            await AfterPaintAsync(native, () => native.XOffset = 37);
            var expectedX = native.PointXFromPosition(32);
            await AfterPaintAsync(native, () => native.FirstVisibleLine = 78);
            Check(native.XOffset == 37 && native.PointXFromPosition(starts[78] + 32) == expectedX,
                "LTR vertical scrolling retains its absolute horizontal offset when a wider row becomes visible");
            native.PointXFromPosition(starts[160]);
            Check(native.XOffset == 37 && native.PointXFromPosition(starts[78] + 32) == expectedX &&
                Math.Abs(LeadingOffset(editor) - 37 / scale) <= 1 / scale,
                "An LTR offscreen geometry query retains absolute horizontal scroll semantics");
        });
    }

    private static async Task CheckGutterPlacementAsync(TextEditorCore core, Editor native, EditorBaseControl view)
    {
        var originalFontSize = core.FontSize;
        var originalNumbers = core.DisplayLineNumbers;
        var reveal = (Border)((Grid)core.Content).Children[1];
        var horizontal = FindElement<ScrollBar>(view, "HorizontalScrollBar");
        var vertical = FindElement<ScrollBar>(view, "VerticalScrollBar");
        var surface = FindElement<Border>(view, "ImageTarget");
        Check(horizontal != null && vertical != null && surface != null,
            "Native gutter fixture locates the real text surface and both scrollbars");
        var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        var text = string.Join('\r', Enumerable.Repeat("F12 אבג " + new string('א', 200), 30));
        void CheckPlacement(bool rtl)
        {
            var margin = native.GetMarginWidthN(0) / scale;
            Check(margin > 0 && reveal.Visibility == Visibility.Visible && Math.Abs(reveal.Width - margin) < 0.01,
                "Visible line numbers and their hover overlay have the same native width");
            Check(core.FlowDirection == FlowDirection.LeftToRight && view.FlowDirection == FlowDirection.LeftToRight,
                "Document direction moves the gutter without mirroring the XAML bitmap");
            Check(Math.Abs((rtl ? horizontal.Margin.Right : horizontal.Margin.Left) - margin) < 0.01 &&
                (rtl ? horizontal.Margin.Left : horizontal.Margin.Right) == 0,
                "The horizontal scrollbar excludes the gutter on the document's leading side");
            var surfaceOrigin = surface.TransformToVisual(core).TransformPoint(new Windows.Foundation.Point());
            var scrollbarOrigin = vertical.TransformToVisual(core).TransformPoint(new Windows.Foundation.Point());
            Check(vertical.Visibility == Visibility.Visible && vertical.ActualWidth > 0 && vertical.Maximum > 0,
                "The fixture exercises a visible, scrollable vertical scrollbar");
            Check(rtl
                    ? Math.Abs(scrollbarOrigin.X + vertical.ActualWidth - surfaceOrigin.X) < 1
                    : Math.Abs(surfaceOrigin.X + surface.ActualWidth - scrollbarOrigin.X) < 1,
                "The vertical scrollbar sits outside the left of RTL text and the right of LTR text");
            var revealOrigin = reveal.TransformToVisual(core).TransformPoint(new Windows.Foundation.Point());
            var expectedEdge = rtl
                ? surfaceOrigin.X + surface.ActualWidth - margin - native.MarginRight / (2 * scale)
                : surfaceOrigin.X + margin + native.MarginLeft / (2 * scale);
            var actualEdge = revealOrigin.X + (rtl ? 0 : reveal.ActualWidth);
            Check(Math.Abs(actualEdge - expectedEdge) < 1,
                "The hover edge stays halfway between line numbers and text, excluding the vertical scrollbar");
            // Exercise the public close-hit APIs, not just derived bounds. The
            // text-rectangle containment check must reject either gutter.
            var surfaceWidth = checked((int)Math.Round(surface.ActualWidth * scale));
            var gutterX = rtl ? surfaceWidth - native.GetMarginWidthN(0) / 2 : native.GetMarginWidthN(0) / 2;
            var y = native.PointYFromPosition(0) + native.TextHeight(0) / 2;
            Check(native.PositionFromPointClose(gutterX, y) == -1 &&
                native.CharPositionFromPointClose(gutterX, y) == -1,
                "Close-hit queries reject the line-number gutter in both LTR and RTL documents");
            // Use the interior of the Latin run: position zero lies at a BiDi
            // run boundary and can have two legitimate visual caret locations.
            // Showing caret zero need not expose the rest of this long run.
            // Scroll its target glyph's right edge into view before hit testing.
            native.GotoPos(2);
            var glyphX = (native.PointXFromPosition(1) + native.PointXFromPosition(2)) / 2;
            var character = native.CharPositionFromPointClose(glyphX, y);
            var position = native.PositionFromPointClose(glyphX, y);
            Check(character == 1 && position is 1 or 2,
                $"Close-hit queries accept the visible Latin glyph: RTL={rtl}, character={character}, position={position}");
        }
        try
        {
            await AfterPaintAsync(native, () =>
            {
                core.DisplayLineNumbers = true;
                core.TextWrapping = TextWrapping.NoWrap;
                core.RightToLeftReadingOrder = false;
                core.SetText(string.Join('\r', Enumerable.Repeat("F12 " + new string('A', 200), 30)));
                native.GotoPos(0);
                core.UpdateLayout();
            });
            // Pure ASCII uses the legacy LTR fast path, without paragraph hit
            // testing's separate range checks masking a text-rectangle defect.
            CheckPlacement(false);
            await AfterPaintAsync(native, () => core.SetText(text));
            // The mixed LTR document has already painted. Reassigning its
            // direction and caret to their current values need not repaint.
            CheckPlacement(false);
            foreach (var rtl in new[] { true, false })
            {
                await AfterPaintAsync(native, () =>
                {
                    core.RightToLeftReadingOrder = rtl;
                    native.GotoPos(0);
                    core.UpdateLayout();
                });
                CheckPlacement(rtl);
                var additional = (int)Math.Round(7 * scale);
                native.SetMarginWidthN(1, additional);
                Check(Math.Abs((rtl ? horizontal.Margin.Right : horizontal.Margin.Left) -
                    (native.GetMarginWidthN(0) + additional) / scale) < 0.01,
                    "Scrollbar placement follows every native gutter column synchronously");
                native.SetMarginWidthN(1, 0);
                if (rtl)
                {
                    var before = native.GetMarginWidthN(0);
                    await AfterPaintAsync(native, () => core.FontSize = originalFontSize * 1.25);
                    Check(native.GetMarginWidthN(0) > before, "Zoom increases the right-side native line-number width");
                    CheckPlacement(true);
                }
            }
            await AfterPaintAsync(native, () => core.DisplayLineNumbers = false);
            Check(native.GetMarginWidthN(0) == 0 && reveal.Visibility == Visibility.Collapsed &&
                horizontal.Margin.Left == 0 && horizontal.Margin.Right == 0,
                "Hiding line numbers removes both gutter insets and the hover overlay");
            Check(text == core.GetText(), "Gutter placement and zoom preserve the entire logical document");
        }
        finally
        {
            await AfterPaintAsync(native, () =>
            {
                native.SetMarginWidthN(1, 0);
                core.DisplayLineNumbers = originalNumbers;
                core.FontSize = originalFontSize;
                core.RightToLeftReadingOrder = false;
                core.SetText(string.Empty);
                core.UpdateLayout();
            });
        }
    }

    private static async Task CheckGutterPreferredColumnAsync(TextEditorCore core, Editor native)
    {
        var originalNumbers = core.DisplayLineNumbers;
        var originalWrapping = core.TextWrapping;
        try
        {
            foreach (var rtl in new[] { true, false })
            foreach (var wide in new[] { false, true })
            foreach (var wrapping in new[] { TextWrapping.NoWrap, TextWrapping.Wrap })
            foreach (var change in new[] { "show", "hide", "right-gap" })
            {
                // Wrap-mode coverage uses one short physical row per line;
                // no expected column depends on a changing wrap boundary.
                if (wrapping == TextWrapping.Wrap && (!rtl || wide || change == "right-gap")) continue;
                var line = string.Concat(Enumerable.Repeat("אבגדהוזחט", wide ? 80 : 1));
                var text = line + '\r' + line;
                core.TextWrapping = wrapping;
                core.DisplayLineNumbers = change != "show";
                await LoadAsync(core, native, text, rtl);
                if (wrapping == TextWrapping.Wrap)
                    Check(native.WrapCount(0) == 1 && native.WrapCount(1) == 1,
                        "The wrap-mode gutter fixture retains one display row per physical line");
                native.GotoPos(6); // Three complete Hebrew characters, on row one.
                native.ChooseCaretX();
                var secondRow = Encoding.UTF8.GetByteCount(line) + 1;
                var context = $"rtl={rtl}, wide={wide}, wrap={wrapping}, change={change}";

                await AfterPaintAsync(native, () =>
                {
                    if (change == "right-gap") native.MarginRight += 17;
                    else core.DisplayLineNumbers = change == "show";
                    // Do not inspect glyph geometry or re-choose X after changing
                    // the gutter: the next real arrow must preserve the column.
                    native.LineDown();
                    Check(native.CurrentPos == secondRow + 6 && native.Anchor == native.CurrentPos,
                        $"Down preserves the chosen Hebrew column after {context}: expected={secondRow + 6}, actual={native.CurrentPos}");
                    native.LineUp();
                    Check(native.CurrentPos == 6 && native.Anchor == 6,
                        $"Up returns to the original column after {context}: expected=6, actual={native.CurrentPos}");
                });
                Check(core.GetText() == text, "Gutter changes and vertical arrows preserve document bytes");
            }

            core.DisplayLineNumbers = false;
            core.TextWrapping = TextWrapping.NoWrap;
            const string unevenRows = "אבגדהוזחט\rא\rאבגדהוזחט";
            await LoadAsync(core, native, unevenRows, true);
            native.GotoPos(6);
            native.ChooseCaretX();
            native.LineDown();
            Check(native.CurrentPos == 21, "The short intermediate row clamps the chosen column to its end");
            await AfterPaintAsync(native, () =>
            {
                core.DisplayLineNumbers = true;
                // Moving the gutter must retain the original chosen column,
                // rather than replace it with the short row's clamped caret.
                native.LineDown();
                Check(native.CurrentPos == 28, "The next long row recovers the chosen column after the gutter moves");
                native.LineUp();
                Check(native.CurrentPos == 21, "Returning through the short row retains its logical end");
                native.LineUp();
                Check(native.CurrentPos == 6, "The original long row recovers the chosen column after the roundtrip");
            });
            Check(core.GetText() == unevenRows, "Preserving the preferred column does not edit uneven rows");
        }
        finally
        {
            await AfterPaintAsync(native, () =>
            {
                core.DisplayLineNumbers = originalNumbers;
                core.TextWrapping = originalWrapping;
                core.RightToLeftReadingOrder = false;
                core.SetText(string.Empty);
            });
        }
    }

    private static async Task CheckPreferredColumnCanvasGrowthAsync()
    {
        var previousContent = Window.Current.Content;
        foreach (var rtl in new[] { false, true })
        {
            using var core = new TextEditorCore
            {
                Width = 620, Height = 420, FontSize = 24,
                FontFamily = new FontFamily("Consolas"),
                DisplayLineNumbers = false, TextWrapping = TextWrapping.NoWrap,
                RightToLeftReadingOrder = rtl,
                HorizontalAlignment = HorizontalAlignment.Left,
                VerticalAlignment = VerticalAlignment.Top
            };
            var glyph = rtl ? 'א' : 'a';
            var rows = Enumerable.Repeat(new string(glyph, 9), 100).ToArray();
            rows[80] = new string(glyph, 600);
            var starts = new long[rows.Length];
            for (var row = 1; row < rows.Length; row++)
                starts[row] = starts[row - 1] + Encoding.UTF8.GetByteCount(rows[row - 1]) + 1;
            var text = string.Join('\r', rows);
            await core.LoadTextAsync(text);
            var view = (EditorBaseControl)((Grid)core.Content).Children[0];
            var native = view.Editor;
            try
            {
                await AfterPaintAsync(native, () => Window.Current.Content = core);
                Check(native.FirstVisibleLine == 0 && native.LinesOnScreen < 70 && native.XOffset == 0,
                    "Preferred-column fixture starts before the distant wider row has been measured");
                var column = Encoding.UTF8.GetByteCount(new string(glyph, 3));
                native.SetEmptySelection(column);
                native.ChooseCaretX();
                var originalWidth = native.ScrollWidth;

                // Do not query any distant geometry or yield for a paint.
                // The arrow/scroll path itself must discover the wider row
                // and preserve the originally chosen three-character column.
                for (var row = 1; row <= 82; row++)
                {
                    native.LineDown();
                    Check(native.CurrentPos == starts[row] + column && native.Anchor == native.CurrentPos,
                        $"Down preserves the chosen column through cold canvas growth: rtl={rtl}, row={row}");
                }
                if (rtl)
                    Check(native.ScrollWidth > originalWidth + 1000 && native.XOffset > 1000,
                        "The RTL traversal actually grows and reprojects the horizontal canvas");
                for (var row = 81; row >= 0; row--)
                {
                    native.LineUp();
                    Check(native.CurrentPos == starts[row] + column && native.Anchor == native.CurrentPos,
                        $"Up preserves the original column after canvas growth: rtl={rtl}, row={row}");
                }
                Equal(text, core.GetText(), "Preferred-column traversal preserves all document bytes");
            }
            finally { Window.Current.Content = previousContent; }
        }
    }

    private static async Task CheckGutterWrapTransitionAsync(TextEditorCore core, Editor native)
    {
        var originalNumbers = core.DisplayLineNumbers;
        var originalWrapping = core.TextWrapping;
        const string line = "אבגדהוזחט";
        const string text = line + "\r" + line;
        try
        {
            core.DisplayLineNumbers = false;
            core.TextWrapping = TextWrapping.Wrap;
            await LoadAsync(core, native, text, rtl: true);
            Check(native.WrapCount(0) == 1 && native.WrapCount(1) == 1,
                "The wrap-transition rectangle starts with two single-row Hebrew lines");
            native.EmptyUndoBuffer();
            var secondRow = Encoding.UTF8.GetByteCount(line) + 1;
            await AfterPaintAsync(native, () =>
            {
                // The old finite wrapped layout is still cached here. Do not
                // let a paint/geometry query repair its width before the edit.
                core.DisplayLineNumbers = true;
                core.TextWrapping = TextWrapping.NoWrap;
                native.RectangularSelectionAnchor = 2;
                native.RectangularSelectionCaret = secondRow + 6;
                var ranges = Enumerable.Range(0, checked((int)native.Selections))
                    .Select(index => (native.GetSelectionNStart(index), native.GetSelectionNEnd(index)))
                    .OrderBy(range => range.Item1).ToArray();
                Check(ranges.SequenceEqual(new[] { (2L, 6L), (secondRow + 2L, secondRow + 6L) }),
                    "Gutter plus wrap-off selects only bet and gimel on both rows");
                core.TypeText("X");
                Equal("אXדהוזחט\rאXדהוזחט", core.GetText(),
                    "Immediate rectangle replacement preserves both unselected Hebrew prefixes");
                core.Undo();
                Equal(text, core.GetText(), "Wrap-transition rectangle replacement undoes exactly once");
                Check(!core.CanUndo, "Wrap-transition rectangle replacement has one undo boundary");
            });
        }
        finally
        {
            await AfterPaintAsync(native, () =>
            {
                native.SelectionMode = SelectionMode.Stream;
                native.MoveExtendsSelection = false;
                core.DisplayLineNumbers = originalNumbers;
                core.TextWrapping = originalWrapping;
                core.RightToLeftReadingOrder = false;
                core.SetText(string.Empty);
            });
        }
    }

    private static async Task CheckLeadingOriginsAsync(TextEditorCore core, Editor native)
    {
        var representation = native.GetRepresentation("\u0001");
        try
        {
            native.SetRepresentation("\u0001", "CONTROL");
            foreach (var wrapping in new[] { TextWrapping.NoWrap, TextWrapping.Wrap })
            {
                core.TextWrapping = wrapping;
                foreach (var prefix in new[] { "ASCII", "中文", "\tTab", "\u0001Control", "e\u0301", "👩‍💻", "ffi office" })
                {
                    var line = prefix + new string('x', 200);
                    await LoadAsync(core, native, line + '\r' + line, rtl: false);
                    Check(wrapping == TextWrapping.NoWrap || native.WrapCount(0) > 1,
                        "The leading-origin fixture really wraps at the rendered width");
                    // Every LTR-only physical line begins at the configured text
                    // margin, independent of how Windows shapes its first cluster.
                    long origin = native.MarginLeft;
                    for (var margin = 0; margin < native.Margins; margin++) origin += native.GetMarginWidthN(margin);
                    foreach (var position in new[] { 0L, native.PositionFromLine(1) })
                        Check(native.PointXFromPosition(position) == origin && native.XOffset == 0,
                            $"LTR {wrapping} origin at byte {position} matches the text margin for {prefix}");
                    Check(native.PointXFromPosition(Encoding.UTF8.GetByteCount(prefix)) > origin,
                        "Interior positions retain the first cluster's actual advance rather than using the origin shortcut");
                    Check(native.PointYFromPosition(0) == 0 && native.PointYFromPosition(native.PositionFromLine(1)) > 0,
                        "Physical line origins keep distinct display rows");
                }
            }
        }
        finally
        {
            core.TextWrapping = TextWrapping.NoWrap;
            if (string.IsNullOrEmpty(representation)) native.ClearRepresentation("\u0001");
            else native.SetRepresentation("\u0001", representation);
        }
    }

    private static async Task CheckDirectionAsync(TextEditorCore core, Editor native, Grid host, EditorBaseControl view)
    {
        Progress("direction: load Hebrew LTR");
        await LoadAsync(core, native, "אבג", false);
        Progress("direction: edit Hebrew LTR");
        var leftAligned = native.PointXFromPosition(0);
        native.EmptyUndoBuffer();
        core.MarkSaved();
        core.SetTextSelectionPosition(3, 3);
        core.TypeText("!");
        var text = core.GetText();
        var revision = native.DocumentRevision;
        var caret = native.CurrentPos;
        Progress("direction: switch to RTL");
        await AfterPaintAsync(native, () => core.RightToLeftReadingOrder = true);
        Check(native.Bidirectional == Bidirectional.R2l, "RTL property reaches native paragraph direction");
        Check(native.PointXFromPosition(0) > leftAligned + 100,
            "RTL paragraph moves to the viewport's right edge");
        Check(core.GetText() == text && native.DocumentRevision == revision && native.CurrentPos == caret && core.CanUndo,
            "Changing direction preserves bytes, revision, selection and undo");
        Progress("direction: undo");
        core.Undo();
        Equal("אבג", core.GetText(), "Direction change does not consume an undo action");
        Check(!core.IsDocumentModified, "Undo after direction change returns to saved state");

        Progress("direction: inherited RTL UI");
        host.FlowDirection = FlowDirection.RightToLeft;
        host.UpdateLayout();
        Check(core.FlowDirection == FlowDirection.LeftToRight && view.FlowDirection == FlowDirection.LeftToRight,
            "RTL app chrome never mirrors the native canvas or glyphs");
        host.FlowDirection = FlowDirection.LeftToRight;
        Progress("direction: Latin under RTL");
        await LoadAsync(core, native, "ABC", true);
        Check(native.PointXFromPosition(0) < native.PointXFromPosition(1) &&
            native.PointXFromPosition(1) < native.PointXFromPosition(2),
            "Latin letters retain LTR order inside an RTL paragraph");

        Progress("direction: empty RTL document");
        await LoadAsync(core, native, string.Empty, true);
        var emptyRight = native.PointXFromPosition(0);
        await AfterPaintAsync(native, () => core.RightToLeftReadingOrder = false);
        Check(emptyRight > native.PointXFromPosition(0) + 100,
            "An empty RTL document has a right-aligned insertion point");

        foreach (var policy in new[] { (CaretPolicy)0, CaretPolicy.Slop | CaretPolicy.Strict | CaretPolicy.Even })
        {
            native.SetXCaretPolicy(policy, 20);
            foreach (var document in new[] { string.Empty, "אבג", new string('א', 80) })
            {
                await LoadAsync(core, native, document, true);
                native.ScrollCaret();
                var width = native.ScrollWidth;
                var offset = native.XOffset;
                var x = native.PointXFromPosition(0);
                for (var count = 0; count < 10; count++) native.ScrollCaret();
                Check(native.ScrollWidth == width && native.XOffset == offset && native.PointXFromPosition(0) == x,
                    $"Repeated RTL caret visibility is stable: policy={policy}, chars={document.Length}, width={width}->{native.ScrollWidth}, offset={offset}->{native.XOffset}, x={x}->{native.PointXFromPosition(0)}");
            }
        }
        native.SetXCaretPolicy((CaretPolicy)0, 0);

        Progress("direction: newline shrinks RTL canvas");
        await LoadAsync(core, native, new string('א', 160), true);
        native.XOffset = 0;
        var wideRight = native.PointXFromPosition(0);
        var fullSpan = Math.Abs(wideRight - native.PointXFromPosition(native.Length));
        native.SetEmptySelection(160); // Eighty two-byte Hebrew characters.
        core.TypeText("\r");
        native.XOffset = 0;
        Equal(new string('א', 80) + "\r" + new string('א', 80), core.GetText(),
            "A newline splits the wide physical paragraph into equal halves");
        var immediateRight = native.PointXFromPosition(0);
        Check(Math.Abs(wideRight - immediateRight - fullSpan / 2.0) < 2,
            $"Newline invalidates the old RTL extent before paint: old={wideRight}, new={immediateRight}, span={fullSpan}");
        await AfterPaintAsync(native, () => { native.SetEmptySelection(0); native.XOffset = 0; });
        Check(Math.Abs(native.PointXFromPosition(0) - immediateRight) < 2,
            "First paint retains the new shared extent after splitting a wide RTL paragraph");
        native.ScrollCaret();
        var splitWidth = native.ScrollWidth;
        var splitOffset = native.XOffset;
        for (var repeat = 0; repeat < 10; repeat++) native.ScrollCaret();
        Check(native.ScrollWidth == splitWidth && native.XOffset == splitOffset,
            "Repeated caret visibility preserves the shrunken RTL canvas");
    }

    private static async Task CheckHebrewEditingAsync(TextEditorCore core, Editor native)
    {
        await LoadAsync(core, native, "אבג", true);
        var previousX = native.PointXFromPosition(0);
        foreach (var position in new long[] { 2, 4, 6 })
        {
            native.CharLeft();
            Check(native.CurrentPos == position, "Left visits the next Hebrew character in visual order");
            var x = native.PointXFromPosition(native.CurrentPos);
            Check(x < previousX, "Left moves the visible caret left");
            previousX = x;
        }
        foreach (var position in new long[] { 4, 2, 0 })
        {
            native.CharRight();
            Check(native.CurrentPos == position, "Right reverses Hebrew visual navigation");
            var x = native.PointXFromPosition(native.CurrentPos);
            Check(x > previousX, "Right moves the visible caret right");
            previousX = x;
        }
        native.CharLeftExtend();
        native.CharLeftExtend();
        Equal("אב", core.GetSelectedText(), "Shift+Left selects logical Hebrew prefix");
        core.TypeText("X");
        Equal("Xג", core.GetText(), "RTL visual selection replacement edits the selected bytes");
        core.Undo();
        Equal("אבג", core.GetText(), "RTL replacement undo restores the original text");

        native.SetEmptySelection(4);
        native.DeleteBack();
        Equal("אג", core.GetText(), "Backspace deletes the preceding logical character");
        core.Undo();
        native.SetEmptySelection(2);
        native.Clear();
        Equal("אג", core.GetText(), "Delete removes the following logical character");
        core.Undo();
        Equal("אבג", core.GetText(), "Both deletion paths retain normal undo semantics");

        await LoadAsync(core, native, "א\u05b7ב😀ג", true);
        native.SetEmptySelection(0);
        // The Hebrew base plus vowel is a single shaped cluster. The following
        // emoji occupies four UTF-8 bytes and must not be entered halfway.
        foreach (var position in new long[] { 4, 6, 10, 12 })
        {
            native.CharLeft();
            Check(native.CurrentPos == position, "Visual movement respects combining and emoji clusters");
        }
    }

    private static async Task CheckLineNavigationAsync(TextEditorCore core, Editor native)
    {
        await LoadAsync(core, native, "אבג דהו", true);
        native.SetEmptySelection(4);
        native.Home();
        Check(native.CurrentPos == 0, "RTL Home reaches the paragraph's logical start");
        native.LineEnd();
        Check(native.CurrentPos == 13, "RTL End reaches the paragraph's logical end");
        native.WordRight();
        Check(native.CurrentPos == 7, "Ctrl+Right reaches the preceding Hebrew word start");
        native.WordRight();
        Check(native.CurrentPos == 0, "Ctrl+Right reaches the first Hebrew word start");
        native.WordLeft();
        Check(native.CurrentPos == 7, "Ctrl+Left reaches the following Hebrew word start");
        native.WordLeft();
        Check(native.CurrentPos == 13, "Ctrl+Left reaches the paragraph end");
        native.SetSelection(4, 0);
        native.CharLeft();
        Check(native.CurrentPos == 4 && native.Anchor == 4, "Left collapses RTL selection to its visual left edge");
        native.SetSelection(4, 0);
        native.CharRight();
        Check(native.CurrentPos == 0 && native.Anchor == 0, "Right collapses RTL selection to its visual right edge");

        await LoadAsync(core, native, "אבג\rאבג\rאבג", true);
        native.SetEmptySelection(2);
        native.ChooseCaretX();
        native.LineDown();
        Check(native.CurrentPos == 9, "Down keeps the same visible Hebrew column on the next line");
        native.LineDown();
        Check(native.CurrentPos == 16, "Repeated Down keeps the preferred RTL column");
        native.LineUp();
        Check(native.CurrentPos == 9, "Up returns to the same RTL column");
        native.LineUp();
        Check(native.CurrentPos == 2, "Up/Down roundtrip returns to the original byte boundary");

        try
        {
            foreach (var rtl in new[] { false, true })
            {
                await AfterPaintAsync(native, () =>
                {
                    core.Width = 220;
                    core.TextWrapping = TextWrapping.Wrap;
                    core.RightToLeftReadingOrder = rtl;
                    core.SetText(new string('א', 64));
                    core.UpdateLayout();
                });
                var firstY = native.PointYFromPosition(0);
                var rowStart = Enumerable.Range(1, 63).Select(index => (long)index * 2)
                    .First(position => native.PointYFromPosition(position) > firstY);
                var rowY = native.PointYFromPosition(rowStart);
                var rowEnd = Enumerable.Range((int)rowStart / 2 + 1, 64 - (int)rowStart / 2 - 1)
                    .Select(index => (long)index * 2).First(position => native.PointYFromPosition(position) > rowY);
                Check(rowEnd - rowStart >= 6, "Wrapped navigation fixture has an interior second-row caret");
                var interior = rowStart + 2;
                native.SetEmptySelection(interior);
                native.Home();
                Check(native.CurrentPos == 0, "Plain Home crosses wrapped rows to paragraph start");
                native.SetEmptySelection(interior);
                native.LineEnd();
                Check(native.CurrentPos == native.Length, "Plain End crosses wrapped rows to paragraph end");

                native.SetEmptySelection(interior);
                native.HomeDisplay();
                var displayHome = native.CurrentPos;
                Check(displayHome == (rtl ? rowStart : rowEnd), "Display Home uses the leading visual edge of the current row");
                native.SetEmptySelection(interior);
                native.LineEndDisplay();
                var displayEnd = native.CurrentPos;
                Check(displayEnd == (rtl ? rowEnd : rowStart), "Display End uses the trailing visual edge of the current row");
                native.SetEmptySelection(interior);
                native.HomeWrap();
                Check(native.CurrentPos == displayHome, "First HomeWrap reaches the current display edge");
                native.HomeWrap();
                Check(native.CurrentPos == 0, "Second HomeWrap reaches paragraph start");
                native.SetEmptySelection(interior);
                native.LineEndWrap();
                Check(native.CurrentPos == displayEnd, "First EndWrap reaches the current display edge");
                native.LineEndWrap();
                Check(native.CurrentPos == native.Length, "Second EndWrap reaches paragraph end");

                // A trailing wrap-boundary caret belongs to the preceding row.
                // With one visible row, losing affinity would scroll to row 2.
                var originalHeight = core.Height;
                await AfterPaintAsync(native, () =>
                {
                    core.Height = native.TextHeight(0) / DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
                    core.UpdateLayout();
                });
                // The scrollbar may appear at this height, changing the wrap
                // width. Observe the completed layout before moving the caret.
                var resizedFirstY = native.PointYFromPosition(0);
                var resizedRowStart = Enumerable.Range(1, 63).Select(index => (long)index * 2)
                    .First(position => native.PointYFromPosition(position) > resizedFirstY);
                Check(native.WrapCount(0) >= 3, "Wrap-affinity fixture has enough rows to scroll below row zero");
                await AfterPaintAsync(native, () =>
                {
                    native.SetEmptySelection(2);
                    if (rtl) native.LineEndDisplay(); else native.HomeDisplay();
                });
                Check(native.CurrentPos == resizedRowStart,
                    $"Display edge retains the first row's trailing boundary: rtl={rtl}, expected={resizedRowStart}, actual={native.CurrentPos}");
                // Start one row below the caret. Treating its byte offset as
                // the next row would incorrectly leave this viewport at 1.
                native.FirstVisibleLine = 1;
                for (var repeat = 0; repeat < 10; repeat++) native.ScrollCaret();
                Check(native.FirstVisibleLine == 0, "ScrollCaret retains trailing affinity at a wrapped row boundary");
                await AfterPaintAsync(native, () => { core.Height = originalHeight; core.UpdateLayout(); });
            }
        }
        finally
        {
            await AfterPaintAsync(native, () =>
            {
                core.Width = 620;
                core.TextWrapping = TextWrapping.NoWrap;
                core.UpdateLayout();
            });
        }
    }

    private static async Task CheckUndoCaretAsync(TextEditorCore core, Editor native, EditorBaseControl view)
    {
        var provider = (ITextProvider2)FrameworkElementAutomationPeer.CreatePeerForElement(view)
            .GetPattern(PatternInterface.Text);

        (long Position, long Anchor, double X, double Y) Caret()
        {
            var bounds = Rectangles(provider.GetCaretRange(out _));
            Check(bounds.Length == 4, "The history fixture exposes one visible caret");
            return (native.CurrentPos, native.Anchor, bounds[0], bounds[1]);
        }

        void ExpectCaret((long Position, long Anchor, double X, double Y) expected, string operation)
        {
            var actual = Caret();
            Check(actual.Position == expected.Position && actual.Anchor == expected.Anchor &&
                Math.Abs(actual.X - expected.X) < 1 && Math.Abs(actual.Y - expected.Y) < 1,
                $"{operation}: expected={expected}, actual={actual}");
        }

        void ExpectTrailingGlyph(int character, bool rtl, string operation)
        {
            var glyph = ((ITextProvider)provider).DocumentRange.Clone();
            glyph.MoveEndpointByRange(TextPatternRangeEndpoint.End, glyph, TextPatternRangeEndpoint.Start);
            glyph.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, TextUnit.Character, character);
            glyph.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 1);
            var bounds = Rectangles(glyph);
            var actual = Caret();
            Check(bounds.Length == 4 && Math.Abs(actual.X - (rtl ? bounds[0] : bounds[0] + bounds[2])) < 1 &&
                Math.Abs(actual.Y - bounds[1]) < 1, operation);
        }

        foreach (var rtl in new[] { false, true })
        {
            var source = rtl ? "אבג abc" : "abc אבג";
            var inserted = rtl ? "דה" : "XY";
            var start = Encoding.UTF8.GetByteCount(source.AsSpan(0, 4));
            var end = start + Encoding.UTF8.GetByteCount(inserted);
            var edited = source.Insert(4, inserted);
            await LoadAsync(core, native, source, rtl);
            native.SetEmptySelection(start);
            var before = Caret();
            core.TypeText(inserted);
            var after = Caret();
            Check(after.Position == end && after.Anchor == end, "Typing ends after the inserted characters");
            ExpectTrailingGlyph(5, rtl, "Typing stays at the inserted glyph's trailing edge");
            core.Undo();
            Equal(source, core.GetText(), "Undo removes the inserted text");
            ExpectCaret(before, "Undo insertion retains the original leading home");
            core.Redo();
            Equal(edited, core.GetText(), "Redo restores the inserted text");
            ExpectCaret(after, "Redo insertion retains the typed trailing home");
            foreach (var left in new[] { true, false })
            {
                if (left) native.CharLeft(); else native.CharRight();
                var moved = Caret();
                core.Undo();
                core.Redo();
                ExpectCaret(after, "Redo restores the same home after navigation");
                if (left) native.CharLeft(); else native.CharRight();
                ExpectCaret(moved, "The next arrow follows the same visual path after redo");
                core.Undo();
                core.Redo();
            }

            // Undo of deletion also restores an insertion tail. Its redo must
            // keep deletion's leading home, not inherit that restored tail.
            await LoadAsync(core, native, edited, rtl);
            native.SetSel(start, end);
            native.Clear();
            var deleted = Caret();
            core.Undo();
            Equal(edited, core.GetText(), "Undo deletion restores the selected text");
            ExpectCaret(after, "Undo deletion restores the trailing home");
            core.Redo();
            Equal(source, core.GetText(), "Redo deletion removes only the selected text");
            ExpectCaret(deleted, "Redo deletion restores the leading home");

            // A group can end with a non-text container in either direction.
            // The restore position still belongs to the last actual text step.
            var original = source.Insert(4, rtl ? "וז" : "QR");
            await LoadAsync(core, native, original, rtl);
            native.SetSel(start, end);
            native.BeginUndoAction();
            try
            {
                native.AddUndoAction(101, (UndoFlags)0);
                core.TypeText(inserted);
                native.AddUndoAction(102, (UndoFlags)0);
            }
            finally { native.EndUndoAction(); }
            ExpectCaret(after, "Grouped replacement retains the inserted tail");
            core.Undo();
            Equal(original, core.GetText(), "Grouped undo restores the replaced text");
            Check(native.CurrentPos == end && native.Anchor == end && !core.CanUndo,
                "Grouped undo restores the entire replacement in one action");
            ExpectTrailingGlyph(5, rtl, "A final undo container retains the restored glyph's trailing edge");
            core.Redo();
            Equal(edited, core.GetText(), "Grouped redo restores the replacement");
            ExpectCaret(after, "A final redo container retains the inserted trailing home");

            await LoadAsync(core, native, edited, rtl);
            native.SetEmptySelection(start);
            native.Clear();
            native.Clear();
            Equal(source, core.GetText(), "Two adjacent deletes remove the fixture characters");
            core.Undo();
            Equal(edited, core.GetText(), "One undo restores both coalesced deletions");
            Check(!core.CanUndo, "Adjacent deletions coalesce into one undo action");
            ExpectCaret(after, "Coalesced deletion undo uses the whole restored interval's tail");

            native.EmptyUndoBuffer();
            var unchanged = Caret();
            native.AddUndoAction(103, (UndoFlags)0);
            core.Undo();
            ExpectCaret(unchanged, "Container-only undo does not reposition the caret");
            core.Redo();
            ExpectCaret(unchanged, "Container-only redo does not reposition the caret");
        }
    }

    private static async Task CheckPageNavigationAsync(TextEditorCore core, Editor native, EditorBaseControl view)
    {
        var originalWidth = core.Width;
        var originalHeight = core.Height;
        var pattern = (ITextProvider2)FrameworkElementAutomationPeer.CreatePeerForElement(view)
            .GetPattern(PatternInterface.Text);
        try
        {
            foreach (var rtl in new[] { false, true })
            {
                await AfterPaintAsync(native, () =>
                {
                    core.Width = 220;
                    core.Height = 160;
                    core.TextWrapping = TextWrapping.Wrap;
                    core.RightToLeftReadingOrder = rtl;
                    core.SetText(new string('א', 512));
                    native.SetEmptySelection(0);
                    native.FirstVisibleLine = 0;
                    core.UpdateLayout();
                });
                // Observe row boundaries through the native position API rather
                // than assuming a glyph width, DPI or number of characters/row.
                var rowStarts = Enumerable.Range(0, 512).Select(index => (long)index * 2)
                    .GroupBy(position => native.PointYFromPosition(position))
                    .Select(row => row.First()).ToArray();
                var pageRows = checked((int)Math.Max(1, native.LinesOnScreen - 1));
                Check(native.LinesOnScreen >= 3 && rowStarts.Length > pageRows * 2 + 4,
                    "Page fixture has multiple visible rows and does not reach the document edges");
                foreach (var down in new[] { true, false })
                {
                    var sourceRow = down ? 1 : pageRows + 1;
                    var targetRow = down ? sourceRow + pageRows : sourceRow - pageRows;
                    native.SetEmptySelection(rowStarts[sourceRow] + 2);
                    if (rtl) native.LineEndDisplay(); else native.HomeDisplay();
                    Check(native.CurrentPos == rowStarts[sourceRow + 1],
                        "Paging starts at the preceding row's trailing wrap boundary");
                    native.FirstVisibleLine = sourceRow - 1;
                    var before = Rectangles(pattern.GetCaretRange(out _));
                    Check(before.Length == 4, "The trailing caret is visible before paging");
                    if (down) native.PageDown(); else native.PageUp();
                    Check(native.CurrentPos == rowStarts[targetRow + 1] && native.Anchor == native.CurrentPos,
                        $"Paging preserves the trailing display edge: rtl={rtl}, down={down}, " +
                        $"expected={rowStarts[targetRow + 1]}, actual={native.CurrentPos}");
                    var after = Rectangles(pattern.GetCaretRange(out _));
                    Check(after.Length == 4 && Math.Abs(after[1] - before[1]) < 2,
                        "A full page move preserves the caret's visible row instead of moving one row lower");
                }
            }
        }
        finally
        {
            await AfterPaintAsync(native, () =>
            {
                core.Width = originalWidth;
                core.Height = originalHeight;
                core.TextWrapping = TextWrapping.NoWrap;
                core.UpdateLayout();
            });
        }
    }

    private static async Task CheckMixedEditingAsync(TextEditorCore core, Editor native)
    {
        const string text = "ABC אבג 123 DEF";
        foreach (var rtl in new[] { false, true })
        {
            await LoadAsync(core, native, text, rtl);
            // The interior bet/gimel boundary is unambiguous in either base
            // direction. Sample inside the first half of bet, avoiding both
            // directional boundaries and integer rounding at the midpoint.
            var bet = native.PointXFromPosition(6);
            var gimel = native.PointXFromPosition(8);
            Check(bet > gimel + 2, "Hebrew interior geometry advances right to left");
            var hit = native.PositionFromPointClose((3 * bet + gimel) / 4,
                native.PointYFromPosition(6) + native.TextHeight(0) / 2);
            Check(hit == 6, "Clicking bet's leading half chooses its exact logical position");
            native.SetEmptySelection(hit);
            core.TypeText("|");
            Equal("ABC א|בג 123 DEF", core.GetText(), "Mixed-direction click insertion has fixed logical result");
            core.Undo();
            Equal(text, core.GetText(), "Mixed click insertion undo");

            var before = new long[] { 0, 1, 2, 4, 6, 8, 11, 12, 13, 15, 16 }
                .Select(native.PointXFromPosition).ToArray();
            await AfterPaintAsync(native, () =>
            {
                native.StyleSetFore(1, 0x0000ff);
                native.StyleSetFore(2, 0xff0000);
                native.StartStyling(0, 0);
                for (long position = 0; position < native.Length; position++)
                    native.SetStyling(1, (int)(position % 2) + 1);
            });
            var after = new long[] { 0, 1, 2, 4, 6, 8, 11, 12, 13, 15, 16 }
                .Select(native.PointXFromPosition).ToArray();
            Check(before.SequenceEqual(after), "Color-only lexer boundaries never change paragraph geometry");
            Equal(text, core.GetText(), "Styling never changes logical bytes");
        }
    }

    private static async Task CheckStreamSelectionEdgesAsync(TextEditorCore core, Editor native,
        EditorBaseControl view)
    {
        const string text = "abc אבג def";
        var provider = (ITextProvider2)FrameworkElementAutomationPeer.CreatePeerForElement(view)
            .GetPattern(PatternInterface.Text);
        foreach (var rtl in new[] { false, true })
        foreach (var reverse in new[] { false, true })
        foreach (var left in new[] { true, false })
        {
            var context = $"rtl={rtl}, reverse={reverse}, left={left}";
            await LoadAsync(core, native, text, rtl);
            native.MoveExtendsSelection = false;
            native.SetSelection(reverse ? 4 : 12, reverse ? 12 : 4);

            // Logical endpoints do not necessarily bound the selected ink.
            // UIA supplies the complete selected range, including bidi islands,
            // independently of the keyboard navigation and caret affinity.
            var selection = provider.DocumentRange.Clone();
            selection.MoveEndpointByRange(TextPatternRangeEndpoint.End, selection, TextPatternRangeEndpoint.Start);
            Check(selection.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, TextUnit.Character, 4) == 4 &&
                selection.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 5) == 5,
                "Stream-edge fixture selects five logical characters");
            Equal("אבג d", selection.GetText(-1), "Stream-edge fixture selects the intended mixed range");
            var boxes = Rectangles(selection);
            CheckRectangles(boxes, false);
            var leftEdge = Enumerable.Range(0, boxes.Length / 4).Min(index => boxes[index * 4]);
            var rightEdge = Enumerable.Range(0, boxes.Length / 4)
                .Max(index => boxes[index * 4] + boxes[index * 4 + 2]);

            if (left) native.CharLeft(); else native.CharRight();
            var collapsed = Rectangles(provider.GetCaretRange(out _));
            Check(native.CurrentPos == native.Anchor && native.Selections == 1 &&
                collapsed.Length == 4 && Math.Abs(collapsed[0] - (left ? leftEdge : rightEdge)) <= 2,
                $"Stream selection collapses to its outer visible edge: {context}");

            // Equivalent logical bidi boundaries are acceptable, but the saved
            // affinity must make the next opposite arrow enter the selection.
            if (left) native.CharRight(); else native.CharLeft();
            var inward = Rectangles(provider.GetCaretRange(out _));
            Check(inward.Length == 4 && (left ? inward[0] > collapsed[0] + 2 : inward[0] < collapsed[0] - 2) &&
                inward[0] >= leftEdge - 2 && inward[0] <= rightEdge + 2,
                $"The next arrow moves inward from the collapsed visual edge: {context}");
            Equal(text, core.GetText(), "Stream selection navigation preserves logical text");
        }
    }

    private static async Task CheckRectanglePasteGrowthAsync(TextEditorCore core, Editor native,
        EditorBaseControl view)
    {
        foreach (var rtl in new[] { false, true })
        foreach (var growingRow in new[] { 0, 1 })
        {
            var sourceLine = rtl ? "אבג" : "abc";
            var source = string.Join('\r', Enumerable.Repeat(sourceLine, 3));
            var inserts = new[] { "X", "Y", "Z" };
            inserts[growingRow] = new string(rtl ? 'א' : 'a', 1000);
            var paste = string.Join('\r', inserts);
            var expected = string.Join('\r', inserts.Select(insert => insert + sourceLine));
            await LoadAsync(core, native, source, rtl);
            native.MoveExtendsSelection = false;
            native.EmptyUndoBuffer();

            // A rectangular paste samples one visual column before editing.
            // Recomputing later targets against the expanded RTL canvas would
            // add padding or insert at the opposite end of those source rows.
            native.ReplaceRectangular(Encoding.UTF8.GetByteCount(paste), paste);
            Check(core.GetText() == expected,
                $"Rectangular paste retains every original insertion column: rtl={rtl}, growingRow={growingRow}");
            core.Undo();
            Check(core.GetText() == source && !core.CanUndo,
                "One undo removes all rectangular paste rows and any virtual-space padding");
            core.Redo();
            Check(core.GetText() == expected, "Rectangular paste redo preserves the exact row contents");
        }

        var originalFontSize = core.FontSize;
        var provider = (ITextProvider2)FrameworkElementAutomationPeer.CreatePeerForElement(view)
            .GetPattern(PatternInterface.Text);
        try
        {
            core.FontSize = 23.25;
            foreach (var rtl in new[] { false, true })
            {
                const string source = "   \r ";
                await LoadAsync(core, native, source, rtl);
                var spaces = provider.DocumentRange.Clone();
                spaces.MoveEndpointByRange(TextPatternRangeEndpoint.End, spaces, TextPatternRangeEndpoint.Start);
                Check(spaces.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 3) == 3,
                    "Fractional padding fixture measures the first row's three spaces");
                var width = SumWidths(spaces);
                Check(width > 0 && Math.Abs(width - Math.Round(width)) > 0.001,
                    "The rectangular paste target has a nonintegral text advance");
                var wideInsert = new string(rtl ? 'א' : 'a', 1000);
                var paste = wideInsert + "\rX";
                native.EmptyUndoBuffer();
                native.SetEmptySelection(3);
                native.ReplaceRectangular(Encoding.UTF8.GetByteCount(paste), paste);
                // All source columns are the same space glyph, so exactly two
                // additional spaces reach column three. Fractional X rounding
                // must not add a third space after the first row widens.
                Check(core.GetText() == "   " + wideInsert + "\r   X",
                    $"Rectangular paste pads the fractional EOL column by exactly two spaces: rtl={rtl}");
                core.Undo();
                Check(core.GetText() == source && !core.CanUndo,
                    "The wide insertion and exact EOL padding share one undo action");
            }
        }
        finally { await AfterPaintAsync(native, () => core.FontSize = originalFontSize); }
    }

    private static async Task CheckWrappingAsync(TextEditorCore core, Editor native)
    {
        var text = string.Concat(Enumerable.Repeat("אבג ABC 123 مرحبا 😀 ", 18));
        var originalFontSize = core.FontSize;
        var originalZoom = core.GetFontZoomFactor();
        try
        {
            await AfterPaintAsync(native, () =>
            {
                core.Width = 300;
                core.TextWrapping = TextWrapping.Wrap;
                core.RightToLeftReadingOrder = true;
                core.SetText(text);
                core.UpdateLayout();
            });
            var narrowRows = native.WrapCount(0);
            Check(narrowRows > 3, "Mixed RTL fixture spans multiple display rows");
            await AfterPaintAsync(native, () => { core.Width = 580; core.UpdateLayout(); });
            var wideRows = native.WrapCount(0);
            Check(wideRows < narrowRows, "Width change recomputes paragraph wrapping");
            await AfterPaintAsync(native, () => core.SetFontZoomFactor(originalZoom * 1.5));
            Check(core.FontSize > originalFontSize * 1.4 && native.WrapCount(0) > wideRows,
                $"Zoom invalidates RTL wrapping: width={core.ActualWidth}, font={originalFontSize}->{core.FontSize}, rows={wideRows}->{native.WrapCount(0)}");
            await AfterPaintAsync(native, () => core.RightToLeftReadingOrder = false);
            Check(native.WrapCount(0) > 1 && core.GetText() == text,
                "Changing base direction preserves every wrapped character");
            await AfterPaintAsync(native, () => core.RightToLeftReadingOrder = true);
            native.SetEmptySelection(0);
            native.CharLeft();
            Check(native.CurrentPos == 2, "Navigation uses refreshed layout after wrap/zoom/direction changes");
        }
        finally
        {
            await AfterPaintAsync(native, () =>
            {
                core.Width = 620;
                core.FontSize = originalFontSize;
                core.TextWrapping = TextWrapping.NoWrap;
                core.UpdateLayout();
            });
        }
    }

    private static async Task CheckRectangleZoomCollapseAsync(TextEditorCore core, Editor native)
    {
        var originalWidth = core.Width;
        var originalHeight = core.Height;
        var originalFontSize = core.FontSize;
        var originalZoom = native.Zoom;
        var failures = new StringBuilder();
        try
        {
            foreach (var managedFont in new[] { false, true })
            foreach (var reverse in new[] { false, true })
            foreach (var left in new[] { true, false })
            {
                var lines = Enumerable.Repeat("אבג", 161).ToArray();
                lines[^1] = new string('א', 1000);
                var text = string.Join('\r', lines);
                var lastStart = (long)Encoding.UTF8.GetByteCount(string.Join('\r', lines[..^1])) + 1;
                await AfterPaintAsync(native, () =>
                {
                    native.SelectionMode = SelectionMode.Stream;
                    native.MoveExtendsSelection = false;
                    native.Zoom = originalZoom;
                    core.FontSize = originalFontSize;
                    core.Width = 260;
                    core.Height = 160;
                    core.TextWrapping = TextWrapping.NoWrap;
                    core.RightToLeftReadingOrder = true;
                    core.SetText(text);
                    native.SetEmptySelection(0);
                    native.FirstVisibleLine = 0;
                    native.XOffset = 0;
                    core.UpdateLayout();
                });
                Check(native.FirstVisibleLine == 0 && native.LinesOnScreen < 80,
                    "The rectangle's wide endpoint remains outside the initial viewport");
                native.RectangularSelectionAnchor = reverse ? lastStart : 0;
                native.RectangularSelectionCaret = reverse ? 0 : lastStart;
                Check(native.Selections == lines.Length,
                    "The zero-width rectangle spans all fixture rows");
                for (var row = 0; row < lines.Length; row++)
                {
                    var documentRow = reverse ? lines.Length - 1 - row : row;
                    var expectedStart = documentRow * 7L; // UTF-8 אבג plus one CR.
                    Check(native.GetSelectionNStart(row) == expectedStart &&
                        native.GetSelectionNEnd(row) == expectedStart,
                        "The established rectangle starts empty at every paragraph's right edge");
                }

                // Deliberately no await, paint, UIA, PointX/Y, or layout query
                // between invalidating font metrics and collapsing selection.
                // This distinguishes synchronous geometry from a later paint
                // that may incidentally repair the shared canvas width.
                if (managedFont) core.FontSize = originalFontSize * 1.25;
                else native.Zoom = originalZoom + 2;
                if (left) native.CharLeft(); else native.CharRight();

                var expected = left ? lastStart : 0;
                if (native.CurrentPos != expected || native.Anchor != expected ||
                    native.SelectionMode != SelectionMode.Stream || native.Selections != 1)
                {
                    failures.AppendLine($"source={(managedFont ? "managed-font" : "native-zoom")}, " +
                        $"reverse={reverse}, left={left}, expected={expected}, " +
                        $"caret={native.CurrentPos}, anchor={native.Anchor}, ranges={native.Selections}");
                }
            }
            Check(failures.Length == 0,
                "An immediate font change preserves rectangle collapse at the original physical corners:\n" + failures);
        }
        finally
        {
            await AfterPaintAsync(native, () =>
            {
                native.SelectionMode = SelectionMode.Stream;
                native.MoveExtendsSelection = false;
                native.SetEmptySelection(0);
                native.Zoom = originalZoom;
                core.FontSize = originalFontSize;
                core.Width = originalWidth;
                core.Height = originalHeight;
                core.UpdateLayout();
            });
        }
    }

    private static async Task CheckRectangleMetricReplacementAsync(TextEditorCore core, Editor native)
    {
        var originalWidth = core.Width;
        var originalHeight = core.Height;
        var originalFontSize = core.FontSize;
        var originalZoom = native.Zoom;
        const int rowCount = 161;
        var tail = new string('א', 1000);
        var source = string.Join('\r', Enumerable.Repeat("אבג ABC", rowCount)) + tail;
        const long lastStart = (rowCount - 1) * 11L; // UTF-8 אבג ABC plus CR.
        try
        {
            foreach (var (rtl, managedFont, reverse, clearFirst) in new[]
            {
                (true, false, false, false), (true, true, true, true),
                (false, false, true, false), (false, true, false, true)
            })
            {
                var context = $"rtl={rtl}, managedFont={managedFont}, reverse={reverse}, clearFirst={clearFirst}";
                await AfterPaintAsync(native, () =>
                {
                    native.SelectionMode = SelectionMode.Stream;
                    native.MoveExtendsSelection = false;
                    native.Zoom = originalZoom;
                    core.FontSize = originalFontSize;
                    core.Width = 260;
                    core.Height = 160;
                    core.TextWrapping = TextWrapping.NoWrap;
                    core.RightToLeftReadingOrder = rtl;
                    core.SetText(source);
                    native.SetEmptySelection(0);
                    native.FirstVisibleLine = 0;
                    native.XOffset = 0;
                    core.UpdateLayout();
                });
                native.EmptyUndoBuffer();
                native.RectangularSelectionAnchor = reverse ? lastStart + 8 : 0;
                native.RectangularSelectionCaret = reverse ? 0 : lastStart + 8;

                // Fix the logical islands independently of their enumeration
                // order. RTL leaves A; LTR selects only the space and A.
                var actualIslands = Enumerable.Range(0, checked((int)native.Selections))
                    .Select(index => (Start: native.GetSelectionNStart(index), End: native.GetSelectionNEnd(index)))
                    .OrderBy(range => range.Start).ThenBy(range => range.End).ToArray();
                var expectedIslands = Enumerable.Range(0, rowCount).SelectMany(row => rtl
                    ? new[] { (Start: row * 11L, End: row * 11L + 7), (Start: row * 11L + 8, End: row * 11L + 10) }
                    : new[] { (Start: row * 11L + 6, End: row * 11L + 8) }).ToArray();
                Check(actualIslands.SequenceEqual(expectedIslands),
                    $"Metric-change fixture establishes exact mixed-direction rectangle islands: {context}");
                string Expected(string insertion) => string.Join('\r', Enumerable.Repeat(
                    rtl ? insertion + "A" : "אבג" + insertion + "BC", rowCount)) + tail;

                // No paint or geometry query may repair the canvas between
                // changing metrics and the first destructive command.
                if (managedFont) core.FontSize = originalFontSize * 1.25;
                else native.Zoom = originalZoom + 2;
                if (clearFirst)
                {
                    native.Clear();
                    Check(core.GetText() == Expected(string.Empty),
                        $"Clear removes only the established islands: {context}");
                }
                core.TypeText("X");
                Check(core.GetText() == Expected("X"),
                    $"Replacement inserts once at each original leading edge: {context}");
                core.TypeText("Y");
                Check(core.GetText() == Expected("XY"),
                    $"Subsequent typing preserves the insertion anchors after thinning the rectangle: {context}");
                core.Undo();
                Check(core.GetText() == Expected("X"), "One undo removes the second input across all rectangle rows");
                core.Undo();
                Check(core.GetText() == (clearFirst ? Expected(string.Empty) : source),
                    "One undo removes the entire rectangle replacement or insertion");
                if (clearFirst) core.Undo();
                Check(core.GetText() == source && !core.CanUndo,
                    "Rectangle editing restores the exact original document without extra undo steps");
            }
        }
        finally
        {
            await AfterPaintAsync(native, () =>
            {
                native.SelectionMode = SelectionMode.Stream;
                native.MoveExtendsSelection = false;
                native.SetEmptySelection(0);
                native.Zoom = originalZoom;
                core.FontSize = originalFontSize;
                core.Width = originalWidth;
                core.Height = originalHeight;
                core.UpdateLayout();
            });
        }
    }

    private static async Task CheckColdRectangleCanvasAsync(TextEditorCore core, Editor native)
    {
        var originalWidth = core.Width;
        var originalHeight = core.Height;
        var failures = new StringBuilder();
        try
        {
            foreach (var (widestAtEnd, selectedCharacters) in new[] { (true, 0), (false, 0), (true, 1), (false, 1) })
            {
                var lines = Enumerable.Repeat("אבג", 161).ToArray();
                lines[widestAtEnd ? lines.Length - 1 : 80] = new string('א', 1000);
                var text = string.Join('\r', lines);
                var starts = new long[lines.Length];
                for (var line = 1; line < lines.Length; line++)
                    starts[line] = starts[line - 1] + Encoding.UTF8.GetByteCount(lines[line - 1]) + 1;
                var lastStart = starts[^1];
                await AfterPaintAsync(native, () =>
                {
                    core.Width = 260;
                    core.Height = 160;
                    core.TextWrapping = TextWrapping.NoWrap;
                    core.RightToLeftReadingOrder = true;
                    native.SelectionMode = SelectionMode.Stream;
                    native.MoveExtendsSelection = false;
                    core.SetText(text);
                    native.SetEmptySelection(0);
                    native.FirstVisibleLine = 0;
                    native.XOffset = 0;
                    core.UpdateLayout();
                });
                Check(native.FirstVisibleLine == 0 && native.LinesOnScreen < 80,
                    "The wide RTL paragraph is outside the initial viewport");
                var coldRight = native.PointXFromPosition(0);
                var viewportWidth = FindElement<Border>(core, "ImageTarget").ActualWidth *
                    DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
                Check(native.XOffset == 0 && coldRight > 0 && coldRight <= viewportWidth + 1,
                    "The initial visible line has not inherited an expanded canvas from a previous fixture");
                // Do not inspect offscreen coordinates or UIA rectangles before
                // this operation: those queries would prime the shared canvas.
                // Logical starts share the right edge; a one-character span
                // must select exactly the first Hebrew glyph on every row.
                native.RectangularSelectionAnchor = 0;
                native.RectangularSelectionCaret = lastStart + selectedCharacters * 2;
                var nonemptyRows = 0;
                var misplacedRows = 0;
                for (var selection = 0; selection < native.Selections; selection++)
                {
                    var start = native.GetSelectionNStart(selection);
                    var end = native.GetSelectionNEnd(selection);
                    if (start != end) nonemptyRows++;
                    if (selection >= starts.Length || start != starts[selection] ||
                        end != starts[selection] + selectedCharacters * 2) misplacedRows++;
                }
                if (native.Selections != lines.Length || misplacedRows != 0)
                    failures.AppendLine($"widest={(widestAtEnd ? "endpoint" : "middle")}, " +
                        $"selected={selectedCharacters}, rows={native.Selections}, nonempty={nonemptyRows}, misplaced={misplacedRows}");
                Check(native.RectangularSelectionAnchor == 0 &&
                    native.RectangularSelectionCaret == lastStart + selectedCharacters * 2,
                    "Building a cold rectangle retains both original logical corners");
            }
            Check(failures.Length == 0,
                "A cold RTL rectangle preserves its exact logical range on every row:\n" + failures);
        }
        finally
        {
            await AfterPaintAsync(native, () =>
            {
                native.SelectionMode = SelectionMode.Stream;
                native.MoveExtendsSelection = false;
                native.SetEmptySelection(0);
                core.Width = originalWidth;
                core.Height = originalHeight;
                core.UpdateLayout();
            });
        }
    }

    private static async Task CheckRectanglesAsync(TextEditorCore core, Editor native)
    {
        await CheckRectangleCollapseAsync(core, native);
        await CheckRectangleViewportAsync(core, native);
        const string line = "abc אבג def";
        foreach (var twoRows in new[] { false, true })
        {
            var text = twoRows ? line + "\r" + line : line;
            await LoadAsync(core, native, text, false);
            native.EmptyUndoBuffer();
            void SelectRectangle()
            {
                native.SelectionMode = SelectionMode.Rectangle;
                native.RectangularSelectionAnchor = 1;
                native.RectangularSelectionCaret = twoRows ? 23 : 8;
            }
            SelectRectangle();
            Check(native.Selections == (twoRows ? 4 : 2), "One visual rectangle exposes the separate logical islands on each row");
            Equal(twoRows ? "bc ג\rbc ג\r" : "bc ג\r", core.GetSelectedText(),
                "Rectangle copy groups visual rows and excludes the unselected aleph/bet");
            native.Clear();
            Equal(twoRows ? "aאב def\raאב def" : "aאב def", core.GetText(),
                "Rectangle deletion never removes unselected characters between logical islands");
            core.Undo();
            Equal(text, core.GetText(), "Rectangle deletion is one undo action across all islands");
            Check(!core.CanUndo, "Rectangle deletion introduced exactly one undo boundary");
            SelectRectangle();
            core.TypeText("X");
            Equal(twoRows ? "aXאב def\raXאב def" : "aXאב def", core.GetText(),
                "Rectangle replacement inserts once per display row rather than once per logical island");
            core.Undo();
            Equal(text, core.GetText(), "Rectangle replacement undoes as one action");
            Check(!core.CanUndo, "Rectangle replacement introduced exactly one undo boundary");
        }
        native.SelectionMode = SelectionMode.Stream;

        Progress("rectangle: pure RTL paste");
        await LoadAsync(core, native, "אבג\rאבג", true);
        native.EmptyUndoBuffer();
        native.SetEmptySelection(2);
        native.ReplaceRectangular(3, "X\rY");
        Equal("אXבג\rאYבג", core.GetText(), "Rectangular paste inserts once at the same RTL column on each line");
        core.Undo();
        Equal("אבג\rאבג", core.GetText(), "RTL rectangular paste is one undo action");
        Check(!core.CanUndo, "RTL rectangular paste has one undo boundary");

        await LoadAsync(core, native, "אבגד\rא", true);
        native.EmptyUndoBuffer();
        native.SetEmptySelection(6);
        native.ReplaceRectangular(3, "X\rY");
        var pasted = core.GetText().Split('\r');
        Equal("אבגXד", pasted[0], "RTL rectangle paste keeps the first line's fixed logical insertion");
        Check(pasted.Length == 2 && pasted[1].Length is > 2 and < 64 && pasted[1][0] == 'א' &&
            pasted[1][^1] == 'Y' && pasted[1].AsSpan(1, pasted[1].Length - 2).IndexOfAnyExcept(' ') < 0,
            "RTL rectangle paste pads a shorter following line with bounded whitespace and terminates");
        var xGlyph = native.PointXFromPosition(6); // Fixed byte offset of X in אבגXד.
        var yPosition = native.PositionFromLine(1) + Encoding.UTF8.GetByteCount(pasted[1][..^1]);
        var yGlyph = native.PointXFromPosition(yPosition);
        var spaceAdvance = native.TextWidth((int)StylesCommon.Default, " ");
        Check(spaceAdvance > 0 && Math.Abs(xGlyph - yGlyph) <= spaceAdvance + 1,
            $"Padded rectangle glyphs align within one space advance: X={xGlyph}, Y={yGlyph}, space={spaceAdvance}");
        core.Undo();
        Equal("אבגד\rא", core.GetText(), "RTL rectangle paste removes padding and inserted text in one undo");
        Check(!core.CanUndo, "Padded RTL rectangle paste has one undo boundary");

        Progress("rectangle: wrapped display rows");
        try
        {
            await AfterPaintAsync(native, () =>
            {
                core.Width = 220;
                core.TextWrapping = TextWrapping.Wrap;
                core.SetText(new string('א', 64));
                core.UpdateLayout();
            });
            var firstY = native.PointYFromPosition(0);
            var secondRow = Enumerable.Range(1, 63).Select(index => (long)index * 2)
                .First(position => native.PointYFromPosition(position) > firstY);
            native.EmptyUndoBuffer();
            native.SelectionMode = SelectionMode.Rectangle;
            native.RectangularSelectionAnchor = 2;
            native.RectangularSelectionCaret = secondRow + 4;
            Check(native.Selections == 2, "Wrapped rectangle produces one selection on each of two display rows");
            var starts = Enumerable.Range(0, (int)native.Selections).Select(index => native.GetSelectionNStart(index)).OrderBy(value => value).ToArray();
            var ends = Enumerable.Range(0, (int)native.Selections).Select(index => native.GetSelectionNEnd(index)).OrderBy(value => value).ToArray();
            Check(starts.SequenceEqual(new[] { 2L, secondRow + 2 }) && ends.SequenceEqual(new[] { 4L, secondRow + 4 }),
                "Wrapped rectangle selects exactly the second glyph of each visible row");
            native.Clear();
            Equal(new string('א', 62), core.GetText(), "Wrapped rectangle removes exactly two selected glyphs");
            core.Undo();
            Equal(new string('א', 64), core.GetText(), "Wrapped rectangle deletion undoes as one operation");
            Check(!core.CanUndo, "Wrapped rectangle has one undo boundary");

            native.SelectionMode = SelectionMode.Stream;
            native.MoveExtendsSelection = false;
            native.SetEmptySelection(0);
            for (long position = 0; position < secondRow; position += 2) native.CharLeftRectExtend();
            Check(native.RectangularSelectionCaret == secondRow,
                "Keyboard rectangle reaches the first wrapped row's trailing edge");
            native.CharLeft();
            Check(native.CurrentPos == secondRow && native.SelectionMode == SelectionMode.Stream,
                "Left collapses the wrapped rectangle to its trailing row edge");
            native.CharRight();
            Check(native.CurrentPos == secondRow - 2,
                "Collapsed rectangle retains wrap affinity when reversing direction");
        }
        finally
        {
            await AfterPaintAsync(native, () =>
            {
                native.SelectionMode = SelectionMode.Stream;
                core.Width = 620;
                core.TextWrapping = TextWrapping.NoWrap;
                core.UpdateLayout();
            });
        }
    }

    private static async Task CheckRectangleCollapseAsync(TextEditorCore core, Editor native)
    {
        foreach (var rtl in new[] { false, true })
        {
            await LoadAsync(core, native, "אבג", rtl);
            foreach (var left in new[] { true, false })
            {
                native.MoveExtendsSelection = false;
                native.SetEmptySelection(0);
                native.CharLeftRectExtend();
                native.CharLeftRectExtend();
                Check(native.RectangularSelectionCaret == 4, "Keyboard rectangle covers exactly two Hebrew glyphs");
                if (left) native.CharLeft(); else native.CharRight();
                var expected = left ? 4 : 0;
                Check(native.SelectionMode == SelectionMode.Stream && native.CurrentPos == expected && native.Anchor == expected,
                    $"Hebrew rectangle collapses to the physical edge: rtl={rtl}, left={left}, expected={expected}, actual={native.CurrentPos}");
            }

            var text = rtl ? "אבג\rאבג" : "abcd\rabcd";
            await LoadAsync(core, native, text, rtl);
            var secondLine = rtl ? 7L : 5L;
            var twoCharacters = rtl ? 4L : 2L;
            foreach (var reverseDiagonal in new[] { false, true })
            {
                foreach (var left in new[] { true, false })
                {
                    native.MoveExtendsSelection = false;
                    native.RectangularSelectionAnchor = reverseDiagonal ? twoCharacters : 0;
                    native.RectangularSelectionCaret = secondLine + (reverseDiagonal ? 0 : twoCharacters);
                    if (left) native.CharLeft(); else native.CharRight();
                    var expected = left == rtl ? secondLine + twoCharacters : 0;
                    Check(native.SelectionMode == SelectionMode.Stream && native.CurrentPos == expected && native.Anchor == expected,
                        $"Multiline rectangle collapse is independent of diagonal: rtl={rtl}, reverse={reverseDiagonal}, left={left}, expected={expected}, actual={native.CurrentPos}");
                }
            }
        }

        foreach (var twoRows in new[] { false, true })
        {
            const string line = "abc אבג def";
            await LoadAsync(core, native, twoRows ? line + "\r" + line : line, false);
            foreach (var reverse in new[] { false, true })
            {
                foreach (var left in new[] { true, false })
                {
                    native.MoveExtendsSelection = false;
                    var farEdge = twoRows ? 23L : 8L;
                    native.RectangularSelectionAnchor = reverse ? farEdge : 1;
                    native.RectangularSelectionCaret = reverse ? 1 : farEdge;
                    Check(native.Selections == (twoRows ? 4 : 2), "Collapse fixture contains disjoint logical islands");
                    if (left) native.CharLeft(); else native.CharRight();
                    var expected = left ? 1 : farEdge;
                    Check(native.SelectionMode == SelectionMode.Stream && native.CurrentPos == expected && native.Anchor == expected,
                        $"Mixed rectangle uses the visible island edge: rows={(twoRows ? 2 : 1)}, reverse={reverse}, left={left}, expected={expected}, actual={native.CurrentPos}");
                    if (left) native.CharLeft(); else native.CharRight();
                    Check(native.CurrentPos == (left ? 0 : farEdge - 2),
                        "A collapsed mixed rectangle keeps the island's visual caret affinity");
                }
            }

            foreach (var reverseDiagonal in new[] { false, true })
            {
                foreach (var left in new[] { true, false })
                {
                    native.MoveExtendsSelection = false;
                    native.SetEmptySelection(reverseDiagonal ? 12 : 8);
                    if (!reverseDiagonal) native.CharLeft(); // 10 trailing, the Hebrew run's visual left edge.
                    var moves = new StringBuilder();
                    for (var step = 0; step < 5; step++)
                    {
                        if (reverseDiagonal) native.CharLeftRectExtend(); else native.CharRightRectExtend();
                        moves.Append(native.RectangularSelectionCaret).Append(',');
                    }
                    if (twoRows) native.LineDownRectExtend();
                    Check(native.Selections == (twoRows ? 2 : 1) && native.GetSelectionNStart(0) == 4 &&
                        native.GetSelectionNEnd(0) == 12,
                        $"Merged rectangle fixture: rows={(twoRows ? 2 : 1)}, reverse={reverseDiagonal}, left={left}, " +
                        $"anchor={native.RectangularSelectionAnchor}, caret={native.RectangularSelectionCaret}, moves=[{moves}], " +
                        "ranges=" + string.Join(",", Enumerable.Range(0, (int)native.Selections)
                            .Select(index => $"[{native.GetSelectionNStart(index)},{native.GetSelectionNEnd(index)}]")));
                    if (left) native.CharLeft(); else native.CharRight();
                    var expected = left ? 10 : twoRows ? 27 : 12;
                    Check(native.SelectionMode == SelectionMode.Stream && native.CurrentPos == expected && native.Anchor == expected,
                        $"Merged island collapse uses its visual corner: rows={(twoRows ? 2 : 1)}, reverse={reverseDiagonal}, left={left}, expected={expected}, actual={native.CurrentPos}");
                    if (left)
                    {
                        native.CharRight();
                        Check(native.CurrentPos == 8,
                            "The merged island's left corner retains trailing affinity inside the Hebrew run");
                    }
                }
            }
        }

        await LoadAsync(core, native, "אבג", true);
        foreach (var home in new[] { true, false })
        {
            native.MoveExtendsSelection = false;
            native.SetEmptySelection(0);
            native.CharLeftRectExtend();
            native.CharLeftRectExtend();
            if (home) native.Home(); else native.LineEnd();
            Check(native.CurrentPos == (home ? 0 : 6) && native.SelectionMode == SelectionMode.Stream,
                "Home/End retain logical paragraph boundaries when leaving a rectangle");
        }
    }

    private static async Task CheckRectangleViewportAsync(TextEditorCore core, Editor native)
    {
        const int characters = 40;
        var line = "abc " + new string('א', characters) + " def";
        var hebrewEnd = 4 + characters * 2;
        var selectedEnd = hebrewEnd + 2;
        var secondLine = Encoding.UTF8.GetByteCount(line) + 1;
        var previousWidth = core.Width;
        try
        {
            await AfterPaintAsync(native, () => core.Width = 260);
            await LoadAsync(core, native, line + "\r" + line, false);
            native.MoveExtendsSelection = false;
            native.SetEmptySelection(selectedEnd);
            native.ScrollCaret();
            Check(native.XOffset > 0, "Long mixed rectangle fixture starts horizontally scrolled");
            for (var step = 0; step < characters + 2; step++) native.CharLeftRectExtend();
            Check(native.RectangularSelectionCaret == hebrewEnd && native.GetSelectionNStart(0) == 4 &&
                native.GetSelectionNEnd(0) == selectedEnd,
                "Long rectangle's physical corner is inside its merged logical island");
            var viewportWidth = FindElement<Border>(core, "ImageTarget").ActualWidth *
                DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
            var visibleGlyph = native.PointXFromPosition(hebrewEnd - 2);
            Check(visibleGlyph > 0 && visibleGlyph < viewportWidth,
                "Rectangle extension scrolls to the physical Hebrew edge, not the opposite logical endpoint");
            var offset = native.XOffset;
            native.ScrollCaret();
            Check(native.XOffset == offset, "Explicit ScrollCaret keeps the already visible rectangle corner stable");
            var view = (EditorBaseControl)((Grid)core.Content).Children[0];
            var pattern = FrameworkElementAutomationPeer.CreatePeerForElement(view).GetPattern(PatternInterface.Text);
            void CheckPhysicalCorner(int row, long position)
            {
                var lineStart = row * secondLine;
                // At this bidi boundary the preceding space's trailing edge
                // and Hebrew run's trailing edge occupy the same visual point.
                Check(position == lineStart + 4 || position == lineStart + hebrewEnd,
                    "Vertical hit testing chooses one of the two equivalent boundary positions");
                var expectedGlyph = ((ITextProvider)pattern).DocumentRange.Clone();
                expectedGlyph.MoveEndpointByRange(TextPatternRangeEndpoint.End, expectedGlyph, TextPatternRangeEndpoint.Start);
                expectedGlyph.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, TextUnit.Character,
                    row * (line.Length + 1) + 4 + characters - 1);
                expectedGlyph.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 1);
                var actualBounds = Rectangles(((ITextProvider2)pattern).GetCaretRange(out _));
                var expectedBounds = Rectangles(expectedGlyph);
                Check(actualBounds.Length == 4 && expectedBounds.Length == 4 &&
                    Math.Abs(actualBounds[0] - expectedBounds[0]) < 2 && Math.Abs(actualBounds[1] - expectedBounds[1]) < 2,
                    "The actual caret stays at the requested row and physical Hebrew edge");
            }
            var glyph = ((ITextProvider)pattern).DocumentRange.Clone();
            glyph.MoveEndpointByRange(TextPatternRangeEndpoint.End, glyph, TextPatternRangeEndpoint.Start);
            glyph.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, TextUnit.Character, 4 + characters - 1);
            glyph.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 1);
            var automationCaret = ((ITextProvider2)pattern).GetCaretRange(out _);
            Check(automationCaret.CompareEndpoints(TextPatternRangeEndpoint.Start, glyph, TextPatternRangeEndpoint.End) == 0,
                "UIA reports the physical rectangle corner's logical position without changing the selection islands");
            var caretBounds = Rectangles(automationCaret);
            var glyphBounds = Rectangles(glyph);
            Check(caretBounds.Length == 4 && glyphBounds.Length == 4 && Math.Abs(caretBounds[0] - glyphBounds[0]) < 2,
                "UIA degenerate rectangle caret retains the trailing edge at the Hebrew visual left boundary");
            native.LineDownRectExtend();
            CheckPhysicalCorner(1, native.RectangularSelectionCaret);
            Check(native.Selections == 2 && native.GetSelectionNStart(0) == 4 && native.GetSelectionNEnd(0) == selectedEnd &&
                native.GetSelectionNStart(1) == secondLine + 4 && native.GetSelectionNEnd(1) == secondLine + selectedEnd,
                "Down preserves the rectangle's physical column and complete first-row island");
            Check(native.PointXFromPosition(secondLine + hebrewEnd - 2) == visibleGlyph,
                "The second-row rectangle caret uses the same visible column");
            native.CharLeft();
            CheckPhysicalCorner(0, native.CurrentPos);
            native.CharRight();
            Check(native.CurrentPos == hebrewEnd - 2, "Collapsed long rectangle retains the internal trailing affinity");
            foreach (var down in new[] { true, false })
            {
                native.SetEmptySelection((down ? 0 : secondLine) + selectedEnd);
                for (var step = 0; step < characters + 2; step++) native.CharLeftRectExtend();
                if (down) native.LineDown(); else native.LineUp();
                var row = down ? 1 : 0;
                Check(native.SelectionMode == SelectionMode.Stream && native.CurrentPos == native.Anchor,
                    "Plain Up/Down leaves a rectangle from its physical corner and keeps its visual column");
                CheckPhysicalCorner(row, native.CurrentPos);
                native.CharRight();
                Check(native.CurrentPos == row * secondLine + hebrewEnd - 2, "Vertical rectangle exit preserves trailing affinity");
            }
        }
        finally { await AfterPaintAsync(native, () => core.Width = previousWidth); }
    }

    private static async Task CheckVirtualSpaceAsync(TextEditorCore core, Editor native)
    {
        var previousOptions = native.VirtualSpaceOptions;
        try
        {
            native.VirtualSpaceOptions = VirtualSpace.RectangularSelection;
            foreach (var rtl in new[] { false, true })
            {
                var text = rtl ? "אבג ABC\rאבג ABC" : "ABC אבג\rABC אבג";
                var position = rtl ? 8L : 6L;
                await LoadAsync(core, native, text, rtl);
                native.EmptyUndoBuffer();
                native.SelectionMode = SelectionMode.Thin;
                native.RectangularSelectionAnchor = position;
                native.RectangularSelectionCaret = position + 11;
                Check(native.Selections == 2, "Thin mixed-direction rectangle spans two rows");
                var carets = Enumerable.Range(0, 2).Select(native.GetSelectionNCaret).OrderBy(value => value).ToArray();
                Check(carets.SequenceEqual(new[] { position, position + 11 }),
                    "A real glyph after the logical EOL's visual X does not become an EOL hit");
                Check(Enumerable.Range(0, 2).All(index => native.GetSelectionNCaretVirtualSpace(index) == 0 &&
                    native.GetSelectionNAnchorVirtualSpace(index) == 0),
                    "Thin selection inside opposite-direction text has zero virtual spaces");
                core.TypeText("|");
                Equal(rtl ? "אבג A|BC\rאבג A|BC" : "ABC א|בג\rABC א|בג", core.GetText(),
                    "Thin rectangle inserts inside both real opposite-direction runs");
                core.Undo();
                Equal(text, core.GetText(), "Opposite-direction thin insertion has one undo");
                Check(!core.CanUndo, "Thin insertion introduced one undo boundary");
            }

            await LoadAsync(core, native, "אבג\rאבג", true);
            native.EmptyUndoBuffer();
            native.SelectionMode = SelectionMode.Thin;
            native.RectangularSelectionAnchor = 6;
            native.RectangularSelectionCaret = 13;
            native.RectangularSelectionAnchorVirtualSpace = 2;
            native.RectangularSelectionCaretVirtualSpace = 2;
            Check(native.Selections == 2 && Enumerable.Range(0, 2).All(index => native.GetSelectionNCaretVirtualSpace(index) == 2),
                "Pure RTL thin rectangles retain explicit virtual space beyond the visual end");
            core.TypeText("|");
            Equal("אבג  |\rאבג  |", core.GetText(), "Pure RTL virtual insertion materializes exactly two spaces per row");
            core.Undo();
            Equal("אבג\rאבג", core.GetText(), "Pure RTL virtual insertion and padding undo together");
            Check(!core.CanUndo, "Pure RTL virtual insertion has one undo boundary");
        }
        finally
        {
            native.SelectionMode = SelectionMode.Stream;
            native.VirtualSpaceOptions = previousOptions;
        }
    }

    private static async Task CheckAutomationAsync(TextEditorCore core, Editor native, EditorBaseControl view)
    {
        var provider = (ITextProvider)FrameworkElementAutomationPeer.CreatePeerForElement(view).GetPattern(PatternInterface.Text);
        await LoadAsync(core, native, "אבג", true);
        Progress("UIA: Hebrew nonempty range");
        Progress($"UIA: loaded={view.IsLoaded}, size={view.ActualWidth}x{view.ActualHeight}, scrollWidth={native.ScrollWidth}, xOffset={native.XOffset}, firstLine={native.FirstVisibleLine}, x0={native.PointXFromPosition(0)}, x6={native.PointXFromPosition(6)}");
        var hebrew = provider.DocumentRange;
        Equal("אבג", hebrew.GetText(-1), "UIA exposes RTL text in logical order");
        CheckRectangles(Rectangles(hebrew), false);
        var caret = hebrew.Clone();
        caret.MoveEndpointByRange(TextPatternRangeEndpoint.End, caret, TextPatternRangeEndpoint.Start);
        Progress("UIA: degenerate Hebrew caret");
        CheckRectangles(Rectangles(caret), false);
        var beforeMove = Rectangles(hebrew)[0];
        core.Margin = new Thickness(40, 0, 0, 0);
        core.UpdateLayout();
        var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        Check(Math.Abs(Rectangles(hebrew)[0] - beforeMove - 40 * scale) < 2,
            "Retained UIA ranges query the current screen transform after layout moves");
        core.Margin = new Thickness(0);
        core.UpdateLayout();

        await LoadAsync(core, native, "abc אבג def", false);
        Progress("UIA: mixed disjoint range");
        var split = provider.DocumentRange.Clone();
        split.MoveEndpointByRange(TextPatternRangeEndpoint.End, split, TextPatternRangeEndpoint.Start);
        Check(split.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, TextUnit.Character, 1) == 1,
            "UIA sets the mixed selection start");
        Check(split.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 5) == 5,
            "UIA sets the mixed selection end");
        Equal("bc אב", split.GetText(-1), "Disjoint visual selection remains contiguous logical text");
        CheckRectangles(Rectangles(split), true);
        split.Select();
        Equal("bc אב", core.GetSelectedText(), "UIA selection reaches the native logical range");
        Check(native.SelectionStart == 1 && native.SelectionEnd == 8,
            "UIA mixed selection converts UTF-16 positions to exact UTF-8 bytes");

        // Provider geometry is in physical root units (the UWP adapter adds
        // the screen origin), so a zoomed font must increase
        // the selected span regardless of the host's current display scale.
        var originalWidth = SumWidths(provider.DocumentRange);
        Progress("UIA: zoomed range");
        var originalFontSize = core.FontSize;
        var originalZoom = core.GetFontZoomFactor();
        await AfterPaintAsync(native, () => core.SetFontZoomFactor(originalZoom * 1.5));
        Check(core.FontSize > originalFontSize * 1.4 && SumWidths(provider.DocumentRange) > originalWidth * 1.3,
            "UIA range geometry tracks zoom rather than stale screen coordinates");
        await AfterPaintAsync(native, () => core.FontSize = originalFontSize);

        await AfterPaintAsync(native, () =>
        {
            core.Width = 260;
            core.TextWrapping = TextWrapping.Wrap;
            core.RightToLeftReadingOrder = true;
            core.SetText(string.Concat(Enumerable.Repeat("אבג ABC ", 8)));
            core.UpdateLayout();
        });
        var wrapped = Rectangles(provider.DocumentRange);
        Progress("UIA: wrapped range");
        CheckRectangles(wrapped, false);
        Check(Enumerable.Range(0, wrapped.Length / 4).Select(index => wrapped[index * 4 + 1]).Distinct().Count() > 1,
            "UIA wrapped paragraph exposes multiple visual rows");
        await AfterPaintAsync(native, () =>
        {
            core.TextWrapping = TextWrapping.NoWrap;
            core.RightToLeftReadingOrder = false;
            core.SetText("ABC אבג " + new string('x', 200));
            native.SetEmptySelection(0);
            core.UpdateLayout();
        });
        // Let the changed wrapping/direction and scrollbar viewport settle
        // before applying the user's horizontal scroll to the loaded document.
        await AfterPaintAsync(native, () => native.XOffset = 220);
        var offscreen = provider.DocumentRange.Clone();
        offscreen.MoveEndpointByRange(TextPatternRangeEndpoint.End, offscreen, TextPatternRangeEndpoint.Start);
        offscreen.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 1);
        Check(native.PointXFromPosition(0) < 0,
            $"Horizontal clip fixture puts the first character offscreen: offset={native.XOffset}, x0={native.PointXFromPosition(0)}");
        Check(Rectangles(offscreen).Length == 0, "Offscreen UIA text has no visible rectangle");
        var visible = Rectangles(provider.DocumentRange);
        Progress("UIA: horizontally clipped visible range");
        CheckRectangles(visible, false);
        var surface = FindElement<Border>(view, "ImageTarget");
        var origin = surface.TransformToVisual(null).TransformPoint(new Windows.Foundation.Point());
        var leftEdge = origin.X * scale;
        var rightEdge = leftEdge + surface.ActualWidth * scale;
        for (var index = 0; index < visible.Length; index += 4)
            Check(visible[index] >= leftEdge - 1 && visible[index] + visible[index + 2] <= rightEdge + 1,
                "UIA rectangles are clipped to the current native viewport");

        Progress("UIA: immediate common RTL canvas");
        core.RightToLeftReadingOrder = true;
        core.SetText("אבג\r" + new string('א', 80));
        native.XOffset = 0;
        native.FirstVisibleLine = 0;
        var shortLine = provider.DocumentRange.Clone();
        shortLine.MoveEndpointByRange(TextPatternRangeEndpoint.End, shortLine, TextPatternRangeEndpoint.Start);
        shortLine.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, 3);
        // Deliberately query before yielding for paint. The later wide visible
        // line must already determine the shared RTL alignment canvas.
        var immediate = Rectangles(shortLine);
        Check(immediate.Length == 0,
            "Before paint, the later wide RTL line already puts the short first line beyond the unscrolled viewport");
        await AfterPaintAsync(native, () => native.GotoPos(0));
        var shortVisible = Rectangles(shortLine);
        Progress("UIA: short line on shared RTL canvas");
        CheckRectangles(shortVisible, false);
        var wideLine = provider.DocumentRange.Clone();
        wideLine.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, TextUnit.Character, 4);
        var wideVisible = Rectangles(wideLine);
        Progress("UIA: wide line on shared RTL canvas");
        CheckRectangles(wideVisible, false);
        static double RightEdge(double[] rectangles) => Enumerable.Range(0, rectangles.Length / 4)
            .Max(index => rectangles[index * 4] + rectangles[index * 4 + 2]);
        Check(Math.Abs(RightEdge(shortVisible) - RightEdge(wideVisible)) < 2,
            "Short and later wide RTL lines expose the same visible right edge");
    }

    private static double SumWidths(ITextRangeProvider range)
    {
        var rectangles = Rectangles(range);
        double width = 0;
        for (var index = 0; index < rectangles.Length; index += 4) width += rectangles[index + 2];
        return width;
    }

    private static void CheckRectangles(double[] rectangles, bool disjoint)
    {
        Check(rectangles.Length >= 4 && rectangles.Length % 4 == 0, "UIA returns complete nonempty rectangles");
        double left = double.PositiveInfinity, right = double.NegativeInfinity, width = 0;
        for (var index = 0; index < rectangles.Length; index += 4)
        {
            Check(double.IsFinite(rectangles[index]) && double.IsFinite(rectangles[index + 1]) &&
                rectangles[index + 2] > 0 && rectangles[index + 3] > 0,
                "Every UIA visual rectangle has finite coordinates and positive dimensions");
            left = Math.Min(left, rectangles[index]);
            right = Math.Max(right, rectangles[index] + rectangles[index + 2]);
            width += rectangles[index + 2];
        }
        if (disjoint) Check(rectangles.Length >= 8 && right - left > width + 1,
            "UIA preserves the unselected Hebrew glyph between two selected visual islands");
    }

    private static double[] Rectangles(ITextRangeProvider range)
    {
        range.GetBoundingRectangles(out var rectangles);
        return rectangles ?? Array.Empty<double>();
    }

    private static void Progress(string message) => File.AppendAllText(
        Path.Combine(ApplicationData.Current.LocalFolder.Path, "progress.txt"), "BiDi: " + message + Environment.NewLine);

    private static void Equal(string expected, string actual, string message) => Check(expected == actual,
        message + $"; expected [{expected}], actual [{actual}]");
    private static void Check(bool value, string message)
    {
        if (!value) throw new InvalidOperationException("BiDi: " + message);
    }
}
