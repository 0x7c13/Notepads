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
using Windows.UI.Xaml.Media.Imaging;
using Windows.UI.ViewManagement;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    private static async Task AnimatedGifAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string frames = "R0lGODlhBAADAIEAAAAAANZLMyGofyZk1yH/C05FVFNDQVBFMi4wAwEAAAAh+QQEMgAAACwAAAAABAADAAACCgzDMAzDMAzDMAUAIfkEBDIAAAAsAAAAAAQAAwAAAgoURVEURVEURVEFADs=";
        var gif = await host.Folder.CreateFileAsync("two-frames.gif", CreationCollisionOption.FailIfExists);
        await FileIO.WriteBytesAsync(gif, Convert.FromBase64String(frames));
        const string markdown = "# GIF\n\n<img src=\"two-frames.gif\" alt=\"alternating GIF\" width=\"72\" height=\"54\">\n\nAfter image.";
        var editor = await OpenAsync(host, "animation.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        await UntilAsync(() => ImageState(preview, "alternating GIF", true), "The local GIF is ready in the actual preview.");
        var initial = preview.GetViewportAnchor();
        var text = preview.DisplayedText;
        var animated = new UISettings().AnimationsEnabled;
        async Task ObserveFrames()
        {
            var sawRed = false;
            var sawGreen = false;
            var deadline = DateTime.UtcNow.AddSeconds(5);
            do
            {
                var bitmap = new RenderTargetBitmap();
                await bitmap.RenderAsync(preview);
                var pixels = (await bitmap.GetPixelsAsync()).ToArray();
                var red = 0;
                var green = 0;
                for (var i = 0; i + 3 < pixels.Length; i += 4)
                {
                    if (pixels[i + 3] != 255)
                        continue;
                    if (pixels[i] == 51 && pixels[i + 1] == 75 && pixels[i + 2] == 214)
                        ++red;
                    if (pixels[i] == 127 && pixels[i + 1] == 168 && pixels[i + 2] == 33)
                        ++green;
                }
                sawRed |= red >= 24;
                sawGreen |= green >= 24;
                if (sawRed && (!animated || sawGreen))
                    break;
                await Task.Delay(85);
            } while (DateTime.UtcNow < deadline);
            Check(sawRed && sawGreen == animated, "The preview draws the expected GIF frames under the system animation preference.");
        }
        await ObserveFrames();
        Check(preview.GetViewportAnchor().LayoutGeneration == initial.LayoutGeneration && preview.DisplayedText == text &&
            text.Contains('\uFFFC') && ImageState(preview, "alternating GIF", true),
            "GIF paint updates preserve layout, source text and the one current image peer.");
        preview.Visibility = Visibility.Collapsed;
        try { await Task.Delay(550); }
        finally { preview.Visibility = Visibility.Visible; }
        await UntilAsync(() => preview.IsSurfaceReady && ImageState(preview, "alternating GIF", true),
            "A cached GIF preview restores its current image after becoming visible.");
        await ObserveFrames();
        log.AppendLine($"PASS: Markdown.AnimatedGif verifies preview pixels, stable geometry and cached visibility (system animation {animated}).");
    }

    private static async Task ThemedPictureAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        async Task WriteColor(string name, string color)
        {
            var file = await host.Folder.CreateFileAsync(name, CreationCollisionOption.FailIfExists);
            await FileIO.WriteTextAsync(file,
                $"<svg xmlns='http://www.w3.org/2000/svg' width='72' height='54'><rect width='72' height='54' fill='{color}'/></svg>");
        }

        await WriteColor("light.svg", "#d64b33");
        await WriteColor("dark.svg", "#21a87f");
        await WriteColor("default.svg", "#2664d7");
        const string source = "# Picture\n\n<picture>\n" +
            "<source media='(prefers-color-scheme: dark)' srcset='dark.svg'>\n" +
            "<source media='(prefers-color-scheme: light)' srcset='light.svg'>\n" +
            "<img src='default.svg' alt='themed photo' width='72' height='54'>\n</picture>\n\n" +
            "Inline <picture><source media='(prefers-color-scheme: dark)' srcset='dark.svg'>" +
            "<img src='default.svg' alt='fallback photo' width='72' height='54'></picture> end.";
        var originalTheme = host.Page.RequestedTheme;
        host.Page.RequestedTheme = ElementTheme.Light;
        try
        {
            var editor = await OpenAsync(host, "picture.md", Encoding.UTF8.GetBytes(source));
            editor.ShowHideContentPreview();
            var extension = await CurrentAsync(editor);
            var preview = extension.PreviewView;
            const string expected = "Picture\n\uFFFC\nInline \uFFFC end.";
            Check(preview.DisplayedText == expected, "Each picture owns the one fallback image's Main position.");

            async Task CheckTheme(ElementTheme theme)
            {
                if (host.Page.RequestedTheme != theme)
                {
                    var themeGeneration = preview.ThemeGeneration;
                    var layoutGeneration = preview.GetViewportAnchor().LayoutGeneration;
                    host.Page.RequestedTheme = theme;
                    await WaitForFontLayoutAsync(preview, extension.SourceSession, themeGeneration, layoutGeneration);
                }
                await UntilAsync(() => ImageState(preview, "themed photo", true) && ImageState(preview, "fallback photo", true),
                    "Both pictures expose the authored fallback alt labels and current resource readiness.");
                var deadline = DateTime.UtcNow.AddSeconds(5);
                do
                {
                    var image = new RenderTargetBitmap();
                    await image.RenderAsync(preview);
                    var pixels = (await image.GetPixelsAsync()).ToArray();
                    var light = 0;
                    var dark = 0;
                    var fallback = 0;
                    for (var offset = 0; offset + 3 < pixels.Length; offset += 4)
                    {
                        if (pixels[offset + 3] != 255)
                            continue;
                        var rgb = pixels[offset] | pixels[offset + 1] << 8 | pixels[offset + 2] << 16;
                        if (rgb == 0xd64b33) ++light;
                        if (rgb == 0x21a87f) ++dark;
                        if (rgb == 0x2664d7) ++fallback;
                    }
                    if (theme == ElementTheme.Dark ? dark > 48 && light == 0 && fallback == 0 : light > 24 && fallback > 24 && dark == 0)
                    {
                        Check(preview.DisplayedText == expected, "Theme changes preserve the two picture object positions.");
                        return;
                    }
                    await Task.Delay(60);
                } while (DateTime.UtcNow < deadline);
                Check(false, "The actual preview draws only the current themed picture sources.");
            }

            await CheckTheme(ElementTheme.Light);
            await CheckTheme(ElementTheme.Dark);
            await CheckTheme(ElementTheme.Light);
            log.AppendLine("PASS: Markdown.ThemedPicture verifies current image pixels, fallback and stable Main across light-dark-light.");
        }
        finally
        {
            host.Page.RequestedTheme = originalTheme;
        }
    }

    private static async Task ResourcesAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var screenshots = await host.Folder.CreateFolderAsync("ScreenShots", CreationCollisionOption.FailIfExists);
        var png = await screenshots.CreateFileAsync("fixture.png", CreationCollisionOption.FailIfExists);
        await FileIO.WriteBytesAsync(png, Convert.FromBase64String(PngBase64));
        var svg = await host.Folder.CreateFileAsync("fixture.svg", CreationCollisionOption.FailIfExists);
        await FileIO.WriteTextAsync(svg, "<svg xmlns='http://www.w3.org/2000/svg' width='64' height='32'><rect width='64' height='32' fill='#336699'/><circle cx='32' cy='16' r='10' fill='#ffcc00'/></svg>");
        string jpeg = await CreateDataJpegAsync();
        string markdown = "# Resources\r\r![relative PNG](ScreenShots/fixture.png?raw=true \"Screenshot\")\r\r![relative SVG](fixture.svg)\r\r" +
            "![inline PNG](data:image/png;base64," + PngBase64 + ")\r\r" +
            "![inline JPEG](data:image/jpeg;base64," + jpeg + ")\r\r" +
            "![mismatched JPEG](data:image/jpeg;base64," + PngBase64 + ")\r\r" +
            "![corrupt JPEG](data:image/jpeg;base64,/9j/)\r\r" +
            "![invalid inline PNG](data:image/png;base64,AAAAAAAAAAAA)\r\r" +
            "![invalid inline base64](data:image/png;base64,!!!!)\r\r" +
            "![inline GIF](data:image/gif;base64,R0lGODlh)\r\r" +
            "![after Save As](after.png)\r\rInline $x^2+1$.\r\r```mermaid\rflowchart LR\rA[Start] --> B[Finish]\r```\r\r" +
            "```csharp\rvar answer = 42;\r```";
        var editor = await OpenAsync(host, "resources.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        await UntilAsync(() => ImageState(preview, "relative PNG", true) && ImageState(preview, "relative SVG", true) &&
            ImageState(preview, "inline PNG", true) && ImageState(preview, "inline JPEG", true) &&
            ImageState(preview, "mismatched JPEG", false) && ImageState(preview, "corrupt JPEG", false) &&
            ImageState(preview, "invalid inline PNG", false) &&
            ImageState(preview, "invalid inline base64", false) && ImageState(preview, "inline GIF", false) &&
            ImageState(preview, "after Save As", false), "Relative and inline PNG/JPEG decode through the real provider; invalid and unsupported data remain missing.");
        Check(preview.DisplayedText.Count(value => value == '\uFFFC') >= 5 &&
            !preview.DisplayedText.Contains("flowchart LR", StringComparison.Ordinal) &&
            !preview.DisplayedText.Contains("$x^2+1$", StringComparison.Ordinal) &&
            preview.DisplayedText.Contains("var answer = 42;", StringComparison.Ordinal),
            "Host opt-in renders math and Mermaid objects while code remains selectable text.");
        await CompareFullSyncAsync(host, editor, "images/math/Mermaid/code", log);

        var theme = preview.ThemeGeneration;
        var fontLayout = preview.GetViewportAnchor().LayoutGeneration;
        var zoom = editor.TestNativeCore.GetFontZoomFactor();
        try
        {
            editor.TestNativeCore.SetFontZoomFactor(125);
            await WaitForFontLayoutAsync(preview, extension.SourceSession, theme, fontLayout);
            await UntilAsync(() => ImageState(preview, "relative PNG", true) && ImageState(preview, "relative SVG", true) && ImageState(preview, "inline JPEG", true),
                "Current font layout preserves committed PNG and SVG outcomes after the asynchronous redraw.", 25000);
            Check(ImageState(preview, "relative PNG", true) && ImageState(preview, "relative SVG", true) && ImageState(preview, "inline JPEG", true),
                "Changing font zoom retains committed relative image outcomes.");
        }
        finally { editor.TestNativeCore.SetFontZoomFactor(zoom); }

        var moved = await host.Folder.CreateFolderAsync("save-as-resources", CreationCollisionOption.FailIfExists);
        await png.CopyAsync(moved, "after.png", NameCollisionOption.FailIfExists);
        var destination = await moved.CreateFileAsync("renamed.md", CreationCollisionOption.FailIfExists);
        TestFileSavePicker.Results.Enqueue(destination);
        Check(await host.Page.TestSaveDocumentAsync(editor, saveAs: true), "The real Save As command commits the supplied owned destination.");
        await CurrentAsync(editor);
        await UntilAsync(() => ImageState(preview, "after Save As", true) && ImageState(preview, "relative PNG", false) &&
            ImageState(preview, "relative SVG", false) && ImageState(preview, "inline PNG", true) && ImageState(preview, "inline JPEG", true),
            "Save As changes the relative-image base while inline PNG/JPEG remain independent of the document folder.");
        Check(editor.EditingFile.IsEqual(destination) && editor.GetText() == markdown,
            "Resource rebinding preserves the native source and adopts only the new document file.");
        log.AppendLine("PASS: Markdown.Resources — actual PNG/JPEG/SVG image loaded states, missing resource, math/Mermaid objects, code, zoom and Save As relative-base rebinding.");
    }

    private static async Task CompactImageFallbackAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var badges = "[![ ](missing.png)](#badge-0 \" \")" +
            string.Concat(Enumerable.Range(1, 7).Select(i => $"[![](missing.png)](#badge-{i})"));
        var markdown = "# Resources\r\r" + badges + "\r\r" +
            "[![ko-fi](missing.png)](#resources)\r\rBefore ![](missing.png) after";
        var editor = await OpenAsync(host, "compact-image-fallback.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        await UntilAsync(() =>
        {
            var children = peer.GetChildren();
            return children?.Count(child => child.GetAutomationControlType() == AutomationControlType.Image &&
                child.GetHelpText() == "Image not loaded: the host did not provide it") == 10;
        }, "All ten missing resources settle to the published failure state.", 25000);

        var objects = peer.GetChildren();
        var links = objects.Where(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink).ToArray();
        var icons = links.Where(link => link.GetName().StartsWith("#badge-", StringComparison.Ordinal)).ToArray();
        var scale = preview.XamlRoot.RasterizationScale;
        var bounds = icons.Select(icon => icon.GetBoundingRectangle()).ToArray();
        Check(bounds.Length == 8 && bounds.All(rect => rect.Width / scale is > 6 and < 32 &&
            Math.Abs(rect.Y - bounds[0].Y) <= 1), "Eight icon-only failed links fit one compact line and retain a clickable area.");
        var badge = links.Single(link => link.GetName() == "ko-fi");
        var badgeWidth = badge.GetBoundingRectangle().Width / scale;
        Check(badgeWidth > bounds[0].Width / scale && badgeWidth < 110,
            "The linked fallback occupies only the icon and alternative text width.");
        var decorative = objects.Where(child => child.GetAutomationControlType() == AutomationControlType.Image).Last();
        Check(decorative.GetBoundingRectangle().Width == 0 && preview.DisplayedText.Count(value => value == '\uFFFC') == 10,
            "A failed decorative image uses zero width while its source and reading span stay addressable.");

        string destination = null;
        preview.LinkRequested += (_, args) => destination = args.Destination;
        ((IInvokeProvider)icons[0].GetPattern(PatternInterface.Invoke)).Invoke();
        Check(destination == "#badge-0", "An icon-only fallback still invokes its outer link.");
        ((IInvokeProvider)badge.GetPattern(PatternInterface.Invoke)).Invoke();
        Check(destination == "#resources", "A fallback with alternative text still invokes its outer link.");
        log.AppendLine($"PASS: Markdown.CompactImageFallback — eight links at {bounds[0].Width / scale:F1} DIP share one row; linked alt is {badgeWidth:F1} DIP; decorative image is zero width.");
    }

}
