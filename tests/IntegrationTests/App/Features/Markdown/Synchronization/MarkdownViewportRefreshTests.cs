// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Controls.Markdown;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    internal static async Task RunViewportRefreshAsync(StringBuilder log)
    {
        await RunPaintedBottomFollowAsync(log);
        await RunBottomImageReflowAsync(log);
        await RunImageHeadingFollowAsync(log);
        await RunInitialFollowPaddingAsync(log);
        await RunCachedPreviewGeometryAsync(log);
        await RunFollowedTabViewportAsync(log);
        await RunEvictedTabRestoreAsync(log, follow: true);
        await RunEvictedTabRestoreAsync(log, follow: false);
        await RunCachedFirstFrameAsync(log, interruptGeometry: true);
        await RunCachedFirstFrameAsync(log, interruptGeometry: true, automaticRetry: true);
        await RunCachedFirstFrameAsync(log, interruptGeometry: false);
        await RunCacheReflowAsync(log);
    }

    internal static async Task RunCacheReflowAsync(StringBuilder log)
    {
        foreach (var kind in new[] { "no-op", "text-range", "resize", "image" })
            await RunCacheReflowAndAutomationAsync(log, kind);
    }

    private static async Task RunPaintedBottomFollowAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var intro = string.Join("\n\n", Enumerable.Range(1, 80).Select(i => $"## Heading {i:D3}\n\nIntroduction text."));
        var people = string.Join("\n", Enumerable.Range(1, 25).Select(i => $"    * [Translator {i:D2}](https://example.org/)"));
        var source = intro + "\n\n## Special Thanks\n\n* Localization Contributors:\n" + people +
            "\n\n## Stay tuned\n\n* [Discord](https://example.org/)\n";
        var editor = await OpenAsync(host, "painted-bottom.md", Encoding.UTF8.GetBytes(source));
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var sourceView = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        editor.Focus();
        await UntilAsync(() => sourceView.VerticalScrollExtent > 0 && preview.VerticalScrollProgress >= 0,
            "The editor and preview have real scroll extents.");
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        bool LastTextVisible() => reading.GetVisibleRanges().Any(range => range.GetText(-1).Contains("Discord", StringComparison.Ordinal));

        native.DocumentEnd();
        await UntilAsync(() => sourceView.VerticalScrollProgress == 1 && preview.VerticalScrollProgress == 1 &&
            LastTextVisible(), "The real Ctrl+End command reaches the bottom in both panes.");
        var bottom = sourceView.GetSourceViewport();
        log.AppendLine($"OBS: Markdown.PaintedBottom CtrlEnd source={sourceView.VerticalScrollProgress:R}, row={native.FirstVisibleLine}, preview={preview.VerticalScrollProgress:R}, last={LastTextVisible()}.");
        Check(native.CurrentPos == native.Length, "The real Ctrl+End command reaches the last source line.");

        Check(sourceView.TrySetVerticalScrollProgress(0.6), "Move away from the end before the thumb-position case.");
        await UntilAsync(() => preview.VerticalScrollProgress < 0.9, "The interior position reaches the preview.");
        double extent = sourceView.VerticalScrollExtent;
        double scale = preview.XamlRoot.RasterizationScale;
        var scroll = Descendants(preview).OfType<ScrollViewer>().Single(item => item.Name == "PreviewScrollViewer");
        double interiorOffset = scroll.VerticalOffset;
        double halfway = 1 - Math.Min(extent, sourceView.ActualHeight) / (2 * extent);
        Check(sourceView.TrySetVerticalScrollProgress(halfway), "The source enters the final viewport of scroll travel.");
        await UntilAsync(() => scroll.VerticalOffset > interiorOffset && preview.VerticalScrollProgress < 1,
            "The preview moves through the final viewport without reaching the end early.");
        double halfwayOffset = scroll.VerticalOffset;

        double twoPixelsBeforeEnd = 1 - 2 / scale / extent;
        Check(sourceView.TrySetVerticalScrollProgress(twoPixelsBeforeEnd), "The source approaches the last two physical pixels.");
        await UntilAsync(() => scroll.VerticalOffset > halfwayOffset && preview.VerticalScrollProgress < 1,
            "The preview advances again and remains short of the physical end.");
        double remainingPixels = (scroll.ScrollableHeight - scroll.VerticalOffset) * scale;
        log.AppendLine($"OBS: Markdown.PaintedBottom two source pixels from end, preview remaining={remainingPixels:F2} pixels.");
        Check(remainingPixels <= 12, "The last two source pixels must not cause a large final preview jump.");

        double lastPixel = 1 - 0.2 / scale / extent;
        Check(sourceView.TrySetVerticalScrollProgress(lastPixel), "The source scrolls within its last physical pixel.");
        await UntilAsync(() => sourceView.VerticalScrollProgress == 1 && preview.VerticalScrollProgress == 1 &&
            LastTextVisible(), "The last painted source pixel also drives the preview to its actual end.");
        var painted = sourceView.GetSourceViewport();
        log.AppendLine($"OBS: Markdown.PaintedBottom lastPixel source={sourceView.VerticalScrollProgress:R}, row={native.FirstVisibleLine}, preview={preview.VerticalScrollProgress:R}, last={LastTextVisible()}.");
        Check(bottom.IsAvailable && painted.IsAvailable && painted.DisplayRow == bottom.DisplayRow &&
            painted.Fraction == bottom.Fraction, "The fractional source position paints the same viewport as the exact end.");

        Check(sourceView.TrySetVerticalScrollProgress(1 - 2 / preview.XamlRoot.RasterizationScale / extent),
            "Scroll visibly away from the source endpoint.");
        await UntilAsync(() => preview.VerticalScrollProgress is > 0 and < 1,
            "Leaving the endpoint resumes content alignment without pinning the preview.");
        log.AppendLine("PASS: Markdown.PaintedBottom — source and preview reach the end together at the final physical pixel.");
    }

    private static async Task RunBottomImageReflowAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var image = await host.Folder.CreateFileAsync("tail.svg", Windows.Storage.CreationCollisionOption.FailIfExists);
        const string small = "<svg xmlns='http://www.w3.org/2000/svg' width='200' height='24'><rect width='200' height='24' fill='blue'/></svg>";
        const string tall = "<svg xmlns='http://www.w3.org/2000/svg' width='200' height='620'><rect width='200' height='620' fill='blue'/></svg>";
        await Windows.Storage.FileIO.WriteTextAsync(image, small);
        var body = string.Join("\n\n", Enumerable.Range(1, 100).Select(i => $"Paragraph {i:D3}"));
        var markdown = body + "\n\n![tail image](tail.svg)\n\nFinal reflow marker.";
        var editor = await OpenAsync(host, "bottom-image-reflow.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        var source = (WinUIEditor.EditorBaseControl)((Grid)editor.TestNativeCore.Content).Children[0];
        var scroll = Descendants(preview).OfType<ScrollViewer>().Single(item => item.Name == "PreviewScrollViewer");
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        bool MarkerVisible() => reading.GetVisibleRanges().Any(range => range.GetText(-1).Contains("Final reflow marker.", StringComparison.Ordinal));

        await UntilAsync(() => source.VerticalScrollExtent > 0 && scroll.ScrollableHeight > 0, "Both panes can scroll.");
        extension.TestBeginViewportScroll(preview: false);
        Check(source.TrySetVerticalScrollProgress(1), "The source owns the scroll to the bottom.");
        await UntilAsync(() => source.VerticalScrollProgress == 1 && preview.VerticalScrollProgress == 1 &&
            ImageState(preview, "tail image", true) && MarkerVisible(), "The small tail image and final text are visible at the bottom.", 25000);
        double oldExtent = scroll.ScrollableHeight;

        await Windows.Storage.FileIO.WriteTextAsync(image, tall);
        extension.TestRestoreSmoothImageResources();
        await UntilAsync(() => source.VerticalScrollProgress == 1 && preview.VerticalScrollProgress == 1 &&
            scroll.ScrollableHeight > oldExtent + 500 && ImageState(preview, "tail image", true) && MarkerVisible(),
            "The preview follows the source to the new bottom after the image grows, without another scroll.", 25000);
        log.AppendLine($"PASS: Markdown.BottomImageReflow — extent {oldExtent:F1} -> {scroll.ScrollableHeight:F1} DIP; both panes stay at the bottom and the final text is visible.");
    }

    private static async Task RunImageHeadingFollowAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var image = await host.Folder.CreateFileAsync("tall.svg", Windows.Storage.CreationCollisionOption.FailIfExists);
        await Windows.Storage.FileIO.WriteTextAsync(image,
            "<svg xmlns='http://www.w3.org/2000/svg' width='390' height='680'><rect width='390' height='680' fill='#447799'/></svg>");
        var before = string.Join("\n\n", Enumerable.Range(1, 24).Select(index => $"Introduction paragraph {index:D2}."));
        var images = string.Join("\n", Enumerable.Range(1, 4).Select(index => $"![Screenshot {index}](tall.svg)"));
        var after = string.Join("\n\n", Enumerable.Range(1, 55).Select(index => $"Shortcut description {index:D2}."));
        var text = before + "\n\n" + images + "\n\n## Shortcuts\n\n" + after;
        var editor = await OpenAsync(host, "image-heading-follow.md", Encoding.UTF8.GetBytes(text));
        var native = editor.TestNativeCore.TestFeatureNative;
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        // Loading is viewport-admitted. Visit every image before testing the
        // final heading position rather than waiting for far-offscreen work.
        for (int index = 1; index <= 4; index++)
        {
            var imageByte = Encoding.UTF8.GetByteCount(text.AsSpan(0,
                text.IndexOf($"![Screenshot {index}]", StringComparison.Ordinal)));
            extension.TestBeginViewportScroll(preview: false);
            native.FirstVisibleLine = native.VisibleFromDocLine(native.LineFromPosition(imageByte));
            await UntilAsync(() => ImageState(preview, $"Screenshot {index}", true),
                $"Screenshot {index} renders after its source row enters the actual viewport.");
        }
        Check(Enumerable.Range(1, 4).All(index => ImageState(preview, $"Screenshot {index}", true)),
            "All four tall images remain Ready after visiting each one.");
        await UntilAsync(() => ImageState(preview, "Screenshot 4", true), "The four tall images have rendered.");
        var titleByte = Encoding.UTF8.GetByteCount(text.AsSpan(0, text.IndexOf("## Shortcuts", StringComparison.Ordinal)));
        var row = native.VisibleFromDocLine(native.LineFromPosition(titleByte));
        var titleOffset = (ulong)preview.DisplayedText.IndexOf("Shortcuts", StringComparison.Ordinal);
        Check(row > 0 && titleOffset < (ulong)preview.DisplayedText.Length, "Both panes have the Shortcuts heading.");

        extension.TestBeginViewportScroll(preview: false);
        native.FirstVisibleLine = row;
        await UntilAsync(() => native.FirstVisibleLine == row && preview.GetViewportAnchor().PreviewUtf16Offset > 0,
            "The source scrolls to Shortcuts and the preview publishes a new position.");
        await Task.Delay(450);
        var anchor = preview.GetViewportAnchor();
        log.AppendLine($"OBS: Markdown.ImageHeadingFollow sourceRow={native.FirstVisibleLine}, titleRow={row}, previewTop={anchor.PreviewUtf16Offset}, title={titleOffset}.");
        Check(anchor.PreviewUtf16Offset == titleOffset,
            "The preview reaches Shortcuts when the heading is the first visible source row, after four tall images.");

        native.FirstVisibleLine = row + 16;
        await UntilAsync(() => preview.GetViewportAnchor().PreviewUtf16Offset > titleOffset,
            "Scrolling the source past Shortcuts advances the preview.");
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        reading.DocumentRange.FindText("Shortcuts", false, false).ScrollIntoView(true);
        await UntilAsync(() => preview.GetViewportAnchor().PreviewUtf16Offset == titleOffset &&
            Math.Abs(native.FirstVisibleLine - row) <= 1, "Reverse scrolling returns both panes to Shortcuts.");
        log.AppendLine("PASS: Markdown.ImageHeadingFollow — both panes reach the heading after four tall images.");
    }

    private static async Task RunCacheReflowAndAutomationAsync(StringBuilder log, string kind, bool previewLed = false)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        Windows.Storage.StorageFile image = null;
        if (kind == "image")
        {
            image = await host.Folder.CreateFileAsync("top.svg", Windows.Storage.CreationCollisionOption.FailIfExists);
            await Windows.Storage.FileIO.WriteTextAsync(image, "<svg xmlns='http://www.w3.org/2000/svg' width='220' height='22'><rect width='220' height='22' fill='blue'/></svg>");
        }
        var body = string.Join("\n\n", Enumerable.Range(1, 100).Select(index =>
            $"## Section {index:D3}\n\n" + string.Join(" ", Enumerable.Repeat("This paragraph wraps differently as the preview width changes.", 5))));
        var editor = await OpenAsync(host, $"cache-edge-{kind}.md", Encoding.UTF8.GetBytes(
            image == null ? body : "![header image](top.svg)\n\n" + body));
        var original = host.Core.GetSelectedTab();
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        var source = (WinUIEditor.EditorBaseControl)((Grid)editor.TestNativeCore.Content).Children[0];
        await UntilAsync(() => preview.VerticalScrollProgress >= 0 && source.VerticalScrollProgress >= 0 &&
            (image == null || ImageState(preview, "header image", true)), "Both extents and the original resource are ready.");
        var scroll = Descendants(preview).OfType<ScrollViewer>().Single(item => item.Name == "PreviewScrollViewer");
        // The changed header remains above the held paragraph, within normal
        // near-viewport admission even after this fixture truly unloads the tab.
        var sourceProgress = image != null ? 0.02 : 0.55;
        extension.TestBeginViewportScroll(preview: false);
        Check(source.TrySetVerticalScrollProgress(sourceProgress), "The source scrolls to the retained paragraph.");
        await UntilAsync(() => Math.Abs(source.VerticalScrollProgress - sourceProgress) < 0.005 &&
            preview.GetViewportAnchor().PreviewUtf16Offset > 100, "The preview publishes the corresponding content.");
        if (kind == "no-op" || kind == "text-range")
        {
            Check(preview.TrySetVerticalScrollProgress(kind == "no-op" ? 1 : 0.25), "The preview can have its own restored position.");
            await UntilAsync(() => Math.Abs(preview.VerticalScrollProgress - (kind == "no-op" ? 1 : 0.25)) < 0.005 &&
                preview.GetViewportAnchor().PreviewUtf16Offset > 100, "The independent preview position settles.");
            await Task.Delay(200);
            Check(Math.Abs(source.VerticalScrollProgress - 0.55) < 0.005, "The original panes have different positions.");
        }

        if (previewLed)
        {
            Check(preview.Focus(FocusState.Programmatic), "The preview owns focus for the image-reflow variant.");
            await UntilAsync(() => !extension.TestSourceFindBusy, "The existing follow work retires before preview-driven scrolling.");
            extension.TestBeginViewportScroll(preview: true);
            var previewProgress = image != null ? 0.035 : 0.38;
            Check(preview.TrySetVerticalScrollProgress(previewProgress), "The preview admits an independent retained viewport.");
            await UntilAsync(() => Math.Abs(preview.VerticalScrollProgress - previewProgress) < 0.005 &&
                !extension.TestSourceFindBusy, "Both panes settle under preview viewport authority.");
        }
        var measuredBefore = preview.MeasuredHeight;
        var before = preview.GetViewportAnchor();
        var sourceBefore = source.VerticalScrollProgress;
        var previewBefore = preview.VerticalScrollProgress;
        double widthBefore = preview.ActualWidth;
        double yBefore = scroll.VerticalOffset;
        if (image != null)
        {
            log.AppendLine($"OBS: CacheImageAdmission sourceTarget={sourceProgress}, previewLed={previewLed}, oldY={yBefore:R}, viewport={scroll.ViewportHeight:R}.");
            Check(yBefore > 64 && yBefore < scroll.ViewportHeight - 64,
                "The old header is offscreen above the held paragraph but remains in normal near-viewport admission.");
        }
        for (int i = 1; i <= 9; i++)
        {
            var other = await OpenAsync(host, $"edge-extra-{i}.md", Encoding.UTF8.GetBytes("# Other"));
            other.ShowHideContentPreview();
            await CurrentAsync(other);
        }
        await UntilAsync(() => !preview.IsLoaded && preview.HasCachedDocument, "The original visual was evicted.");
        if (kind == "resize")
            editor.TestSetMarkdownSplitWeights(2, 1);
        if (image != null)
            await Windows.Storage.FileIO.WriteTextAsync(image, "<svg xmlns='http://www.w3.org/2000/svg' width='220' height='600'><rect width='220' height='600' fill='blue'/></svg>");

        host.Core.SwitchTo(original);
        try { await CurrentAsync(editor); }
        catch (Exception)
        {
            try
            {
                var state = preview.GetSourceState();
                var session = extension.SourceSession;
                log.AppendLine($"OBS: CacheReturnTimeout kind={kind}; loaded={editor.IsLoaded}/{extension.IsLoaded}/{preview.IsLoaded}; visible={preview.Visibility}; enabled={extension.IsExtensionEnabled}; suspended={preview.IsCachedPresentationSuspended}; cached={preview.HasCachedDocument}; surface={preview.IsSurfaceReady}; error={preview.RenderingError}; session={session.Status}/{session.IsCurrent}/{session.AcceptedRevision}; accepted={state.StateKnown}/{state.HasSource}/{state.NeedsResync}/{state.DocumentId}/{state.Revision}; displayed={preview.DisplayedDocumentId}/{preview.DisplayedRevision}; sourceProgress={source.VerticalScrollProgress}; sourcePending={source.IsViewportRestorePending}.");
                LogViewportDiagnostics(log, preview, "cached return timed out: " + kind, includeParts: true);
            }
            catch (Exception diagnosticError)
            {
                log.AppendLine("OBS: CacheReturnTimeout diagnostic failed: " + diagnosticError.Message);
            }
            throw;
        }
        await UntilAsync(() => (kind != "resize" || preview.ActualWidth < widthBefore - 40) &&
            (image == null || ImageState(preview, "header image", true)), "The new preview width or image is ready.");
        await Task.Delay(700);
        var restored = preview.GetViewportAnchor();
        var sourceRestored = source.VerticalScrollProgress;
        var previewRestored = preview.VerticalScrollProgress;
        var yRestored = scroll.VerticalOffset;
        if (kind == "no-op" || kind == "text-range")
        {
            Check(preview.Focus(FocusState.Programmatic), "The restored preview owns accessibility focus.");
            await Task.Delay(150);
            var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
            int starts = 0;
            preview.VerticalScrollStarting += (_, _) => starts++;
            if (kind == "no-op")
            {
                var provider = (IScrollProvider)peer.GetPattern(PatternInterface.Scroll);
                provider.SetScrollPercent(-1, provider.VerticalScrollPercent);
            }
            else
            {
                var provider = (ITextProvider)peer.GetPattern(PatternInterface.Text);
                var end = provider.DocumentRange.FindText("Section 100", false, false);
                Check(end != null, "The range provider locates the last heading.");
                end.ScrollIntoView(true);
            }
            await Task.Delay(600);
            Check(kind == "no-op"
                ? starts == 0 && Math.Abs(source.VerticalScrollProgress - sourceBefore) < 0.005 && preview.VerticalScrollProgress == 1
                : starts == 1 && source.VerticalScrollProgress == 1 && preview.VerticalScrollProgress == 1,
                "Automation changes the peer viewport only for a real preview move.");
            log.AppendLine($"OBS: ReviewAutomation kind={kind}; starts={starts}, before source/preview={sourceBefore:F4}/{previewBefore:F4}, restored={sourceRestored:F4}/{previewRestored:F4}, after={source.VerticalScrollProgress:F4}/{preview.VerticalScrollProgress:F4}, marker={restored.PreviewUtf16Offset}->{preview.GetViewportAnchor().PreviewUtf16Offset}.");
        }
        else
        {
            log.AppendLine($"OBS: CacheAnchorCheck kind={kind}, previewLed={previewLed}; measured={measuredBefore:R}->{preview.MeasuredHeight:R}; before={before.PreviewUtf16Offset}:{before.OffsetWithinLine:R}; after={restored.PreviewUtf16Offset}:{restored.OffsetWithinLine:R}; source={sourceBefore:R}->{sourceRestored:R}; preview={previewBefore:R}->{previewRestored:R}; y={yBefore:R}->{yRestored:R}; busy={extension.TestSourceFindBusy}.");
            Check(Math.Abs((long)restored.PreviewUtf16Offset - (long)before.PreviewUtf16Offset) < (kind == "resize" ? 100 : 1),
                "Cached geometry restores the original paragraph across window and resource reflow.");
            if (image != null)
                Check(preview.MeasuredHeight > measuredBefore + 400 && yRestored > yBefore + 400 &&
                    Math.Abs(restored.OffsetWithinLine - before.OffsetWithinLine) <= 1 / preview.XamlRoot.RasterizationScale,
                    "The larger image actually reflows above the held paragraph while retaining its within-line position to one physical pixel.");
            log.AppendLine($"OBS: ReviewReflow kind={kind}; width={widthBefore:F1}->{preview.ActualWidth:F1}, source={sourceBefore:F4}->{sourceRestored:F4}, preview={previewBefore:F4}->{previewRestored:F4}, y={yBefore:F1}->{yRestored:F1}, marker={before.PreviewUtf16Offset}:{before.OffsetWithinLine:F1}->{restored.PreviewUtf16Offset}:{restored.OffsetWithinLine:F1}.");
        }
    }

    private static async Task RunEvictedTabRestoreAsync(StringBuilder log, bool follow)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var text = "# Intro\n\n```text\n" +
            string.Join("\n", Enumerable.Range(1, 160).Select(index => $"Code row {index:D3}")) + "\n```\n\n" +
            string.Join("\n\n", Enumerable.Range(1, 100).Select(index => $"# Heading {index:D3}"));
        var editor = await OpenAsync(host, "evicted-viewport.md", Encoding.UTF8.GetBytes(text));
        var original = host.Core.GetSelectedTab();
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        var session = extension.SourceSession;
        var source = (WinUIEditor.EditorBaseControl)((Grid)editor.TestNativeCore.Content).Children[0];
        extension.FollowSource = follow;
        await UntilAsync(() => !source.IsViewportRestorePending && preview.VerticalScrollProgress >= 0 &&
            source.VerticalScrollProgress >= 0, "Both scroll extents are ready before eviction.");
        var sourceAtStart = source.VerticalScrollProgress;
        if (follow) extension.TestBeginViewportScroll(preview: true);
        Check(preview.TrySetVerticalScrollProgress(0.55), "The preview scrolls to the middle before eviction.");
        await UntilAsync(() => Math.Abs(preview.VerticalScrollProgress - 0.55) < 0.005 &&
            (!follow || !extension.TestSourceFindBusy && !source.IsViewportRestorePending && !source.IsScrollAnimating &&
                source.VerticalScrollProgress > sourceAtStart + 0.05) &&
            preview.GetViewportAnchor().IsAvailable && preview.GetViewportAnchor().PreviewUtf16Offset > 100,
            "The preview has rendered the middle and, with follow enabled, the source has moved before eviction.");
        var before = preview.GetViewportAnchor();
        double beforeSource = source.VerticalScrollProgress, beforePreview = preview.VerticalScrollProgress;
        var version = (session.AcceptedRevision, session.FullSubmissions, session.DeltaSubmissions, session.CopiedBytes);

        for (int i = 1; i <= 9; i++)
        {
            var other = await OpenAsync(host, $"eviction-tab-{i}.md", Encoding.UTF8.GetBytes("# Other"));
            other.ShowHideContentPreview();
            await CurrentAsync(other);
        }
        await UntilAsync(() => !preview.IsLoaded && preview.HasCachedDocument, "Nine newer previews exceed the eight-background cache and evict the first visual.");

        bool readyBeforePaint = false, loadingBeforePaint = false;
        session.Changed += (_, _) =>
        {
            if (preview.IsSurfaceReady || !extension.IsLoaded) return;
            readyBeforePaint |= session.Status == MarkdownSessionStatus.Ready;
            loadingBeforePaint |= extension.FindName("PreviewLoading") is Border
                { Visibility: Visibility.Visible, Child: Notepads.Presentation.Controls.DocumentLoadingPlaceholder };
        };
        MarkdownViewportAnchor firstFrame = null;
        preview.ViewportChanged += (_, anchor) =>
        {
            if (anchor.IsAvailable) firstFrame ??= anchor;
        };

        host.Core.SwitchTo(original);
        await CurrentAsync(editor);
        await Task.Delay(650); // A late layout or follow callback must not shift the restored position.
        var after = preview.GetViewportAnchor();
        log.AppendLine($"OBS: EvictedTab follow={follow}; source={beforeSource:F4}->{source.VerticalScrollProgress:F4}, preview={beforePreview:F4}->{preview.VerticalScrollProgress:F4}, actual marker={before.PreviewUtf16Offset}:{before.OffsetWithinLine:F1}->{after.PreviewUtf16Offset}:{after.OffsetWithinLine:F1}, readyBeforePaint={readyBeforePaint}, loadingBeforePaint={loadingBeforePaint}.");
        Check(Math.Abs(source.VerticalScrollProgress - beforeSource) < 0.005 &&
            Math.Abs(preview.VerticalScrollProgress - beforePreview) < 0.005 &&
            firstFrame?.PreviewUtf16Offset == before.PreviewUtf16Offset && after.IsAvailable &&
            after.PreviewUtf16Offset == before.PreviewUtf16Offset &&
            Math.Abs(after.OffsetWithinLine - before.OffsetWithinLine) < 0.5,
            "The first frame and settled viewport preserve the evicted tab's actual reading position.");
        Check(!readyBeforePaint && loadingBeforePaint &&
            extension.FindName("PreviewLoading") is Border { Visibility: Visibility.Collapsed, Child: null } &&
            version == (session.AcceptedRevision, session.FullSubmissions, session.DeltaSubmissions, session.CopiedBytes),
            "The cached model shows the skeleton until the first frame without resubmitting text.");

        if (follow)
        {
            Check(preview.Focus(FocusState.Programmatic), "The restored preview can receive focus.");
            var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
            var scrollProvider = (IScrollProvider)peer.GetPattern(PatternInterface.Scroll);
            scrollProvider.SetScrollPercent(-1, 65);
            await UntilAsync(() => Math.Abs(preview.VerticalScrollProgress - 0.65) < 0.005 &&
                source.VerticalScrollProgress > beforeSource + 0.005,
                "The preview's automation scroll resumes viewport following after reattachment.");
            double next = source.VerticalScrollProgress < 0.7 ? 0.75 : 0.45;
            Check(source.TrySetVerticalScrollProgress(next), "Native source scrolling is admitted after reattachment.");
            await UntilAsync(() => Math.Abs(source.VerticalScrollProgress - next) < 0.005 &&
                Math.Abs(preview.VerticalScrollProgress - 0.65) > 0.01,
                "A subsequent native source scroll also follows without a separate intent hook.");
        }
    }

    private static async Task RunCachedFirstFrameAsync(StringBuilder log, bool interruptGeometry, bool automaticRetry = false)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var root = (Panel)host.Page.Content;
        var preview = new MarkdownView
        {
            CacheDocumentOnUnload = true, Width = 430, Height = 320,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        var document = Guid.NewGuid();
        var text = string.Join("\n\n", Enumerable.Range(1, 700).Select(index =>
            $"## Section {index:D3}\n\nSeveral words wrap across the native Markdown preview viewport."));
        root.Children.Add(preview);
        try
        {
            await UntilAsync(() => preview.IsLoaded && preview.ActualHeight == 320, "The initial viewport is arranged.");
            var initial = await preview.SetDocumentAsync(document, 1, Encoding.UTF8.GetBytes(text))
                .AsTask().WaitAsync(TimeSpan.FromSeconds(15));
            Check(initial.Status == DocumentUpdateStatus.Ready && initial.Published, "The initial frame is ready.");
            root.Children.Remove(preview);
            await UntilAsync(() => !preview.IsLoaded && preview.HasCachedDocument, "The native model survives unload.");
            root.Children.Add(preview);
            ScrollViewer scroll = null;
            await UntilAsync(() =>
            {
                scroll ??= Descendants(preview).OfType<ScrollViewer>().FirstOrDefault(view => view.Name == "PreviewScrollViewer");
                return preview.IsLoaded && !preview.IsSurfaceReady &&
                    scroll is { ActualWidth: > 0, ActualHeight: > 0, ScrollableHeight: > 0 };
            }, "Reattached geometry is available before cached layout.");

            var resume = preview.ResumeCachedDocumentAsync(document, 1).AsTask();
            if (interruptGeometry)
            {
                // Both layouts happen before the first worker can deliver its frame on the UI thread.
                scroll.Height = 0;
                root.UpdateLayout();
                Check(scroll.ActualHeight == 0, "The first cached layout loses its viewport.");
                scroll.Height = double.NaN;
                root.UpdateLayout();
                if (automaticRetry)
                    await UntilAsync(() => preview.IsSurfaceReady, "The restored layout alone retries the cached first frame.");
                var retry = preview.ResumeCachedDocumentAsync(document, 1).AsTask();
                var first = await resume.WaitAsync(TimeSpan.FromSeconds(15));
                var second = await retry.WaitAsync(TimeSpan.FromSeconds(15));
                log.AppendLine($"OBS: CachedReceipt automatic={automaticRetry}, restoredHeight={scroll.ActualHeight:F0}, first={first.Status}, second={second.Status}, ready={preview.IsSurfaceReady}.");
                Check(first.Status == DocumentUpdateStatus.NotLoaded && second.Status == DocumentUpdateStatus.Ready &&
                    second.Published && preview.IsSurfaceReady,
                    "The replacement receipt publishes with no further layout or input.");
            }
            else
            {
                double firstY = -1;
                MarkdownViewportAnchor firstFrame = null;
                preview.ViewportChanged += (_, anchor) =>
                {
                    if (firstFrame != null || !anchor.IsAvailable) return;
                    firstY = scroll.VerticalOffset;
                    firstFrame = anchor;
                };
                Check(preview.TrySetVerticalScrollProgress(1), "The viewport scrolls during cached layout.");
                var result = await resume.WaitAsync(TimeSpan.FromSeconds(15));
                await UntilAsync(() => preview.GetViewportAnchor().PreviewUtf16Offset > 100, "The bottom viewport renders.");
                var final = preview.GetViewportAnchor();
                log.AppendLine($"OBS: CachedFirstFrame receipt={result.Status}/{result.Published}, firstY={firstY:F1}, firstMarker={firstFrame?.PreviewUtf16Offset}, finalY={scroll.VerticalOffset:F1}, finalMarker={final.PreviewUtf16Offset}.");
                Check(result.Status == DocumentUpdateStatus.Ready && result.Published &&
                    Math.Abs(firstY - scroll.ScrollableHeight) < 0.5 && firstFrame?.PreviewUtf16Offset == final.PreviewUtf16Offset,
                    "The first frame drawn while resuming already matches the scrollbar at the bottom.");
            }
        }
        finally { root.Children.Remove(preview); }
    }

    private static async Task RunInitialFollowPaddingAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var source = "<p align=\"center\">\n  Notepads\n</p>\n\n" +
            string.Join("\n\n", Enumerable.Range(1, 100).Select(index => $"## Section {index}"));
        var editor = await OpenAsync(host, "initial-follow-padding.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        var scroll = Descendants(preview).OfType<ScrollViewer>().Single(item => item.Name == "PreviewScrollViewer");
        var sourceView = (WinUIEditor.EditorBaseControl)((Grid)editor.TestNativeCore.Content).Children[0];
        Check(extension.FollowSource, "The fixture exercises default source following.");
        await UntilAsync(() => sourceView.VerticalScrollProgress == 0 && scroll.ScrollableHeight > 0,
            "The source is at the top and the preview has room to scroll.");

        // Give the initial source mapping time to settle without scroll, focus or resize input.
        await Task.Delay(400);
        double initialOffset = scroll.VerticalOffset;
        var initialAnchor = preview.GetViewportAnchor();

        extension.TestBeginViewportScroll(preview: false);
        await UntilAsync(() => scroll.VerticalOffset == 0 && preview.GetViewportAnchor().IsAvailable,
            "Physical scroll following publishes the top of the preview.");
        var topAnchor = preview.GetViewportAnchor();
        log.AppendLine($"OBS: Markdown.InitialFollowPadding scrollbar={initialOffset:F1}->{scroll.VerticalOffset:F1} DIP, drawn offset={initialAnchor.PreviewUtf16Offset}:{initialAnchor.OffsetWithinLine:F1}->{topAnchor.PreviewUtf16Offset}:{topAnchor.OffsetWithinLine:F1}, scale={preview.XamlRoot.RasterizationScale}.");
        Check(initialOffset == 0 && initialAnchor.IsAvailable &&
            initialAnchor.PreviewUtf16Offset == topAnchor.PreviewUtf16Offset &&
            Math.Abs(initialAnchor.OffsetWithinLine - topAnchor.OffsetWithinLine) < 0.5,
            "Initial source following must show the same padded top as physical scroll following.");
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        var probe = reading.DocumentRange.FindText("Notepads", false, false);
        Check(probe != null, "A fixed top text range is available as a screen-position oracle.");
        probe.GetBoundingRectangles(out var initialRect);
        Check(initialRect.Length == 4, "The fixed top text range is visible before scrolling.");
        double scale = preview.XamlRoot.RasterizationScale;
        double extent = sourceView.VerticalScrollExtent;
        double previousOffset = 0;
        foreach (int pixels in new[] { 2, 6, 2 })
        {
            double sourceOffset = pixels / scale;
            Check(sourceView.TrySetVerticalScrollProgress(sourceOffset / extent), "Move by a few source pixels near the top.");
            await UntilAsync(() => scroll.VerticalOffset > 0 && scroll.VerticalOffset != previousOffset,
                "The preview follows each small movement near the top.");
            await UntilAsync(() =>
            {
                probe.GetBoundingRectangles(out var rect);
                return rect.Length == 4 && Math.Abs(initialRect[1] - rect[1] - scroll.VerticalOffset * scale) <= 1;
            }, "The visible preview frame catches up to the scroll offset.");
            log.AppendLine($"OBS: Markdown.InitialFollowPadding source={pixels} physical pixels, preview={scroll.VerticalOffset:F2} DIP, scale={scale}.");
            Check(scroll.VerticalOffset <= 4 * sourceOffset + 1 / scale,
                "A small scroll near the top must not jump over the entire preview inset.");
            previousOffset = scroll.VerticalOffset;
        }
        Check(sourceView.TrySetVerticalScrollProgress(0), "Return to the physical source top.");
        await UntilAsync(() =>
        {
            probe.GetBoundingRectangles(out var rect);
            return scroll.VerticalOffset == 0 && rect.Length == 4 && Math.Abs(rect[1] - initialRect[1]) <= 1;
        }, "The preview renders the original padded top again.");
        log.AppendLine("PASS: Markdown.InitialFollowPadding — first paint and small scrolls in both directions preserve a continuous top inset.");
    }

    private static async Task RunFollowedTabViewportAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var source = string.Join("\r\r", Enumerable.Range(1, 150).Select(index => $"# Tab viewport marker {index:D3}"));
        var editor = await OpenAsync(host, "followed-viewport.md", Encoding.UTF8.GetBytes(source));
        var tab = host.Core.GetSelectedTab();
        var native = editor.TestNativeCore.TestFeatureNative;
        var sourceView = (WinUIEditor.EditorBaseControl)((Grid)editor.TestNativeCore.Content).Children[0];
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        Check(extension.FollowSource, "The fixture exercises the default source following.");
        editor.Focus();
        native.SetEmptySelection(0);
        await UntilAsync(() => !sourceView.IsViewportRestorePending && sourceView.VerticalScrollProgress >= 0 &&
            preview.VerticalScrollProgress >= 0, "Both real scroll surfaces are ready.");

        await OpenAsync(host, "other-tab.txt", Encoding.UTF8.GetBytes("Other tab"));
        var other = host.Core.GetSelectedTab();
        host.Core.SwitchTo(tab);
        await CurrentAsync(editor);
        await UntilAsync(() => !sourceView.IsViewportRestorePending && sourceView.VerticalScrollProgress >= 0 &&
            preview.VerticalScrollProgress >= 0, "Both real scroll surfaces are restored after switching tabs.");

        foreach (bool fromPreview in new[] { false, true })
        {
            var pane = fromPreview ? "preview" : "source";
            double progress = fromPreview ? 0.65 : 0.5;
            if (fromPreview) Check(preview.Focus(FocusState.Programmatic), "The preview can own the next scroll.");
            var sourceBeforeScroll = sourceView.VerticalScrollProgress;
            var previewBeforeScroll = preview.GetViewportAnchor().PreviewUtf16Offset;
            extension.TestBeginViewportScroll(preview: fromPreview);
            bool scrolled = fromPreview ? preview.TrySetVerticalScrollProgress(progress) : sourceView.TrySetVerticalScrollProgress(progress);
            Check(scrolled, "The " + pane + " scrolls to the middle without moving the caret.");
            await UntilAsync(() =>
            {
                var anchor = preview.GetViewportAnchor();
                return anchor.IsAvailable && anchor.PreviewUtf16Offset > 100 &&
                    (fromPreview
                        ? Math.Abs(preview.VerticalScrollProgress - progress) < 0.005 &&
                            Math.Abs(sourceView.VerticalScrollProgress - sourceBeforeScroll) > 0.005
                        : Math.Abs(sourceView.VerticalScrollProgress - progress) < 0.005 &&
                            anchor.PreviewUtf16Offset != previewBeforeScroll);
            }, "The preview draws the middle before switching tabs.");
            Check(native.CurrentPos == 0, "The source caret stays at the beginning.");
            var before = preview.GetViewportAnchor();
            var offsetBefore = preview.VerticalScrollProgress;
            var sourceOffsetBefore = sourceView.VerticalScrollProgress;

            host.Core.SwitchTo(other);
            await TabDragTestHost.FrameAsync();
            Check(preview.IsLoaded, "The Markdown preview is retained while another tab is selected.");
            host.Core.SwitchTo(tab);
            await CurrentAsync(editor);
            await Task.Delay(350); // Let any focus-triggered mapping finish without scrolling again.
            var after = preview.GetViewportAnchor();
            log.AppendLine($"OBS: Markdown.TabViewport {pane} source={sourceView.VerticalScrollProgress:F3}, preview={offsetBefore:F3}->{preview.VerticalScrollProgress:F3}, drawn offset={before.PreviewUtf16Offset}->{after.PreviewUtf16Offset}.");
            Check(native.CurrentPos == 0 && Math.Abs(sourceView.VerticalScrollProgress - sourceOffsetBefore) < 0.005 &&
                Math.Abs(preview.VerticalScrollProgress - offsetBefore) < 0.005 &&
                after.IsAvailable && after.PreviewUtf16Offset == before.PreviewUtf16Offset,
                "Restoring focus preserves the preview scrollbar and the actually drawn middle after scrolling the " + pane + ".");
        }

        var beforeCaret = preview.GetViewportAnchor().PreviewUtf16Offset;
        Check(editor.TestNativeCore.GoTo(3), "The real Go To Line command accepts a visible line.");
        await UntilAsync(() => preview.VerticalScrollProgress < 0.05 &&
            preview.GetViewportAnchor().PreviewUtf16Offset < beforeCaret,
            "A subsequent real caret command still reveals its target in the preview.");
        log.AppendLine("PASS: Markdown.TabViewport — default follow preserves both panes on tab return and still reveals actual caret navigation.");
    }

    private static async Task RunCachedPreviewGeometryAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var root = host.Page.Content as Panel;
        Check(root != null, "The isolated workspace can host a native preview.");
        var preview = new MarkdownView
        {
            CacheDocumentOnUnload = true, Width = 430, Height = 320,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        var document = Guid.NewGuid();
        var text = string.Join("\n\n", Enumerable.Range(1, 160).Select(index =>
            $"## Section {index}\n\nSeveral words wrap across the native Markdown preview viewport."));
        root.Children.Add(preview);
        try
        {
            await UntilAsync(() => preview.IsLoaded && preview.ActualHeight == 320, "The initial preview is arranged.");
            var initial = await preview.SetDocumentAsync(document, 1, Encoding.UTF8.GetBytes(text))
                .AsTask().WaitAsync(TimeSpan.FromSeconds(15));
            Check(initial.Status == DocumentUpdateStatus.Ready && initial.Published, "The initial preview publishes.");
            root.Children.Remove(preview);
            await UntilAsync(() => !preview.IsLoaded && preview.HasCachedDocument, "Detach retains the published model.");

            root.Children.Add(preview);
            var scroll = (ScrollViewer)null;
            await UntilAsync(() =>
            {
                if (!preview.IsLoaded) return false;
                scroll ??= Descendants(preview).OfType<ScrollViewer>().FirstOrDefault(item => item.Name == "PreviewScrollViewer");
                return scroll?.ViewportHeight == 320;
            }, "The reattached preview has its original viewport before redraw.");

            // Resume queues the first layout synchronously. Change only its height
            // while the worker result still has to return through the UI dispatcher.
            var redraw = preview.ResumeCachedDocumentAsync(document, 1).AsTask();
            Check(!redraw.IsCompleted, "The cached redraw is still pending.");
            preview.Height = 460;
            preview.UpdateLayout();
            Check(scroll.ViewportHeight == 460, "The pending redraw is overtaken by a height-only layout change.");
            if (await Task.WhenAny(redraw, Task.Delay(TimeSpan.FromSeconds(3))) != redraw)
            {
                log.AppendLine("OBS: no cached frame was published during three seconds of idle time.");
                preview.Height = 461;
                preview.UpdateLayout();
                bool recovered = await Task.WhenAny(redraw, Task.Delay(TimeSpan.FromSeconds(3))) == redraw;
                log.AppendLine($"OBS: another layout event completed the same redraw: {recovered}.");
                throw new InvalidOperationException("The latest viewport must publish without a second layout event.");
            }

            var result = await redraw;
            Check(result.Status == DocumentUpdateStatus.Ready && result.Published &&
                HasReadingIdentity(preview, document, 1), "The current viewport publishes the same document and revision.");
            var surface = Descendants(preview).OfType<FrameworkElement>().Single(item => item.Name == "SurfaceTarget");
            await UntilAsync(() => Math.Abs(surface.ActualHeight - scroll.ViewportHeight) <= 2 / preview.XamlRoot.RasterizationScale,
                "The published surface is arranged at the visible viewport height.");
            CheckPixelTarget(surface, "cached preview after height-only layout change");
            log.AppendLine($"PASS: Markdown.ViewportRefresh — cached revision 1 presents the {scroll.ViewportHeight:F0} DIP viewport without any second layout event; scale={preview.XamlRoot.RasterizationScale}.");
        }
        finally
        {
            preview.Cancel();
            root.Children.Remove(preview);
        }
    }
}
