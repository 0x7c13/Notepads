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
    private static async Task InlineCodeBadgesAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string longCode = "a_long_identifier(first_argument, second_argument, third_argument) with more words to wrap across several visible lines";
        const string source = "## 行内代码标签\r\r" +
            "Plain text: `rustfmt` and `unsafe`.\r\r" +
            "* Rust 遵循 stable 工具链与 `rustfmt`；`unsafe` 集中在平台边界。\r" +
            "* C# 使用 `file-scoped namespace`，文件格式为 `UTF-8`、`CRLF`。\r" +
            "* Unicode：`中文 😀 é שלום`；标点紧邻(`x`)，链接 [`docs`](https://example.test/docs)。\r\r" +
            "Long code: `" + longCode + "`.\r\r" +
            "| Name | Value |\r| --- | --- |\r| `code` | Normal text |\r\r" +
            "> 引用里的 `inline code` 也使用相同标签。\r\r" +
            "```rust\r// Fenced code retains its existing block appearance.\rlet value = 42;\r```";
        var editor = await OpenAsync(host, "inline-code.md", Encoding.UTF8.GetBytes(source));
        var core = editor.TestNativeCore;
        var originalZoom = core.GetFontZoomFactor();
        var originalTheme = host.Page.RequestedTheme;
        var originalBounds = Window.Current.CoreWindow.Bounds;
        try
        {
            Check(ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(1100, 760)),
                "The inline-code fixture admits the established real window size.");
            editor.ShowHideContentPreview();
            var extension = await CurrentAsync(editor);
            var preview = extension.PreviewView;
            ITextRangeProvider Find(string text)
            {
                var peer = FrameworkElementAutomationPeer.FromElement(preview) ??
                    FrameworkElementAutomationPeer.CreatePeerForElement(preview);
                var range = ((ITextProvider)peer.GetPattern(PatternInterface.Text)).DocumentRange.FindText(text, false, false);
                Check(range != null && range.GetText(-1) == text, "The actual preview exposes unchanged code text: " + text);
                return range;
            }
            double[] Bounds(string text)
            {
                Find(text).GetBoundingRectangles(out var rectangles);
                return rectangles;
            }
            var identity = preview.GetSelection();
            var start = (ulong)preview.DisplayedText.IndexOf("rustfmt", StringComparison.Ordinal);
            var longStart = (ulong)preview.DisplayedText.IndexOf(longCode, StringComparison.Ordinal);
            foreach (var theme in new[] { ElementTheme.Dark, ElementTheme.Light })
            {
                if (extension.ActualTheme != theme)
                {
                    var generation = preview.ThemeGeneration;
                    var layout = preview.GetViewportAnchor().LayoutGeneration;
                    host.Page.RequestedTheme = theme;
                    await WaitForFontLayoutAsync(preview, extension.SourceSession, generation, layout);
                }
                Check(extension.ActualTheme == theme, "The actual Markdown preview adopted " + theme + ".");
                double baselineWidth = 0;
                foreach (var zoom in new[] { 100, 150, 200 })
                {
                    if (core.GetFontZoomFactor() != zoom)
                    {
                        var generation = preview.ThemeGeneration;
                        var layout = preview.GetViewportAnchor().LayoutGeneration;
                        core.SetFontZoomFactor(zoom);
                        await WaitForFontLayoutAsync(preview, extension.SourceSession, generation, layout);
                    }
                    preview.RevealRange(identity.DocumentId, identity.Revision, identity.TextGeneration, 0, 0, true, false);
                    await UntilAsync(() => Bounds("rustfmt").Length == 4, "The code probe is visible after font relayout.");
                    var code = Bounds("rustfmt");
                    if (zoom == 100) baselineWidth = code[2];
                    Check(Math.Abs(code[2] - baselineWidth * zoom / 100.0) < 1.5,
                        "Badges keep the actual code glyph advances and apply font zoom once.");
                    if (zoom == 100)
                    {
                        preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, start, 0);
                        await TabDragTestHost.FrameAsync();
                        var bitmap = new RenderTargetBitmap();
                        await bitmap.RenderAsync(preview);
                        var pixels = (await bitmap.GetPixelsAsync()).ToArray();
                        var peer = FrameworkElementAutomationPeer.FromElement(preview);
                        var origin = peer.GetBoundingRectangle();
                        var scale = preview.XamlRoot.RasterizationScale;
                        var plain = Bounds("Plain text:");
                        (byte B, byte G, byte R) Sample(double[] rectangle)
                        {
                            var x = (int)Math.Floor(rectangle[0] - origin.X + rectangle[2] / 2);
                            var y = (int)Math.Floor(rectangle[1] - origin.Y + 2 * scale);
                            Check(x >= 0 && y >= 0 && x < bitmap.PixelWidth && y < bitmap.PixelHeight,
                                "The real code/plain pixel probes are inside the rendered preview.");
                            var offset = (y * bitmap.PixelWidth + x) * 4;
                            return (pixels[offset], pixels[offset + 1], pixels[offset + 2]);
                        }
                        var codePixel = Sample(code);
                        var plainPixel = Sample(plain);
                        Check(codePixel != plainPixel, "Actual " + theme + " inline code paints a background distinct from plain text.");
                        var page = new RenderTargetBitmap();
                        await page.RenderAsync(host.Page);
                        var file = await host.Folder.CreateFileAsync("inline-code-" + theme + ".png", CreationCollisionOption.FailIfExists);
                        using var stream = await file.OpenAsync(FileAccessMode.ReadWrite);
                        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
                        encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied,
                            (uint)page.PixelWidth, (uint)page.PixelHeight, 96, 96, (await page.GetPixelsAsync()).ToArray());
                        await encoder.FlushAsync();
                        log.AppendLine($"OBS: InlineCode {theme}: codePixel={codePixel}, plainPixel={plainPixel}, raster={file.Path}");
                    }
                    preview.RevealRange(identity.DocumentId, identity.Revision, identity.TextGeneration,
                        longStart, (ulong)longCode.Length, true, false);
                    await UntilAsync(() => Bounds(longCode).Length >= 8, "Long inline code has multiple visible wrapped fragments.");
                    preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, start + 1, 3);
                    Check(preview.GetSelection().Start == start + 1 && preview.GetSelection().Length == 3,
                        "A partial code selection retains the actual UTF16 range.");
                    var mapped = await preview.MapPreviewSpanAsync(identity.DocumentId, identity.Revision,
                        identity.TextGeneration, start + 1, start + 4, MarkdownPositionAffinity.After);
                    var bytes = Encoding.UTF8.GetBytes(source);
                    Check(Encoding.UTF8.GetString(bytes, (int)mapped.Start.SourceByteOffset,
                        (int)(mapped.End.SourceByteOffset - mapped.Start.SourceByteOffset)) == "ust",
                        "Code badge geometry preserves the source byte mapping inside backticks.");
                    Check(editor.GetText() == source && extension.SourceSession.IsCurrent,
                        "Theme, wrap and font zoom leave the original Markdown source intact.");
                    log.AppendLine($"OBS: InlineCode {theme} {zoom}%: glyphWidth={code[2]:F2}px; wrappedFragments={Bounds(longCode).Length / 4}.");
                }
            }
            log.AppendLine("PASS: Markdown.InlineCode — actual light/dark background pixels, wrapped native geometry, font zoom 100/150/200%, partial selection and source mapping.");
        }
        finally
        {
            var extension = editor.TestMarkdownPreview;
            var preview = extension?.PreviewView;
            var generation = preview?.ThemeGeneration ?? 0;
            var layout = preview?.GetViewportAnchor().LayoutGeneration ?? 0;
            var restoreFont = core.GetFontZoomFactor() != originalZoom;
            var restoreTheme = host.Page.RequestedTheme != originalTheme;
            core.SetFontZoomFactor(originalZoom);
            host.Page.RequestedTheme = originalTheme;
            ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(originalBounds.Width, originalBounds.Height));
            // Restore through published layout before disposing the window;
            // theme/font/resize notifications can otherwise still be queued.
            if (preview != null && (restoreFont || restoreTheme))
                await WaitForFontLayoutAsync(preview, extension.SourceSession, generation, layout);
            await UntilAsync(() => Math.Abs(Window.Current.CoreWindow.Bounds.Width - originalBounds.Width) < 0.6 &&
                Math.Abs(Window.Current.CoreWindow.Bounds.Height - originalBounds.Height) < 0.6,
                "The owned inline-code window restores its size before host disposal.");
            await TabDragTestHost.FrameAsync();
        }
    }
}
