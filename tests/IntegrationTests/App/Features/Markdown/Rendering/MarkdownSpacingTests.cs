// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Media.Imaging;
using Windows.UI.ViewManagement;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    private static async Task CompactSpacingAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "## Contributing:\r\r" +
            "* [How to contribute?](#contributing)\r" +
            "* V2 development: build and CI, dependencies and tests.\r" +
            "* Notepads is free and open source, please consider:\r" +
            "   * Star this project on GitHub\r" +
            "   * Leave me a review [here](#review)\r" +
            "   * [ko-fi](#support)\r\r" +
            "## Dependencies and References:\r" +
            "* Windows Community Toolkit\r* XAML Controls Gallery\r* Windows UI Library\r" +
            "* ColorCode Universal\r* UTF Unknown\r* DiffPlex\r* Win2D\r\r" +
            "## Special Thanks:\r\r" +
            "* Yi Zhou - App icon designer, inspired by the Windows Terminal icon.\r\r" +
            "* Mahmoud Qurashy - App icon and file icon designer.\r\r" +
            "* Code Contributors: Thank you for helping to improve Notepads.";
        var editor = await OpenAsync(host, "compact-spacing.md", Encoding.UTF8.GetBytes(source));
        var core = editor.TestNativeCore;
        var originalBounds = Window.Current.CoreWindow.Bounds;
        var originalZoom = core.GetFontZoomFactor();
        try
        {
            Check(ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(1100, 760)),
                "The spacing fixture admits the same real window size as the pixel-geometry suite.");
            editor.ShowHideContentPreview();
            var extension = await CurrentAsync(editor);
            var preview = extension.PreviewView;
            foreach (var zoom in new[] { 100, 150, 200 })
            {
                if (core.GetFontZoomFactor() != zoom)
                {
                    var theme = preview.ThemeGeneration;
                    var layout = preview.GetViewportAnchor().LayoutGeneration;
                    core.SetFontZoomFactor(zoom);
                    await WaitForFontLayoutAsync(preview, extension.SourceSession, theme, layout);
                }
                await TabDragTestHost.FrameAsync();
                var scale = preview.XamlRoot.RasterizationScale;
                double[] Bounds(string text)
                {
                    var peer = FrameworkElementAutomationPeer.FromElement(preview) ??
                        FrameworkElementAutomationPeer.CreatePeerForElement(preview);
                    var provider = (ITextProvider)peer.GetPattern(PatternInterface.Text);
                    var range = provider.DocumentRange.FindText(text, false, false);
                    Check(range != null, "The actual preview contains the spacing probe: " + text);
                    range.GetBoundingRectangles(out var rectangles);
                    Check(rectangles.Length >= 4 && rectangles[2] > 0 && rectangles[3] > 0,
                        "The spacing probe has current visible native geometry: " + text);
                    return rectangles;
                }
                var first = Bounds("How to contribute?");
                var second = Bounds("V2 development");
                var nested = Bounds("Star this project on GitHub");
                var nestedNext = Bounds("Leave me a review");
                var heading = Bounds("Contributing:");
                var step = core.FontSize * 1.55;
                var rootStep = (second[1] - first[1]) / scale;
                var nestedStep = (nestedNext[1] - nested[1]) / scale;
                Check(Math.Abs(rootStep - step) < 1 && Math.Abs(nestedStep - step) < 1,
                    $"Tight root/nested list rows have one body-line advance without an extra block gap at {zoom}%.");
                Check(Math.Abs(heading[3] / scale - core.FontSize * 1.6 * 1.25) < 1,
                    "The actual heading uses the compact 1.25 line-height ratio.");
                var selection = preview.GetSelection();
                var start = preview.DisplayedText.IndexOf("How to contribute?", StringComparison.Ordinal);
                preview.SelectRange(selection.DocumentId, selection.Revision, selection.TextGeneration,
                    (ulong)start, (ulong)"How to contribute?".Length);
                Check(preview.GetSelection().Start == (ulong)start && editor.GetText() == source,
                    "Compact geometry preserves the actual selection coordinates and original source.");
                log.AppendLine($"OBS: Markdown spacing {zoom}% font={core.FontSize:F2}, body={step:F2}, root={rootStep:F2}, nested={nestedStep:F2}, heading={heading[3] / scale:F2} DIP.");
                if (zoom == 100)
                {
                    var bitmap = new RenderTargetBitmap();
                    await bitmap.RenderAsync(host.Page);
                    var pixels = await bitmap.GetPixelsAsync();
                    var file = await host.Folder.CreateFileAsync("markdown-compact.png", CreationCollisionOption.FailIfExists);
                    using var stream = await file.OpenAsync(FileAccessMode.ReadWrite);
                    var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
                    encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied,
                        (uint)bitmap.PixelWidth, (uint)bitmap.PixelHeight, 96, 96, pixels.ToArray());
                    await encoder.FlushAsync();
                    log.AppendLine("OBS: Markdown compact raster: " + file.Path);
                }
            }
            log.AppendLine("PASS: Markdown.CompactSpacing — actual tight/nested lists and heading geometry at 100/150/200% font zoom; source and selection retained.");
        }
        finally
        {
            core.SetFontZoomFactor(originalZoom);
            ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(originalBounds.Width, originalBounds.Height));
        }
    }
}
