// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsWorkspaceTests;
using Windows.Storage;
using Windows.Storage.Streams;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Markup;
using Windows.UI.Xaml.Media;
using Windows.UI.ViewManagement;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    internal static async Task RunPixelGeometryAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "pixel-geometry.md", Encoding.UTF8.GetBytes("MMMM"));
        var core = editor.TestNativeCore;
        var nativeView = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
        var sourceTarget = Descendants(nativeView).OfType<FrameworkElement>().Single(element => element.Name == "ImageTarget");
        var rail = Named<Notepads.Controls.SetsView>(host.Page, "Sets");
        var originalBounds = Window.Current.CoreWindow.Bounds;
        var originalZoom = core.GetFontZoomFactor();
        var scale = sourceTarget.XamlRoot.RasterizationScale;
        try
        {
            foreach (var width in new[] { 1100, 1101, 1099 })
            {
                Check(ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(width, 760)),
                    "The owned pixel-geometry fixture admits the requested window width: " + width);
                await UntilAsync(() => Math.Abs(Window.Current.CoreWindow.Bounds.Width - width) < 0.6 &&
                    nativeView.IsLoaded && !nativeView.IsViewportRestorePending,
                    "The real window/source viewport settles at the requested width: " + width);
                await TabDragTestHost.FrameAsync();
                CheckPixelTarget(sourceTarget, "source before preview, window=" + width);

                editor.ShowHideContentPreview();
                var extension = await CurrentAsync(editor);
                await TabDragTestHost.FrameAsync();
                var previewTarget = Descendants(extension.PreviewView).OfType<FrameworkElement>()
                    .Single(element => element.Name == "SurfaceTarget");
                CheckPixelTarget(sourceTarget, "source equal split, window=" + width);
                CheckPixelTarget(previewTarget, "preview equal split, window=" + width);

                // Exercise the actual columns that the production splitter edits.
                editor.TestSetMarkdownSplitWeights(2, 3);
                await TabDragTestHost.FrameAsync();
                await CurrentAsync(editor);
                CheckPixelTarget(sourceTarget, "source weighted split, window=" + width);
                CheckPixelTarget(previewTarget, "preview weighted split, window=" + width);
                editor.ShowHideContentPreview();
                await extension.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
                await TabDragTestHost.FrameAsync();
                CheckPixelTarget(sourceTarget, "source after preview close, window=" + width);
            }
            Check(editor.UseLayoutRounding && sourceTarget.UseLayoutRounding && !rail.UseLayoutRounding,
                "Pixel rounding is inherited by the editor body while the actual tab rail retains its existing rounding policy.");

            editor.ShowHideContentPreview();
            var active = await CurrentAsync(editor);
            var preview = active.PreviewView;
            core.SetFontZoomFactor(100);
            await TabDragTestHost.FrameAsync();
            await CurrentAsync(editor);
            var baseSize = core.FontSize;
            var baseline = PreviewBodyMetrics(preview);
            var generation = preview.ThemeGeneration;
            var layout = preview.GetViewportAnchor().LayoutGeneration;
            core.FontSize = baseSize * 1.5;
            await WaitForFontLayoutAsync(preview, active.SourceSession, generation, layout);
            var middle = PreviewBodyMetrics(preview);
            CheckMetricScale(baseline, middle, core.FontSize / baseSize, "direct Core.FontSize at 150%");

            generation = preview.ThemeGeneration;
            layout = preview.GetViewportAnchor().LayoutGeneration;
            core.SetFontZoomFactor(200);
            await WaitForFontLayoutAsync(preview, active.SourceSession, generation, layout);
            var doubled = PreviewBodyMetrics(preview);
            CheckMetricScale(baseline, doubled, core.FontSize / baseSize, "real editor 200% zoom");
            Check(editor.GetText() == "MMMM" && active.SourceSession.IsCurrent,
                "Pixel geometry and font relayout preserve the actual native source and accepted revision.");
            log.AppendLine($"PASS: Markdown.PixelGeometryZoom — actual RasterizationScale={scale}; widths 1100/1101/1099, open/equal/weighted/close have integer physical surface bounds/origins; body rounds and tab rail does not; real preview text metrics width={baseline.Width}/{middle.Width}/{doubled.Width}, height={baseline.Height}/{middle.Height}/{doubled.Height} apply source font scaling once. Other DPI settings were not exercised by this case.");
        }
        finally
        {
            core.SetFontZoomFactor(originalZoom);
            ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(originalBounds.Width, originalBounds.Height));
        }
    }

    private static void CheckPixelTarget(FrameworkElement target, string label)
    {
        var scale = target.XamlRoot.RasterizationScale;
        var transform = target.TransformToVisual(null);
        var origin = transform.TransformPoint(default);
        var x = transform.TransformPoint(new Windows.Foundation.Point(1, 0));
        var y = transform.TransformPoint(new Windows.Foundation.Point(0, 1));
        var width = target.ActualWidth * scale;
        var height = target.ActualHeight * scale;
        var left = origin.X * scale;
        var top = origin.Y * scale;
        bool Integer(double value) => double.IsFinite(value) && Math.Abs(value - Math.Round(value)) < 0.001;
        Check(width > 0 && height > 0 && Integer(width) && Integer(height) && Integer(left) && Integer(top),
            $"Actual bitmap target occupies whole physical pixels: {label}; scale={scale}, size={width}x{height}, origin={left},{top}.");
        Check(Math.Abs(x.X - origin.X - 1) < 0.00001 && Math.Abs(x.Y - origin.Y) < 0.00001 &&
            Math.Abs(y.X - origin.X) < 0.00001 && Math.Abs(y.Y - origin.Y - 1) < 0.00001,
            "The bitmap target has no extra inherited scale/skew transform: " + label);
    }

    // Candidate/40-only entry point. Keep the frozen 39 RunAsync baseline unchanged.
    internal static async Task RunViewportTemplateGeometryAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var root = host.Page.Content as Panel;
        Check(root != null, "The owned workspace can host a standalone native Markdown control.");
        var placement = new Grid { UseLayoutRounding = false };
        var preview = new MarkdownView
        {
            UseLayoutRounding = false, Margin = new Thickness(0.5, 0.5, 0, 0), Width = 0, Height = 0,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        placement.Children.Add(preview);
        root.Children.Add(placement);
        var originalBounds = Window.Current.CoreWindow.Bounds;
        var document = Guid.NewGuid();
        const string original = "MMMM [viewport link](https://example.com/)";
        const string replacement = "MMMM replacement accepted after the valid template returns.";
        try
        {
            await UntilAsync(() => preview.IsLoaded, "The independent zero-size Markdown control loads.");
            preview.ApplyTemplate();
            var defaultTemplate = preview.Template;
            Check(defaultTemplate != null, "The actual default native control template is available for restoration.");
            Check(Math.Abs(preview.XamlRoot.RasterizationScale - 1) < 0.001,
                "This fractional-host case requires actual 1x DPI; it does not change system DPI or qualify other scales.");
            var waiting = await preview.SetDocumentAsync(document, 1, Encoding.UTF8.GetBytes(original))
                .AsTask().WaitAsync(TimeSpan.FromSeconds(10));
            var waitingSource = preview.GetSourceState();
            Check(waiting.Status == DocumentUpdateStatus.NotLoaded && !waiting.Published && waiting.ErrorCode == 0 && !waitingSource.HasSource,
                "WaitingLayout at zero size produces a bounded NotLoaded receipt without claiming source acceptance or presentation.");
            log.AppendLine($"OBS: Markdown.ViewportTemplate WaitingLayout receipt={waiting.Status}, published={waiting.Published}, sourceKnown={waitingSource.StateKnown}, hasSource={waitingSource.HasSource}.");

            var fractional = false;
            foreach (var width in new[] { 1101, 1099 })
            {
                Check(ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(width, 760)),
                    "The owned fractional-host window admits width " + width);
                preview.Width = (width - 20) / 2.0;
                preview.Height = 520.5;
                await UntilAsync(() => Math.Abs(Window.Current.CoreWindow.Bounds.Width - width) < 0.6 &&
                    ViewportTemplateReady(preview, document, 1), "The zero-size source automatically publishes after a real arrange.");
                await TabDragTestHost.FrameAsync();
                fractional |= CheckViewportTemplateGeometry(preview, log, "default width=" + width);
            }
            Check(fractional && !preview.UseLayoutRounding && preview.Margin.Left == 0.5 && preview.Margin.Top == 0.5,
                "An actually fractional ScrollContentPresenter origin was exercised with the native control's rounding disabled.");

            preview.Template = LoadViewportTemplate(nested: false);
            preview.ApplyTemplate();
            try
            {
                await UntilAsync(() => ViewportTemplateReady(preview, document, 1),
                    "A valid public ControlTemplate with nonzero ScrollViewer padding publishes the accepted source.");
            }
            catch (TimeoutException)
            {
                LogViewportDiagnostics(log, preview, "padded template wait timed out", includeParts: true);
                throw;
            }
            await TabDragTestHost.FrameAsync();
            var insetScroll = Descendants(preview).OfType<ScrollViewer>().Single(element => element.Name == "PreviewScrollViewer");
            Check(insetScroll.Padding.Equals(new Thickness(7, 11, 9, 13)),
                "The applied native ScrollViewer actually has the custom nonzero inset.");
            CheckViewportTemplateGeometry(preview, log, "custom padding=7,11,9,13");
            var selection = preview.GetSelection();
            preview.SelectRange(selection.DocumentId, selection.Revision, selection.TextGeneration, 0, 4);
            Check(preview.GetSelection().IsAvailable && preview.GetSelection().Length == 4 && PreviewBodyMetrics(preview).Width > 0,
                "The inset template keeps the real native MMMM text range, selection and visible geometry available.");

            preview.Template = LoadViewportTemplate(nested: true);
            preview.ApplyTemplate();
            await TabDragTestHost.FrameAsync();
            var rejected = await preview.SetDocumentAsync(document, 2, Encoding.UTF8.GetBytes(replacement))
                .AsTask().WaitAsync(TimeSpan.FromSeconds(10));
            var rejectedSource = preview.GetSourceState();
            Check(rejected.Status == DocumentUpdateStatus.Failed && unchecked((uint)rejected.ErrorCode) == 0x80070057 && !rejected.Published,
                "An old scrolling/nested SurfaceTarget template reaches Failed/E_INVALIDARG rather than leaving its receipt pending.");
            Check(!(rejectedSource.HasSource && rejectedSource.DocumentId == document && rejectedSource.Revision == 2) &&
                !preview.GetViewportAnchor().IsAvailable && !preview.GetSelection().IsAvailable,
                "UnsupportedTemplate does not claim the rejected source is accepted or expose stale reading/selection permissions.");
            log.AppendLine($"OBS: Markdown.ViewportTemplate Unsupported receipt={rejected.Status}, error=0x{unchecked((uint)rejected.ErrorCode):X8}, published={rejected.Published}, sourceKnown={rejectedSource.StateKnown}, hasSource={rejectedSource.HasSource}, sourceRevision={rejectedSource.Revision}.");

            preview.Template = defaultTemplate;
            preview.ApplyTemplate();
            var automaticDeadline = DateTime.UtcNow.AddMilliseconds(2500);
            while (!ViewportTemplateReady(preview, document, 2, requireSourceSync: false) && DateTime.UtcNow < automaticDeadline) await Task.Delay(25);
            var recovery = "automatic template recovery";
            if (!ViewportTemplateReady(preview, document, 2, requireSourceSync: false))
            {
                recovery = "explicit public Refresh after observing no automatic recovery for 2500ms";
                log.AppendLine("OBS: Markdown.ViewportTemplate automatic recovery was not established; invoking the separate explicit Refresh path.");
                preview.Refresh();
                await UntilAsync(() => ViewportTemplateReady(preview, document, 2, requireSourceSync: false),
                    "Explicit Refresh restores the accepted pending source after a supported default template returns.", 25000);
            }
            await TabDragTestHost.FrameAsync();
            Check(preview.DisplayedText.Contains(replacement, StringComparison.Ordinal),
                "Recovery publishes the actual replacement source without a host adapter, new submission or full-resync workaround.");
            CheckViewportTemplateGeometry(preview, log, "restored default");
            var recoveredSource = preview.GetSourceState();
            Check(recoveredSource.StateKnown && recoveredSource.HasSource && recoveredSource.DocumentId == document &&
                recoveredSource.Revision == 2 && recoveredSource.NeedsResync &&
                rejected.Status == DocumentUpdateStatus.Failed && unchecked((uint)rejected.ErrorCode) == 0x80070057 && !rejected.Published,
                "Template recovery presents retained revision2 while preserving the failed receipt and explicit full-sync requirement.");
            log.AppendLine($"OBS: Markdown.ViewportTemplate retained source restored at revision={recoveredSource.Revision}, needsResync={recoveredSource.NeedsResync}; original failed receipt unchanged.");
            var synchronized = await preview.SetDocumentAsync(document, 3, Encoding.UTF8.GetBytes(replacement))
                .AsTask().WaitAsync(TimeSpan.FromSeconds(10));
            Check(synchronized.Status == DocumentUpdateStatus.Ready && synchronized.Published,
                "A separate successful monotonic full-sync revision3 restores the edit channel after template recovery was independently proven.");
            await UntilAsync(() => ViewportTemplateReady(preview, document, 3),
                "Revision3 clears NeedsResync only after the explicit full-source synchronization succeeds.");
            log.AppendLine($"PASS: Markdown.ViewportTemplateGeometry — actual scale={preview.XamlRoot.RasterizationScale}; fractional host, odd widths 1101/1099, default/padded overlay geometry, bounded WaitingLayout/Unsupported receipts and {recovery}. Active-request inner ScrollViewer.Template replacement and non-100% DPI were not exercised.");
        }
        finally
        {
            preview.Cancel();
            root.Children.Remove(placement);
            ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(originalBounds.Width, originalBounds.Height));
        }
    }

    private static bool ViewportTemplateReady(MarkdownView preview, Guid document, ulong revision, bool requireSourceSync = true)
    {
        var source = preview.GetSourceState();
        return source.StateKnown && source.HasSource && (!requireSourceSync || !source.NeedsResync) && source.DocumentId == document && source.Revision == revision &&
            preview.DisplayedDocumentId == document && preview.DisplayedRevision == revision && preview.IsSurfaceReady &&
            string.IsNullOrEmpty(preview.RenderingError) && HasReadingIdentity(preview, document, revision);
    }

    private static void LogViewportDiagnostics(StringBuilder log, MarkdownView preview, string label,
        MarkdownSelection selection = null, bool includeParts = false)
    {
        // Read-only diagnostics must never replace the original test exception.
        string Read(Func<string> value)
        {
            try { return value(); }
            catch (Exception error) { return $"<read failed: {error.GetType().Name}, 0x{unchecked((uint)error.HResult):X8}, {error.Message}>"; }
        }
        log.AppendLine("DIAG: Markdown.Viewport " + label + "; " + Read(() =>
            $"loaded={preview.IsLoaded}, size={preview.ActualWidth}x{preview.ActualHeight}, focus={preview.FocusState}, " +
            $"surfaceReady={preview.IsSurfaceReady}, renderingError=[{preview.RenderingError}], " +
            $"displayed={preview.DisplayedDocumentId}/{preview.DisplayedRevision}/{preview.DisplayedGeneration}"));
        log.AppendLine("DIAG: Markdown.Viewport " + label + "; selection=" + Read(() =>
        {
            var current = selection ?? preview.GetSelection();
            return $"available={current.IsAvailable}, identity={current.DocumentId}/{current.Revision}/{current.TextGeneration}, range={current.Start}+{current.Length}";
        }) + "; anchor=" + Read(() =>
        {
            var anchor = preview.GetViewportAnchor();
            return $"available={anchor.IsAvailable}, identity={anchor.DocumentId}/{anchor.Revision}/{anchor.TextGeneration}, " +
                $"layout={anchor.LayoutGeneration}, main={anchor.PreviewUtf16Offset}, within={anchor.OffsetWithinLine}";
        }) + "; source=" + Read(() =>
        {
            var source = preview.GetSourceState();
            return $"known={source.StateKnown}, hasSource={source.HasSource}, needsResync={source.NeedsResync}, identity={source.DocumentId}/{source.Revision}";
        }));
        if (!includeParts) return;
        log.AppendLine("DIAG: Markdown.Viewport " + label + "; actual parts=" + Read(() =>
        {
            var parts = Descendants(preview).OfType<FrameworkElement>().Where(element =>
                element.Name is "PreviewScrollViewer" or "DocumentExtent" or "ViewportOverlay" or "SurfaceTarget").ToArray();
            return $"count={parts.Length}; " + string.Join("; ", parts.Select(element =>
                $"{element.Name}:{element.GetType().Name}, loaded={element.IsLoaded}, size={element.ActualWidth}x{element.ActualHeight}"));
        }));
        log.AppendLine("DIAG: Markdown.Viewport " + label + "; actual SCPs=" + Read(() =>
        {
            var presenters = Descendants(preview).OfType<ScrollContentPresenter>().ToArray();
            return $"count={presenters.Length}; " + string.Join("; ", presenters.Select(element =>
                $"{element.Name}, loaded={element.IsLoaded}, size={element.ActualWidth}x{element.ActualHeight}, content={element.Content?.GetType().Name}"));
        }) + "; scroll viewports=" + Read(() => string.Join("; ", Descendants(preview).OfType<ScrollViewer>().Select(scroll =>
            $"{scroll.Name}, loaded={scroll.IsLoaded}, actual={scroll.ActualWidth}x{scroll.ActualHeight}, viewport={scroll.ViewportWidth}x{scroll.ViewportHeight}, padding={scroll.Padding}"))));
    }

    private static bool CheckViewportTemplateGeometry(MarkdownView preview, StringBuilder log, string label)
    {
        var scroll = Descendants(preview).OfType<ScrollViewer>().Single(element => element.Name == "PreviewScrollViewer");
        var extent = Descendants(preview).OfType<Canvas>().Single(element => element.Name == "DocumentExtent");
        var overlay = Descendants(preview).OfType<Canvas>().Single(element => element.Name == "ViewportOverlay");
        var target = Descendants(preview).OfType<Border>().Single(element => element.Name == "SurfaceTarget");
        var presenter = Descendants(scroll).OfType<ScrollContentPresenter>().Single(element => Equals(element.Content, extent));
        bool Within(DependencyObject child, DependencyObject ancestor)
        {
            for (var current = child; current != null; current = VisualTreeHelper.GetParent(current))
                if (Equals(current, ancestor)) return true;
            return false;
        }
        Check(Equals(scroll.Content, extent) && Equals(VisualTreeHelper.GetParent(target), overlay) &&
            !Within(overlay, scroll) && !Within(target, extent), "Actual template parts keep the surface outside the scrolling extent: " + label);
        CheckPixelTarget(target, "standalone fractional viewport: " + label);
        var scale = preview.XamlRoot.RasterizationScale;
        var at = presenter.TransformToVisual(null).TransformPoint(default);
        var surface = target.TransformToVisual(null).TransformPoint(default);
        var left = Math.Floor(at.X * scale);
        var top = Math.Floor(at.Y * scale);
        var right = Math.Ceiling((at.X + scroll.ViewportWidth) * scale);
        var bottom = Math.Ceiling((at.Y + scroll.ViewportHeight) * scale);
        Check(Math.Abs(surface.X * scale - left) < 0.001 && Math.Abs(surface.Y * scale - top) < 0.001 &&
            Math.Abs(target.ActualWidth * scale - (right - left)) < 0.001 &&
            Math.Abs(target.ActualHeight * scale - (bottom - top)) < 0.001,
            "The actual bitmap target covers the actual presenter viewport's floor/ceil physical rectangle: " + label);
        var phaseX = at.X * scale - left;
        var phaseY = at.Y * scale - top;
        log.AppendLine($"OBS: Markdown.ViewportTemplate {label}; scale={scale}; SCP root={at.X},{at.Y}, phase={phaseX},{phaseY}, actualSize={presenter.ActualWidth}x{presenter.ActualHeight}, viewport={scroll.ViewportWidth}x{scroll.ViewportHeight}; surface root={surface.X},{surface.Y}, actualSize={target.ActualWidth}x{target.ActualHeight}.");
        return phaseX > 0.001 || phaseY > 0.001;
    }

    private static ControlTemplate LoadViewportTemplate(bool nested)
    {
        var content = nested ?
            "<Canvas x:Name='DocumentExtent'><Canvas x:Name='ViewportOverlay'><Border x:Name='SurfaceTarget' Background='Transparent'/></Canvas></Canvas>" :
            "<Canvas x:Name='DocumentExtent' Background='Transparent'/>";
        var overlay = nested ? "" :
            "<Canvas x:Name='ViewportOverlay' IsHitTestVisible='False' UseLayoutRounding='True'><Border x:Name='SurfaceTarget' IsHitTestVisible='False' UseLayoutRounding='True' Background='Transparent'/></Canvas>";
        return (ControlTemplate)XamlReader.Load("<ControlTemplate xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation' " +
            "xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml' xmlns:local='using:Notepads.Markdown' TargetType='local:MarkdownView'>" +
            "<Grid><ScrollViewer x:Name='PreviewScrollViewer' Padding='7,11,9,13' Background='Transparent' HorizontalScrollMode='Disabled' " +
            "HorizontalScrollBarVisibility='Disabled' VerticalScrollMode='Enabled' VerticalScrollBarVisibility='Auto' ZoomMode='Disabled' " +
            "HorizontalContentAlignment='Left' VerticalContentAlignment='Top' IsTabStop='False'>" + content + "</ScrollViewer>" + overlay + "</Grid></ControlTemplate>");
    }

    private static (double Width, double Height) PreviewBodyMetrics(MarkdownView preview)
    {
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var text = peer.GetPattern(PatternInterface.Text) as ITextProvider;
        Check(text != null, "The current preview exposes its actual native text geometry.");
        var body = text.DocumentRange.FindText("MMMM", false, false);
        Check(body != null, "The short unwrapped body is present in the native preview text range.");
        body.GetBoundingRectangles(out var rectangles);
        Check(rectangles.Length == 4 && rectangles[2] > 0 && rectangles[3] > 0,
            "The actual body range has one visible rectangle for an independent font-scale observation.");
        return (rectangles[2], rectangles[3]);
    }

    private static async Task WaitForFontLayoutAsync(MarkdownView preview, MarkdownSourceSession session, ulong theme, ulong layout)
    {
        await UntilAsync(() => session.IsCurrent && preview.ThemeGeneration > theme && preview.IsSurfaceReady &&
            string.IsNullOrEmpty(preview.RenderingError) && preview.GetViewportAnchor().IsAvailable &&
            preview.GetViewportAnchor().LayoutGeneration > layout,
            "The new source font reaches an actually published native preview layout generation.");
        await TabDragTestHost.FrameAsync();
    }

    private static void CheckMetricScale((double Width, double Height) before, (double Width, double Height) after, double factor, string label)
    {
        Check(Math.Abs(after.Width - before.Width * factor) <= 1.5 && Math.Abs(after.Height - before.Height * factor) <= 1.5,
            $"Actual preview body width/height scale once with {label}: factor={factor}, before={before}, after={after}.");
    }

}
