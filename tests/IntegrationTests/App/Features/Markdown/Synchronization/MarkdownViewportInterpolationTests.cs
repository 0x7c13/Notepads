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
using NotepadsWorkspaceTests;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class MarkdownViewportInterpolationTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var previous = Window.Current.Content;
        var root = new Grid();
        var preview = new MarkdownView
        {
            Width = 520, Height = 280,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        root.Children.Add(preview);
        try
        {
            Window.Current.Content = root;
            Window.Current.Activate();
            await UntilAsync(() => preview.IsLoaded && preview.ActualWidth > 0);
            var document = Guid.NewGuid();
            var source = string.Join("\r\r", Enumerable.Range(0, 30).Select(index => $"Paragraph {index:D3} Ω😀 ordinary body."));
            var receipt = await preview.SetDocumentAsync(document, 1, Encoding.UTF8.GetBytes(source)).AsTask()
                .WaitAsync(TimeSpan.FromSeconds(25));
            Check(receipt.Status == DocumentUpdateStatus.Ready && receipt.Published,
                "The actual ordinary paragraph fixture publishes through the real package.");
            await UntilAsync(() => preview.GetSelection().IsAvailable && preview.GetViewportAnchor().IsAvailable &&
                preview.IsSurfaceReady && string.IsNullOrEmpty(preview.RenderingError));
            var scroll = Descendants(preview).OfType<ScrollViewer>().Single(item => item.Name == "PreviewScrollViewer");
            // Ready publishes the native frame before XAML necessarily arranges
            // its new extent. The endpoint oracle needs a scrollable document.
            await UntilAsync(() => scroll.ViewportHeight > 0 && scroll.ExtentHeight > scroll.ViewportHeight &&
                scroll.ScrollableHeight > 0 && preview.IsSurfaceReady && string.IsNullOrEmpty(preview.RenderingError),
                "The fixed interpolation fixture must have an arranged scrollable extent before its endpoint requests.");
            Check(scroll.VerticalOffset == 0, "The independent screen-position oracle starts at the document origin.");
            var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
            var provider = (ITextProvider)peer.GetPattern(PatternInterface.Text);
            var probe = provider.DocumentRange.FindText("Paragraph 003", false, false);
            Check(probe != null, "A fixed uniquely numbered native text range is visible before any scroll request.");
            probe.GetBoundingRectangles(out var initialRectangle);
            Check(initialRectangle.Length == 4, "The fixed probe starts with one actual visible rectangle.");
            var originY = initialRectangle[1];
            var scale = preview.XamlRoot.RasterizationScale;
            var plain = preview.DisplayedText;
            var start = checked((ulong)plain.IndexOf("Paragraph 001", StringComparison.Ordinal));
            var end = checked((ulong)plain.IndexOf("Paragraph 002", StringComparison.Ordinal));
            Check(start > 0 && end > start, "The two ordinary endpoints are real ordered public UTF-16 positions.");
            var selection = preview.GetSelection();
            preview.SelectRange(document, 1, selection.TextGeneration, 0, 4);
            Check(preview.Focus(FocusState.Programmatic), "The actual preview receives focus before selection-preservation checks.");
            selection = preview.GetSelection();
            var focused = FocusManager.GetFocusedElement();

            // The old public RevealRange is the ordinary-line endpoint oracle.
            // Actual published screen coordinates are checked independently of
            // ScrollViewer's desired offset on every subsequent request.
            var beforeStart = scroll.VerticalOffset;
            log.AppendLine(FormattableString.Invariant($"OBS: Markdown.ViewportEndpoint start-request before={beforeStart:R}, position={start}, current={scroll.VerticalOffset:R}, scrollable={scroll.ScrollableHeight:R}, layout={preview.GetViewportAnchor().LayoutGeneration}."));
            preview.RevealRange(document, 1, selection.TextGeneration, start, 0, true, false);
            await UntilAsync(() => scroll.VerticalOffset > beforeStart,
                "The first ordinary RevealRange request must actually advance the desired offset before its endpoint is sampled.");
            log.AppendLine(FormattableString.Invariant($"OBS: Markdown.ViewportEndpoint start-advanced before={beforeStart:R}, current={scroll.VerticalOffset:R}, layout={preview.GetViewportAnchor().LayoutGeneration}."));
            await PublishedAtDesiredAsync(preview, scroll, probe, originY, scale);
            var startY = scroll.VerticalOffset;
            log.AppendLine(FormattableString.Invariant($"OBS: Markdown.ViewportEndpoint start-published before={beforeStart:R}, start={startY:R}, current={scroll.VerticalOffset:R}, layout={preview.GetViewportAnchor().LayoutGeneration}."));
            var beforeEnd = scroll.VerticalOffset;
            log.AppendLine(FormattableString.Invariant($"OBS: Markdown.ViewportEndpoint end-request before={beforeEnd:R}, position={end}, current={scroll.VerticalOffset:R}, scrollable={scroll.ScrollableHeight:R}, layout={preview.GetViewportAnchor().LayoutGeneration}."));
            preview.RevealRange(document, 1, selection.TextGeneration, end, 0, true, false);
            await UntilAsync(() => scroll.VerticalOffset > beforeEnd,
                "The second ordinary RevealRange request must actually advance the desired offset before its endpoint is sampled.");
            log.AppendLine(FormattableString.Invariant($"OBS: Markdown.ViewportEndpoint end-advanced before={beforeEnd:R}, current={scroll.VerticalOffset:R}, layout={preview.GetViewportAnchor().LayoutGeneration}."));
            await PublishedAtDesiredAsync(preview, scroll, probe, originY, scale);
            var endY = scroll.VerticalOffset;
            log.AppendLine(FormattableString.Invariant($"OBS: Markdown.ViewportEndpoint end-published before={beforeEnd:R}, start={startY:R}, end={endY:R}, current={scroll.VerticalOffset:R}, scrollable={scroll.ScrollableHeight:R}, layout={preview.GetViewportAnchor().LayoutGeneration}."));
            Check(endY > startY && endY < scroll.ScrollableHeight,
                "The endpoint oracle has distinct interior Y values without top/bottom clamping.");
            var layout = preview.GetViewportAnchor().LayoutGeneration;
            foreach (var fraction in new[] { 0.0, 0.25, 0.5, 0.75, 1.0 })
            {
                Check(preview.SetViewportBetweenPositions(document, 1, selection.TextGeneration, layout, start, end, fraction),
                    "The new public API admits the current ordinary interpolation target.");
                var expected = startY + fraction * (endY - startY);
                await UntilAsync(() => Math.Abs(scroll.VerticalOffset - expected) <= 0.001,
                    "The actual desired offset follows the fixed endpoint interpolation formula.");
                await PublishedAtDesiredAsync(preview, scroll, probe, originY, scale);
                Check(preview.GetViewportAnchor().LayoutGeneration == layout,
                    "Viewport interpolation uses one unchanged actual reading layout.");
                log.AppendLine($"OBS: Markdown.ViewportInterpolation fraction={fraction}, desired={scroll.VerticalOffset}, expected={expected}, scale={scale}.");
            }

            var held = scroll.VerticalOffset;
            foreach (var fraction in new[] { double.NaN, double.PositiveInfinity, -0.01, 1.01 })
                RejectInvalid(() => preview.SetViewportBetweenPositions(document, 1, selection.TextGeneration, layout, start, end, fraction));
            RejectInvalid(() => preview.SetViewportBetweenPositions(document, 1, selection.TextGeneration, layout, end, start, 0.5));
            RejectInvalid(() => preview.SetViewportBetweenPositions(document, 1, selection.TextGeneration, layout, start,
                checked((ulong)plain.Length + 1), 0.5));
            var splitScalar = checked((ulong)(plain.IndexOf("😀", StringComparison.Ordinal) + 1));
            RejectInvalid(() => preview.SetViewportBetweenPositions(document, 1, selection.TextGeneration, layout, splitScalar, end, 0.5));
            Check(!preview.SetViewportBetweenPositions(Guid.NewGuid(), 1, selection.TextGeneration, layout, start, end, 0.5) &&
                !preview.SetViewportBetweenPositions(document, 2, selection.TextGeneration, layout, start, end, 0.5) &&
                !preview.SetViewportBetweenPositions(document, 1, selection.TextGeneration + 1, layout, start, end, 0.5) &&
                !preview.SetViewportBetweenPositions(document, 1, selection.TextGeneration, layout + 1, start, end, 0.5),
                "Foreign/stale source, text and layout identities are rejected without an admitted target.");
            await TabDragTestHost.FrameAsync();
            Check(scroll.VerticalOffset == held, "Illegal/stale calls leave the actual desired viewport untouched.");
            await PublishedAtDesiredAsync(preview, scroll, probe, originY, scale);
            var after = preview.GetSelection();
            Check(after.IsAvailable && after.DocumentId == selection.DocumentId && after.Revision == selection.Revision &&
                after.TextGeneration == selection.TextGeneration && after.Start == selection.Start && after.Length == selection.Length &&
                ReferenceEquals(focused, FocusManager.GetFocusedElement()),
                "Admitted/invalid/stale interpolation preserves the actual selection and keyboard focus.");
            log.AppendLine("PASS: Markdown.ViewportInterpolation — real ordinary endpoint RevealRange oracle, 0/quarter/half/three-quarter/1 desired formula and independently published fixed-glyph screen Y; illegal/scalar/stale guards and unchanged selection/focus. Table plateau, resource relayout and source-follow ownership are separate cases.");

            // R17 is a separate gate. Preserve the historical endpoint oracle above;
            // a batch is two point mappings, never a range envelope or a bijection.
            var previewSpan = await preview.MapPreviewSpanAsync(document, 1, selection.TextGeneration,
                start, end, MarkdownPositionAffinity.After).AsTask().WaitAsync(TimeSpan.FromSeconds(25));
            var previewStart = await preview.MapPreviewPositionAsync(document, 1, selection.TextGeneration,
                start, MarkdownPositionAffinity.After).AsTask().WaitAsync(TimeSpan.FromSeconds(25));
            var previewEnd = await preview.MapPreviewPositionAsync(document, 1, selection.TextGeneration,
                end, MarkdownPositionAffinity.After).AsTask().WaitAsync(TimeSpan.FromSeconds(25));
            Check(SameMapping(previewSpan.Start, previewStart) && SameMapping(previewSpan.End, previewEnd),
                "Preview batch endpoints retain exactly the existing public point-mapping semantics.");
            var sourceSpan = await preview.MapSourceSpanAsync(document, 1, selection.TextGeneration,
                previewStart.SourceByteOffset, previewEnd.SourceByteOffset, MarkdownPositionAffinity.After)
                .AsTask().WaitAsync(TimeSpan.FromSeconds(25));
            var sourceStart = await preview.MapSourcePositionAsync(document, 1, selection.TextGeneration,
                previewStart.SourceByteOffset, MarkdownPositionAffinity.After).AsTask().WaitAsync(TimeSpan.FromSeconds(25));
            var sourceEnd = await preview.MapSourcePositionAsync(document, 1, selection.TextGeneration,
                previewEnd.SourceByteOffset, MarkdownPositionAffinity.After).AsTask().WaitAsync(TimeSpan.FromSeconds(25));
            Check(SameMapping(sourceSpan.Start, sourceStart) && SameMapping(sourceSpan.End, sourceEnd),
                "Source batch endpoints retain exactly the existing public point-mapping semantics.");
            var continuous = preview.GetViewportAnchor();
            Check(continuous.IsAvailable && continuous.DocumentId == document && continuous.Revision == 1 &&
                continuous.TextGeneration == selection.TextGeneration && continuous.LayoutGeneration == layout &&
                continuous.ScrollStartUtf16Offset <= continuous.ScrollEndUtf16Offset &&
                continuous.ScrollEndUtf16Offset <= preview.DisplayedTextLength &&
                double.IsFinite(continuous.ScrollProgress) && continuous.ScrollProgress >= 0 && continuous.ScrollProgress <= 1,
                "The published inverse span carries the current immutable identities and finite bounded progress.");
            Check(preview.SetViewportBetweenPositions(document, 1, selection.TextGeneration, layout,
                continuous.ScrollStartUtf16Offset, continuous.ScrollEndUtf16Offset, continuous.ScrollProgress),
                "The actual published inverse span is accepted as a continuous forward target.");
            await UntilAsync(() => Math.Abs(scroll.VerticalOffset - held) * scale <= 1.0,
                "The published inverse span reconstructs the previous interior viewport within one physical pixel.");
            await PublishedAtDesiredAsync(preview, scroll, probe, originY, scale);
            var continuousSelection = preview.GetSelection();
            Check(continuousSelection.Start == selection.Start && continuousSelection.Length == selection.Length &&
                ReferenceEquals(focused, FocusManager.GetFocusedElement()),
                "Read-only batch mappings and inverse-span reconstruction preserve selection and keyboard focus.");
            log.AppendLine("PASS: Markdown.ViewportContinuity — batch endpoints equal the old point APIs in both directions; current published inverse span reconstructs the held interior viewport within one physical pixel, preserving selection/focus. This does not certify actual source-follow motion.");
        }
        finally
        {
            preview.Cancel();
            root.Children.Clear();
            Window.Current.Content = previous;
        }
    }

    private static bool SameMapping(MarkdownMappedPosition left, MarkdownMappedPosition right) =>
        left.DocumentId == right.DocumentId && left.Revision == right.Revision &&
        left.TextGeneration == right.TextGeneration && left.SourceByteOffset == right.SourceByteOffset &&
        left.PreviewUtf16Offset == right.PreviewUtf16Offset && left.Affinity == right.Affinity &&
        left.Correspondence == right.Correspondence && left.Effects == right.Effects;

    private static Task PublishedAtDesiredAsync(MarkdownView preview, ScrollViewer scroll,
        Windows.UI.Xaml.Automation.Provider.ITextRangeProvider probe, double originY, double scale) => UntilAsync(() =>
    {
        if (!preview.GetViewportAnchor().IsAvailable || !preview.IsSurfaceReady || !string.IsNullOrEmpty(preview.RenderingError)) return false;
        try
        {
            probe.GetBoundingRectangles(out var rectangle);
            return rectangle.Length == 4 && Math.Abs(rectangle[1] - (originY - scroll.VerticalOffset * scale)) <= 1.0;
        }
        catch (Exception error) when (unchecked((uint)error.HResult) == 0x80040201) { return false; }
    }, "The actual native fixed glyph reaches the desired viewport within one physical-pixel projection.", 25000);

    private static void RejectInvalid(Func<bool> action)
    {
        try { action(); }
        catch (Exception error) when (unchecked((uint)error.HResult) == 0x80070057) { return; }
        throw new InvalidOperationException("The actual public API did not reject an invalid numeric/range/scalar input with E_INVALIDARG.");
    }

    private static Task UntilAsync(Func<bool> condition, string message = "The actual Markdown reading fixture must become available.",
        int milliseconds = 15000) => NotepadsWorkspaceTests.WorkspaceTestUI.UntilAsync(condition, message, milliseconds);

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
