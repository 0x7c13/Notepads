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
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media.Imaging;
using Windows.UI.ViewManagement;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    private static async Task InlineScriptSyntaxAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "Base ^Raised^ mid ~Lowered~ end.\n\n" +
            "HTML <sup>Raised</sup> mid <sub>Lowered</sub> end.\n\n" +
            "中😀 ^outer<sub>nested</sub>end^ and ~~H~2~O~~.\n\n" +
            "~[^script]~\n[^script]: Visible note.\n\n" +
            "[^unused]: Hidden note.\nUnindented continuation.\n\nVisible final.";
        var editor = await OpenAsync(host, "markdown-scripts.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var document = ((ITextProvider)peer.GetPattern(PatternInterface.Text)).DocumentRange;
        const string expected = "Base Raised mid Lowered end.\nHTML Raised mid Lowered end.\n中😀 outernestedend and H2O.\n" +
            "1\nVisible final.\n1.\nVisible note.\n↩";
        Check(document.GetText(-1) == expected && preview.DisplayedText == expected,
            "Markdown and HTML scripts expose the authored content without markup.");
        double[] Bounds(string value, bool last = false)
        {
            var range = document.FindText(value, last, false);
            Check(range != null && range.GetText(-1) == value, "Script text is reachable through UI Automation.");
            range.GetBoundingRectangles(out var rectangles);
            Check(rectangles.Length == 4, "The fixture's script range is visible on one line.");
            return rectangles;
        }
        var normal = Bounds("Base");
        var raised = Bounds("Raised");
        var lowered = Bounds("Lowered");
        Check(raised[1] < normal[1] - 1 && lowered[1] > normal[1] + 1 &&
            Math.Abs(raised[2] - Bounds("Raised", true)[2]) < 0.5 &&
            Math.Abs(lowered[2] - Bounds("Lowered", true)[2]) < 0.5 &&
            Bounds("nested")[3] < Bounds("outer")[3],
            "Markdown scripts reuse native script displacement, advances and nesting.");
        var identity = preview.GetSelection();
        var start = (ulong)expected.IndexOf("Raised", StringComparison.Ordinal);
        var mapped = await preview.MapPreviewSpanAsync(identity.DocumentId, identity.Revision, identity.TextGeneration,
            start + 1, start + 4, MarkdownPositionAffinity.After);
        var canonical = editor.GetText();
        var authored = (ulong)Encoding.UTF8.GetByteCount(canonical.AsSpan(0, canonical.IndexOf("^Raised^", StringComparison.Ordinal) + 2));
        Check(mapped.Start.SourceByteOffset == authored && mapped.End.SourceByteOffset == authored + 3,
            "Markdown script glyphs map to exact authored bytes inside the delimiters.");
        log.AppendLine("PASS: Markdown.InlineScripts verifies native raised and lowered geometry, mixed nesting, UIA text and exact source mapping.");
    }

    private static async Task RubyAnnotationAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "中😀 [<ruby>漢字<rp>(</rp><rt>reading</rt><rp>)</rp></ruby>](#target) 後。\n\n" +
            "<div><ruby>茶<rt>ちゃ</rt></ruby> と <ruby>文<rt>ぶん</rt>法<rt>ぽう</rt></ruby></div>";
        const string expected = "中😀 漢字reading 後。\n茶ちゃ と 文ぶん法ぽう";
        var editor = await OpenAsync(host, "ruby-annotation.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var text = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        var doc = text.DocumentRange;
        Check(doc.GetText(-1) == expected && preview.DisplayedText == expected,
            "Ruby exposes base and annotation in authored order across inline and block HTML.");

        double[] Bounds(string value)
        {
            var range = doc.FindText(value, false, false);
            Check(range != null && range.GetText(-1) == value, "Ruby range is available in UIA.");
            range.GetBoundingRectangles(out var bounds);
            Check(bounds?.Length == 4 && bounds[2] > 0 && bounds[3] > 0,
                "Each Ruby part exposes its own visible geometry.");
            return bounds;
        }

        var baseBounds = Bounds("漢字");
        var readingBounds = Bounds("reading");
        Check(readingBounds[1] + readingBounds[3] < baseBounds[1] && readingBounds[3] < baseBounds[3] &&
            Math.Abs(readingBounds[0] + readingBounds[2] * 0.5 - baseBounds[0] - baseBounds[2] * 0.5) < 2,
            "Ruby annotation is centered above the base in a separate text layout.");
        var annotation = (ulong)expected.IndexOf("reading", StringComparison.Ordinal);
        var identity = preview.GetSelection();
        preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, annotation + 1, 3);
        var selection = text.GetSelection();
        Check(selection.Length == 1 && selection[0].GetText(-1) == "ead",
            "Ruby annotation supports selecting an interior range.");
        var mapped = await preview.MapPreviewSpanAsync(identity.DocumentId, identity.Revision, identity.TextGeneration,
            annotation + 1, annotation + 4, MarkdownPositionAffinity.After);
        var canonical = editor.GetText();
        var sourceStart = (ulong)Encoding.UTF8.GetByteCount(canonical.AsSpan(0,
            canonical.IndexOf("reading", StringComparison.Ordinal) + 1));
        Check(mapped.Start.SourceByteOffset == sourceStart && mapped.End.SourceByteOffset == sourceStart + 3,
            "Ruby annotation retains exact UTF-8 positions within its rt tag.");
        log.AppendLine("PASS: Markdown.RubyAnnotation verifies UIA text, separate centered geometry, selection and authored source mapping.");
    }

    private static async Task HtmlSupSubAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "Base <sup>Raised</sup> Mid <sub>Lowered</sub> End.\r\r" +
            "Reference Raised and Lowered.\r\r" +
            "<sub>RowSubA</sub><br><sup>RowSupB</sup>\r\r" +
            "Nested <sup>outer<sub>inner</sub>end</sup> done.\r\r" +
            "中😀 <sup>中文 e&#x301; 👩&#x200d;💻</sup> שלום <sub>abc</sub>.\r\r" +
            "Links <sup>[left](#target)[right](#target)</sup>.\r\r" +
            "| Script | Content |\r| --- | --- |\r| Cell | H<sub>2</sub>O x<sup>2</sup> |\r\r" +
            "Literal <sup class='x'>raw</sup> and <sup>unclosed\r\r## target\r\r" +
            "<sup>̧́</sup>\r\r<code>̧́</code>\r\r<sup><code>̧́</code></sup>";
        var editor = await OpenAsync(host, "html-scripts.md", Encoding.UTF8.GetBytes(source));
        var core = editor.TestNativeCore;
        var originalZoom = core.GetFontZoomFactor();
        var originalTheme = host.Page.RequestedTheme;
        var originalBounds = Window.Current.CoreWindow.Bounds;
        log.AppendLine($"OBS: ScriptWindow original={originalBounds}.");
        Action detachViewportTrace = () => { };
        try
        {
            Check(ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(1100, 760)),
                "The script fixture admits the established real window size.");
            editor.ShowHideContentPreview();
            var extension = editor.TestMarkdownPreview;
            try { extension = await CurrentAsync(editor); }
            catch
            {
                var failedView = editor.TestMarkdownPreview?.PreviewView;
                log.AppendLine($"OBS: ScriptInitial session={editor.TestMarkdownPreview?.SourceSession?.Status}; surfaceReady={failedView?.IsSurfaceReady}; render='{failedView?.RenderingError}'; revision={failedView?.DisplayedRevision}; sourceRevision={editor.TestMarkdownPreview?.SourceSession?.AcceptedRevision}.");
                throw;
            }
            var preview = extension.PreviewView;
            var peer = FrameworkElementAutomationPeer.FromElement(preview) ??
                FrameworkElementAutomationPeer.CreatePeerForElement(preview);
            var text = (ITextProvider)peer.GetPattern(PatternInterface.Text);
            double[] Bounds(string value, bool last = false)
            {
                var range = text.DocumentRange.FindText(value, last, false);
                Check(range != null && range.GetText(-1) == value, "Script preview exposes exact authored text: " + value);
                range.GetBoundingRectangles(out var rectangles);
                return rectangles;
            }
            var sourceView = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
            var scroll = Descendants(preview).OfType<ScrollViewer>().Single(item => item.Name == "PreviewScrollViewer");
            var traceClock = System.Diagnostics.Stopwatch.StartNew();
            int viewportSequence = 0;
            string viewportPhase = "setup";
            double selectionViewportY = 0;
            bool trackingSelectionViewport = false;
            bool selectionViewportMoved = false;
            void TraceViewport(string moment)
            {
                core.GetScrollViewerPosition(out var sourceX, out var sourceY);
                var anchor = preview.GetViewportAnchor();
                log.AppendLine($"OBS: ScriptViewport #{++viewportSequence} ms={traceClock.ElapsedMilliseconds} phase={viewportPhase} at={moment}; actualY={scroll.VerticalOffset:R}; scrollable={scroll.ScrollableHeight:R}; extent={scroll.ExtentHeight:R}; viewport={scroll.ViewportHeight:R}; scale={preview.XamlRoot.RasterizationScale:R}; source={sourceX:R},{sourceY:R}; sourceRow={sourceView.Editor.FirstVisibleLine}; sourcePartial={sourceView.DisplayVerticalOffset:R}; sourcePending={sourceView.IsViewportRestorePending}; sourceAnimating={sourceView.IsScrollAnimating}; linkBusy={extension.TestSourceFindBusy}; anchor={anchor.IsAvailable}/{anchor.PreviewUtf16Offset}:{anchor.OffsetWithinLine:R}/{anchor.ScrollStartUtf16Offset}:{anchor.ScrollEndUtf16Offset}:{anchor.ScrollProgress:R}/layout{anchor.LayoutGeneration}.");
            }
            EventHandler<ScrollViewerViewChangedEventArgs> onViewChanged = (_, args) =>
            {
                if (trackingSelectionViewport && scroll.VerticalOffset != selectionViewportY)
                    selectionViewportMoved = true;
                TraceViewport("ViewChanged/" + args.IsIntermediate);
            };
            Windows.Foundation.TypedEventHandler<MarkdownView, MarkdownViewportAnchor> onViewportChanged = (_, _) => TraceViewport("ViewportChanged");
            scroll.ViewChanged += onViewChanged;
            preview.ViewportChanged += onViewportChanged;
            detachViewportTrace = () =>
            {
                scroll.ViewChanged -= onViewChanged;
                preview.ViewportChanged -= onViewportChanged;
            };
            var originalPreview = preview.DisplayedText;
            Check(originalPreview.Contains("Base Raised Mid Lowered End.", StringComparison.Ordinal) &&
                originalPreview.Contains("Nested outerinnerend done.", StringComparison.Ordinal) &&
                originalPreview.Contains("中😀 中文 é 👩‍💻 שלום abc.", StringComparison.Ordinal) &&
                originalPreview.Contains("Links leftright.", StringComparison.Ordinal) &&
                originalPreview.Contains("H2O x2", StringComparison.Ordinal) &&
                originalPreview.Contains("RowSubA\nRowSupB", StringComparison.Ordinal) &&
                originalPreview.Contains("Literal <sup class='x'>raw</sup> and <sup>unclosed", StringComparison.Ordinal),
                "Scripts compose with nested styles, decoded entities, bidi, links and table cells; unsafe or unmatched tags remain literal.");
            var identity = preview.GetSelection();
            var raisedStart = (ulong)originalPreview.IndexOf("Raised", StringComparison.Ordinal);
            foreach (var theme in new[] { ElementTheme.Dark, ElementTheme.Light })
            {
                if (extension.ActualTheme != theme)
                {
                    var generation = preview.ThemeGeneration;
                    var layout = preview.GetViewportAnchor().LayoutGeneration;
                    host.Page.RequestedTheme = theme;
                    await WaitForFontLayoutAsync(preview, extension.SourceSession, generation, layout);
                }
                Check(extension.ActualTheme == theme, "The script preview adopts " + theme + ".");
                foreach (var zoom in new[] { 100, 150, 200 })
                {
                    if (core.GetFontZoomFactor() != zoom)
                    {
                        var generation = preview.ThemeGeneration;
                        var layout = preview.GetViewportAnchor().LayoutGeneration;
                        core.SetFontZoomFactor(zoom);
                        await WaitForFontLayoutAsync(preview, extension.SourceSession, generation, layout);
                    }
                    viewportPhase = theme + "/" + zoom;
                    // The source remains at its physical top. A direct Reveal
                    // would add padding and compete with its pending follow.
                    TraceViewport("font-layout-ready");
                    await UntilAsync(() => Bounds("Base").Length == 4 && Bounds("Raised").Length == 4 &&
                        Bounds("Lowered").Length == 4 && Bounds("Raised", true).Length == 4 && Bounds("Lowered", true).Length == 4,
                        "The root and script probes are visible after real font/theme relayout.");
                    var normal = Bounds("Base");
                    var raised = Bounds("Raised");
                    var lowered = Bounds("Lowered");
                    var raisedReference = Bounds("Raised", true);
                    var loweredReference = Bounds("Lowered", true);
                    Check(raised[1] < normal[1] - 1 && lowered[1] > normal[1] + 1 &&
                        Math.Abs(raised[2] / raisedReference[2] - 5.0 / 6) < 0.05 &&
                        Math.Abs(lowered[2] / loweredReference[2] - 5.0 / 6) < 0.05,
                        "Real script glyph advances are smaller than the same unstyled words and sit above/below their own root row.");
                    var viewportBeforeSelection = preview.GetViewportAnchor();
                    Check(viewportBeforeSelection.IsAvailable && scroll.VerticalOffset == 0 &&
                        viewportBeforeSelection.OffsetWithinLine == 0,
                        "The source-top fixture starts selection at the actual and presented document top.");
                    var layoutBeforeSelection = viewportBeforeSelection.LayoutGeneration;
                    var focusBeforeSelection = preview.FocusState;
                    var windowBeforeSelection = Window.Current.CoreWindow.Bounds;
                    selectionViewportY = scroll.VerticalOffset;
                    selectionViewportMoved = false;
                    trackingSelectionViewport = true;
                    TraceViewport("pre-select");
                    preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, raisedStart, 6);
                    TraceViewport("immediate-select");
                    var immediateSelection = preview.GetSelection();
                    var immediateBounds = Bounds("Raised");
                    await TabDragTestHost.FrameAsync();
                    TraceViewport("after-frame");
                    var selection = text.GetSelection();
                    var actualSelection = preview.GetSelection();
                    var afterSelectionBounds = Bounds("Raised");
                    var viewportAfterSelection = preview.GetViewportAnchor();
                    var actualViewportAfterSelection = scroll.VerticalOffset;
                    trackingSelectionViewport = false;
                    var layoutAfterSelection = viewportAfterSelection.LayoutGeneration;
                    var actualSelectedText = string.Join("|", selection.Select(range => range.GetText(-1)));
                    log.AppendLine($"OBS: ScriptSelection focus={focusBeforeSelection}/{preview.FocusState}; window={windowBeforeSelection}/{Window.Current.CoreWindow.Bounds}; layout={layoutBeforeSelection}/{layoutAfterSelection}; immediate={immediateSelection.Start}+{immediateSelection.Length}; after={actualSelection.Start}+{actualSelection.Length}; UIA={selection.Length}:'{actualSelectedText}'; rectBefore=[{string.Join(",", raised)}]; immediate=[{string.Join(",", immediateBounds)}]; after=[{string.Join(",", afterSelectionBounds)}].");
                    Check(selection.Length == 1 && actualSelectedText == "Raised" &&
                        afterSelectionBounds.SequenceEqual(raised) && layoutAfterSelection == layoutBeforeSelection &&
                        !selectionViewportMoved && actualViewportAfterSelection == selectionViewportY &&
                        viewportAfterSelection.IsAvailable &&
                        viewportAfterSelection.PreviewUtf16Offset == viewportBeforeSelection.PreviewUtf16Offset &&
                        viewportAfterSelection.OffsetWithinLine == viewportBeforeSelection.OffsetWithinLine,
                        "Painting a real script selection preserves its exact text, geometry and published layout.");
                    var mapped = await preview.MapPreviewSpanAsync(identity.DocumentId, identity.Revision,
                        identity.TextGeneration, raisedStart + 1, raisedStart + 4, MarkdownPositionAffinity.After);
                    var bytes = Encoding.UTF8.GetBytes(source);
                    Check(Encoding.UTF8.GetString(bytes, (int)mapped.Start.SourceByteOffset,
                        (int)(mapped.End.SourceByteOffset - mapped.Start.SourceByteOffset)) == "ais",
                        "Script selection maps to the exact original UTF-8 text inside the hidden tags.");
                    preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, raisedStart, 0);
                    await TabDragTestHost.FrameAsync();
                    if (zoom == 100)
                    {
                        var bitmap = new RenderTargetBitmap();
                        await bitmap.RenderAsync(host.Page);
                        var file = await host.Folder.CreateFileAsync("html-scripts-" + theme + ".png", CreationCollisionOption.FailIfExists);
                        using var stream = await file.OpenAsync(FileAccessMode.ReadWrite);
                        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
                        encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied,
                            (uint)bitmap.PixelWidth, (uint)bitmap.PixelHeight, 96, 96, (await bitmap.GetPixelsAsync()).ToArray());
                        await encoder.FlushAsync();
                        log.AppendLine($"OBS: HtmlSupSub {theme}: raster={file.Path}");

                        var crossStart = (ulong)originalPreview.IndexOf("RowSubA", StringComparison.Ordinal);
                        // These rows deliberately sit near the top of the 100%
                        // fixture. Do not queue a scroll while freezing geometry.
                        await UntilAsync(() => Bounds("RowSubA").Length == 4 && Bounds("RowSupB").Length == 4,
                            "The two actual script rows are materialized for selection verification.");
                        var firstRow = Bounds("RowSubA");
                        var nextRow = Bounds("RowSupB");
                        Check(firstRow[3] < normal[3] - 0.5 && nextRow[1] > firstRow[1],
                            "The first-row script selection uses its reduced font height rather than uniform line height.");
                        var crossLayout = preview.GetViewportAnchor().LayoutGeneration;
                        preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, crossStart, 7);
                        await TabDragTestHost.FrameAsync();
                        var crossSelection = text.GetSelection();
                        Check(crossSelection.Length == 1 && crossSelection[0].GetText(-1) == "RowSubA" &&
                            Bounds("RowSubA").SequenceEqual(firstRow) && Bounds("RowSupB").SequenceEqual(nextRow) &&
                            preview.GetViewportAnchor().LayoutGeneration == crossLayout,
                            "Selecting the sub row preserves both rows and the published layout.");
                        var crossBitmap = new RenderTargetBitmap();
                        await crossBitmap.RenderAsync(host.Page);
                        var crossFile = await host.Folder.CreateFileAsync("html-scripts-crossline-" + theme + ".png", CreationCollisionOption.FailIfExists);
                        using var crossStream = await crossFile.OpenAsync(FileAccessMode.ReadWrite);
                        var crossEncoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, crossStream);
                        crossEncoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied,
                            (uint)crossBitmap.PixelWidth, (uint)crossBitmap.PixelHeight, 96, 96, (await crossBitmap.GetPixelsAsync()).ToArray());
                        await crossEncoder.FlushAsync();
                        log.AppendLine($"OBS: HtmlSupSubCrossline {theme}: rowA=[{string.Join(",", firstRow)}]; rowB=[{string.Join(",", nextRow)}]; raster={crossFile.Path}");
                        preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, crossStart, 0);
                    }
                    Check(editor.GetText() == source && extension.SourceSession.IsCurrent,
                        "Font, theme and script selection preserve the original source and accepted revision.");
                    log.AppendLine($"OBS: HtmlSupSub {theme} {zoom}%: rootTop={normal[1]:F2}; supTop={raised[1]:F2}; subTop={lowered[1]:F2}; widths={raised[2]:F2}/{raisedReference[2]:F2}, {lowered[2]:F2}/{loweredReference[2]:F2}.");
                }
            }
            var links = peer.GetChildren().Where(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink).ToArray();
            Check(links.Length == 2, "Adjacent same-destination Markdown links retain two actual accessibility actions.");
            int invoked = 0;
            preview.LinkRequested += (_, args) => { Check(args.Destination == "#target", "Each script link retains its destination."); ++invoked; };
            foreach (var link in links) ((IInvokeProvider)link.GetPattern(PatternInterface.Invoke)).Invoke();
            Check(invoked == 2, "Both distinct links can invoke independently.");

            var session = extension.SourceSession;
            var fullBefore = session.FullSubmissions;
            var deltaBefore = session.DeltaSubmissions;
            var close = Encoding.UTF8.GetByteCount("Base <sup>Raised</");
            core.TestFeatureNative.SetSel(close, close + 3);
            editor.TypeText("sub");
            await CurrentAsync(editor);
            Check(preview.DisplayedText.Contains("Base <sup>Raised</sub> Mid Lowered End.", StringComparison.Ordinal) &&
                session.DeltaSubmissions > deltaBefore && session.FullSubmissions == fullBefore,
                "A mismatched closing script tag restores literal source through the actual native delta.");
            await CompareFullSyncAsync(host, editor, "script closing tag made unbalanced", log);
            core.Undo();
            await CurrentAsync(editor);
            Check(editor.GetText() == source && preview.DisplayedText == originalPreview,
                "Native Undo restores the exact paired tags and projected script text.");
            await CompareFullSyncAsync(host, editor, "script closing tag restored by native Undo", log);
            log.AppendLine("PASS: Markdown.HtmlSupSub verifies real light/dark geometry, 100/150/200% zoom, unchanged selection layout, source mapping, separate links, delta and Undo.");
        }
        finally
        {
            detachViewportTrace();
            var extension = editor.TestMarkdownPreview;
            var preview = extension?.PreviewView;
            var generation = preview?.ThemeGeneration ?? 0;
            var layout = preview?.GetViewportAnchor().LayoutGeneration ?? 0;
            var restoreFont = core.GetFontZoomFactor() != originalZoom;
            var restoreTheme = host.Page.RequestedTheme != originalTheme;
            core.SetFontZoomFactor(originalZoom);
            host.Page.RequestedTheme = originalTheme;
            var restoreSizeAccepted = ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(originalBounds.Width, originalBounds.Height));
            log.AppendLine($"OBS: ScriptWindow restore requested={originalBounds}; accepted={restoreSizeAccepted}; immediate={Window.Current.CoreWindow.Bounds}.");
            if (preview != null && (restoreFont || restoreTheme))
                await WaitForFontLayoutAsync(preview, extension.SourceSession, generation, layout);
            try
            {
                await UntilAsync(() => Math.Abs(Window.Current.CoreWindow.Bounds.Width - originalBounds.Width) < 0.6 &&
                    Math.Abs(Window.Current.CoreWindow.Bounds.Height - originalBounds.Height) < 0.6,
                    "The owned script window restores its size before disposal.");
            }
            catch
            {
                log.AppendLine($"OBS: ScriptWindow restore timed out; requested={originalBounds}; actual={Window.Current.CoreWindow.Bounds}.");
                throw;
            }
            await TabDragTestHost.FrameAsync();
        }
    }
}
