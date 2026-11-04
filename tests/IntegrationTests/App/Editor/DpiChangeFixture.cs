// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.Storage;
using Notepads.Presentation.Controls.DiffViewer;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Graphics.Display;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using WinUIEditor;

namespace NotepadsEditorTests;

// The external driver changes Windows display settings. This fixture only
// observes real DPI notifications; it never supplies a synthetic XAML scale.
internal sealed partial class DpiChangeFixture : IDisposable
{
    private const int TargetLine = 30;
    private const int GlyphColumn = 20;
    private readonly UIElement previous = Window.Current.Content;
    private readonly List<Pane> panes = [];
    private TextEditorCore source;
    private SideBySideDiffViewer viewer;
    private bool disposed;

    private sealed partial class Pane(TextEditorCore core, bool rtl, bool wrap, bool diff, string text, int index) : IDisposable
    {
        internal readonly EditorBaseControl View = (EditorBaseControl)((Grid)core.Content).Children[0];
        internal Editor Native => View.Editor;
        internal double Scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        internal int DpiEvents, GeometryEvents, Paints;
        internal long Position;
        private string paintFailure;
        private bool armed;
        private double paintScale, geometryScale;
        internal double Offset => wrap ? 0 : 48;

        internal void Observe()
        {
            AutomationProperties.SetAutomationId(View, "DpiEditor" + index);
            Position = Native.PositionFromLine(TargetLine) + GlyphColumn * (rtl ? 2 : 1);
            View.DpiChanged += DpiChanged;
            View.ViewportGeometryChanged += GeometryChanged;
            Native.Painted += Painted;
        }

        private void DpiChanged(object sender, double scale) { Scale = scale; ++DpiEvents; }
        private void GeometryChanged(object sender, object args) { geometryScale = Scale; ++GeometryEvents; }
        private void Painted(Editor sender, PaintedEventArgs args)
        {
            ++Paints;
            paintScale = Scale;
            // Do not hit-test or request layout here: first-paint observations
            // must not repair the viewport that is being checked.
            if (armed && (Native.DocLineFromVisible(Native.FirstVisibleLine) != TargetLine ||
                Native.CurrentPos != Position || Native.Anchor != Position))
                paintFailure ??= $"Pane {index}: DPI repaint changed the document anchor or caret.";
        }

        internal void Arm()
        {
            DpiEvents = GeometryEvents = Paints = 0;
            paintScale = geometryScale = Scale;
            armed = true;
        }

        internal void PositionViewport()
        {
            Native.SetEmptySelection(Position);
            Native.CaretPeriod = 0;
            Native.CaretFore = 0xff00ff;
            if (diff) core.SetDiffViewport(TargetLine, Offset);
            else core.SetScrollViewerInitPosition(Offset, 0, TargetLine, 0);
        }

        internal void AppendState(StringBuilder result)
        {
            void Add(string key, object value) => result.Append(index).Append('_').Append(key).Append('=')
                .Append(Convert.ToString(value, CultureInfo.InvariantCulture)).Append('\n');
            Add("scale", Scale);
            Add("dpiEvents", DpiEvents);
            Add("geometryEvents", GeometryEvents);
            Add("paints", Paints);
            Add("paintScale", paintScale);
            Add("geometryScale", geometryScale);
            Add("position", Position);
            Add("character", TargetLine * (text.IndexOf('\r') + 1) + GlyphColumn);
            Add("rtl", rtl);
            Add("pending", View.IsViewportRestorePending);
        }

        internal void Validate(StringBuilder result)
        {
            if (paintFailure != null) throw new InvalidOperationException(paintFailure);
            var windowsScale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
            Require(Math.Abs(Scale - windowsScale) < 0.001, "native DPI notification agrees with the Windows display");
            if (OperatingSystem.IsWindowsVersionAtLeast(10, 0, 18362))
                Require(Math.Abs(View.XamlRoot.RasterizationScale - windowsScale) < 0.001,
                    "XAML agrees with the Windows display");
            Require(!View.IsViewportRestorePending && Native.DocLineFromVisible(Native.FirstVisibleLine) == TargetLine,
                "the viewport retains its document anchor");
            Require(Math.Abs(View.DisplayHorizontalOffset - Offset) <= 1 / Scale, "the leading-edge DIP offset is retained");
            Require(Native.CurrentPos == Position && Native.Anchor == Position && core.GetText() == text && !Native.CanUndo(),
                "DPI changes preserve text, selection and empty undo history");
            var image = Find<FrameworkElement>(View, "ImageTarget");
            var bar = Find<FrameworkElement>(View, "VerticalScrollBar");
            var imageOrigin = image.TransformToVisual(null).TransformPoint(default);
            var barOrigin = bar.TransformToVisual(null).TransformPoint(default);
            Require(rtl ? barOrigin.X < imageOrigin.X : barOrigin.X >= imageOrigin.X + image.ActualWidth - 1,
                "the vertical scrollbar remains on the reading-order trailing side");
            var gutter = View.GutterBounds;
            var reveal = (Border)((Grid)core.Content).Children[1];
            Require(Math.Abs(gutter.Width * Scale - Native.GetMarginWidthN(0)) <= 1 &&
                Math.Abs(reveal.ActualWidth - gutter.Width) <= 1 / Scale,
                "native gutter pixels and the XAML reveal width agree");
            var x = Native.PointXFromPosition(Position);
            var y = Native.PointYFromPosition(Position);
            Require(x > 0 && x < image.ActualWidth * Scale && y >= 0 && y < image.ActualHeight * Scale,
                "the observed caret remains inside the actual text surface");
            // Root DIPs -> screen pixels independently of TextRangeProvider.
            var bounds = Window.Current.CoreWindow.Bounds;
            void Add(string key, double value) => result.Append(index).Append('_').Append(key).Append('=')
                .Append(value.ToString(CultureInfo.InvariantCulture)).Append('\n');
            Add("caretX", (imageOrigin.X + bounds.X) * Scale + x);
            Add("caretY", (imageOrigin.Y + bounds.Y) * Scale + y);
            Add("lineHeight", Native.TextHeight(TargetLine));
            Add("fontWidth", Native.TextWidth(0, "MMMMMMMM"));
            Add("surfaceWidth", image.ActualWidth * Scale);
            Add("surfaceHeight", image.ActualHeight * Scale);
            Add("gutter", Native.GetMarginWidthN(0));
            Add("leadingOffset", View.DisplayHorizontalOffset);
            Add("wrapCount", Native.WrapCount(TargetLine));
        }

        private void Require(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException($"Pane {index}, RTL={rtl}, wrap={wrap}, diff={diff}: {message}.");
        }

        public void Dispose()
        {
            Native.Painted -= Painted;
            View.ViewportGeometryChanged -= GeometryChanged;
            View.DpiChanged -= DpiChanged;
        }
    }

    internal static async Task<DpiChangeFixture> CreateAsync(bool rtl, bool wrap, bool diff)
    {
        if (wrap && diff) throw new ArgumentException("Diff uses unwrapped display rows.");
        var fixture = new DpiChangeFixture();
        try { await fixture.InitializeAsync(rtl, wrap, diff); return fixture; }
        catch { fixture.Dispose(); throw; }
    }

    private async Task InitializeAsync(bool rtl, bool wrap, bool diff)
    {
        var line = new string(rtl ? 'א' : 'x', 96) + " ABC אבג العربية 123";
        var original = string.Join('\r', Enumerable.Repeat(line, 80));
        source = new TextEditorCore
        {
            Width = 600, Height = 300, FontSize = 16, DisplayLineNumbers = true,
            RightToLeftReadingOrder = rtl, TextWrapping = wrap ? TextWrapping.Wrap : TextWrapping.NoWrap,
            RequestedTheme = ElementTheme.Light
        };
        source.SetText(original);
        FrameworkElement content = source;
        if (diff)
        {
            using var baseline = await DocumentBaseline.FromTextAsync(original, Guid.NewGuid());
            var changed = original + " changed";
            source.SetText(changed);
            viewer = new SideBySideDiffViewer { Width = 760, Height = 300 };
            if (!await viewer.PrepareAsync(baseline, source, DocumentLanguages.PlainText, "dpi.txt", CancellationToken.None))
                throw new InvalidOperationException("DPI diff fixture failed to prepare.");
            var old = (TextEditorCore)viewer.FindName("OldEditor");
            var current = (TextEditorCore)viewer.FindName("NewEditor");
            panes.Add(new Pane(old, rtl, false, true, original, 0));
            panes.Add(new Pane(current, rtl, false, true, changed, 1));
            content = viewer;
        }
        else panes.Add(new Pane(source, rtl, wrap, false, original, 0));
        await PaintAsync(() => Window.Current.Content = content);
        foreach (var pane in panes) pane.Observe();
        await PaintAsync(() => { foreach (var pane in panes) pane.PositionViewport(); });
        // Begin first-paint assertions only after deliberate fixture scrolling.
        foreach (var pane in panes) pane.Arm();
    }

    private async Task PaintAsync(Action change)
    {
        var waits = panes.Select(pane => EditorTestLayout.AfterPaintAsync(pane.Native, () => { })).ToArray();
        change();
        await Task.WhenAll(waits);
    }

    internal string State()
    {
        var result = new StringBuilder("panes=" + panes.Count + "\n");
        result.Append("windowsScale=").Append(DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel
            .ToString(CultureInfo.InvariantCulture)).Append('\n');
        foreach (var pane in panes) pane.AppendState(result);
        return result.ToString();
    }

    internal string Validate()
    {
        var result = new StringBuilder(State());
        foreach (var pane in panes) pane.Validate(result);
        return result.ToString();
    }

    private static T Find<T>(DependencyObject root, string name) where T : FrameworkElement
    {
        if (root is T match && match.Name == name) return match;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); ++i)
        {
            var found = Find<T>(VisualTreeHelper.GetChild(root, i), name);
            if (found != null) return found;
        }
        return null;
    }

    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        foreach (var pane in panes) pane.Dispose();
        Window.Current.Content = previous;
        viewer?.Dispose();
        source?.Dispose();
    }
}
