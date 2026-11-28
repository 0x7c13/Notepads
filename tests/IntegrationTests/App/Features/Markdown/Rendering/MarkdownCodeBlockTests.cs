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
using Notepads.Controls;
using Notepads.Markdown;
using Windows.ApplicationModel.Resources;
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
    private static async Task CodeCopyNotificationAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "code-copy.md", Encoding.UTF8.GetBytes("```text\n列表项内的围栏代码\n```"));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        Check(preview.Focus(FocusState.Keyboard), "Keyboard reading focus reveals the code copy action.");
        Button copy = null;
        await UntilAsync(() => (copy = Descendants(preview).OfType<Button>()
            .FirstOrDefault(button => button.Name == "CodeCopyButton" && button.Visibility == Visibility.Visible)) != null,
            "The hosted preview exposes the native code copy button.");
        var icon = (SymbolIcon)copy.Content;
        var notification = (InAppNotification)host.Page.FindName("StatusNotification");
        var message = ResourceLoader.GetForCurrentView().GetString("Clipboard_NotificationMsg_Copied");
        Check(!string.IsNullOrEmpty(message), "The copy notification has a localized message.");

        // Exercise the native click handler and the real workspace notification
        // with an in-process clipboard result, preserving the system clipboard.
        CoreValidationProbe.TestCodeCopy(preview, false);
        Check(icon.Symbol == Symbol.Copy && !Equals(notification.Content, message),
            "A failed clipboard write produces neither the check mark nor the status notification.");
        CoreValidationProbe.TestCodeCopy(preview, true);
        Check(icon.Symbol == Symbol.Accept, "A successful clipboard write shows the check mark.");
        await UntilAsync(() => notification.Visibility == Visibility.Visible && notification.ActualHeight > 0 &&
            Equals(notification.Content, message), "A successful native code copy shows the localized workspace notification.");
        log.AppendLine("PASS: Markdown.CodeCopyNotification — real native and workspace feedback appear only after clipboard success.");
    }

    private static async Task CodeBlockChromeAsync(StringBuilder log)
    {
        var previous = Window.Current.Content;
        var root = new Grid { IsHitTestVisible = false };
        var preview = new MarkdownView
        {
            Width = 420, Height = 300,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        root.Children.Add(preview);
        try
        {
            Window.Current.Content = root;
            Window.Current.Activate();
            await UntilAsync(() => preview.IsLoaded && preview.ActualWidth == 420, "The code preview is arranged.");
            var document = Guid.NewGuid();
            ulong revision = 0;
            async Task<double> MeasureAsync(string markdown, string expected)
            {
                var result = await preview.SetDocumentAsync(document, ++revision, Encoding.UTF8.GetBytes(markdown))
                    .AsTask().WaitAsync(TimeSpan.FromSeconds(25));
                Check(result.Status == DocumentUpdateStatus.Ready && result.Published &&
                    preview.DisplayedText == expected,
                    $"Code layout preserves the complete reading text, including line endings. Actual length {preview.DisplayedText.Length}, expected {expected.Length}.");
                return preview.MeasuredHeight;
            }

            const string command = "  winget install \"Notepads App\"  ";
            var line = await MeasureAsync("ordinary line", "ordinary line");
            var one = await MeasureAsync("```cmd\n" + command + "\n```", command + "\n");
            var withBlank = await MeasureAsync("```cmd\n" + command + "\n\n```", command + "\n\n");
            Check(Math.Abs(one - line - 28) < 1 && Math.Abs(withBlank - one - line) < 1,
                $"A terminal newline adds no painted line, and a deliberate blank line remains. Heights {line}, {one}, {withBlank}.");

            await MeasureAsync("```\n" + command + "\n```", command + "\n");
            Button VisibleCopy() => Descendants(preview).OfType<Button>()
                .SingleOrDefault(button => button.Name == "CodeCopyButton" && button.Visibility == Visibility.Visible);
            Check(preview.Focus(FocusState.Keyboard), "Keyboard focus can reveal the copy affordance in the current code block.");
            await UntilAsync(() => VisibleCopy() is { ActualWidth: 22, ActualHeight: 22 },
                "The copy button appears for keyboard reading focus and fits its reserved space.");
            var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
            var copyPeer = peer.GetChildren().Single(child => child.GetName() == "Copy code block");
            Check(copyPeer.GetPattern(PatternInterface.Invoke) is IInvokeProvider,
                "The code copy button has a name and an Invoke pattern in the reading tree.");

            var multiple = "~~~\nfirst block\n~~~\n\n~~~\nsecond block\n~~~";
            var result = await preview.SetDocumentAsync(document, ++revision, Encoding.UTF8.GetBytes(multiple))
                .AsTask().WaitAsync(TimeSpan.FromSeconds(25));
            Check(result.Published, "The document with two code blocks is published.");
            Check(preview.Focus(FocusState.Keyboard), "The reading caret starts in the first code block.");
            await UntilAsync(() => VisibleCopy() != null, "The first code block exposes Copy.");
            var firstButton = VisibleCopy();
            var firstCopy = peer.GetChildren().Single(child => child.GetName() == "Copy code block");
            var second = ((ITextProvider)peer.GetPattern(PatternInterface.Text)).DocumentRange
                .FindText("second block", false, false);
            Check(second != null, "The second code block has a readable range.");
            var start = Windows.UI.Xaml.Automation.Text.TextPatternRangeEndpoint.Start;
            var end = Windows.UI.Xaml.Automation.Text.TextPatternRangeEndpoint.End;
            second.MoveEndpointByRange(end, second, start);
            second.Select();
            await UntilAsync(() => VisibleCopy() is { } button && button != firstButton && preview.GetSelection().IsAvailable,
                "Keyboard reading focus exposes Copy for the second block.");
            var secondCopy = peer.GetChildren().Single(child => child.GetName() == "Copy code block");
            Check(!ReferenceEquals(firstCopy, secondCopy) && !firstCopy.IsEnabled() &&
                secondCopy.GetPattern(PatternInterface.Invoke) is IInvokeProvider,
                "A stale code block peer becomes unavailable when the reusable button moves to another block.");

            var scroll = Descendants(preview).OfType<ScrollViewer>().Single();
            var chrome = preview.ActualWidth - scroll.ViewportWidth;
            async Task<double> MeasureNarrowAsync(double blockWidth)
            {
                // The default MarkdownTheme contributes 16 DIPs on each side.
                var viewportWidth = 32 + blockWidth;
                preview.Width = chrome + viewportWidth;
                await UntilAsync(() => Math.Abs(scroll.ViewportWidth - viewportWidth) < 0.25,
                    "The intended narrow preview width is arranged.");
                return await MeasureAsync("~~~\n0123456789\n~~~", "0123456789\n");
            }
            var at66 = await MeasureNarrowAsync(66);
            var at67 = await MeasureNarrowAsync(67);
            Check(at67 <= at66 + 0.25,
                $"Widening a narrow code block across the copy threshold cannot add wrapped lines. Heights {at66}, {at67}.");
            preview.Width = 420;
            await UntilAsync(() => scroll.ViewportWidth > 350, "The code fence viewport is wide enough for line numbers.");
            const string fenceBody = "const marker = '会议';\n\nconst second = marker;\n";
            async Task<double> TextLeftAsync(string info)
            {
                await MeasureAsync("```" + info + "\n" + fenceBody + "```", fenceBody);
                var textProvider = (ITextProvider)peer.GetPattern(PatternInterface.Text);
                Check(textProvider.DocumentRange.GetText(-1) == fenceBody, "Code labels are outside the UIA document text.");
                var first = textProvider.DocumentRange.FindText("const marker", false, false);
                Check(first != null, "Code with fence metadata keeps the first source token accessible.");
                first.GetBoundingRectangles(out var bounds);
                Check(bounds.Length >= 4, "The first code token has visible native geometry.");
                return bounds[0];
            }
            var plainLeft = await TextLeftAsync("js");
            var numberedLeft = await TextLeftAsync("js:line-numbers=98 {2-3}");
            var pandocLeft = await TextLeftAsync("{.js .numberLines startFrom=98}");
            var pymdownLeft = await TextLeftAsync("js {linenums=\"98\" hl_lines=\"2 3\"}");
            Check(numberedLeft > plainLeft + 12 && Math.Abs(numberedLeft - pandocLeft) < 1 &&
                Math.Abs(numberedLeft - pymdownLeft) < 1,
                "VitePress, Pandoc and PyMdown numbering share a real gutter without altering the code text.");

            log.AppendLine($"PASS: Markdown.CodeFenceMetadata — preserved text and three dialects share the native number gutter.");
            log.AppendLine($"PASS: Markdown.CodeCopyReview — peer retargeting invalidates the old action and widening 66 to 67 DIP keeps height monotonic ({at66:F1} to {at67:F1}).");
            log.AppendLine($"PASS: Markdown.CodeBlockChrome — one line {one:F1} DIP, deliberate blank {withBlank:F1} DIP, original code text and keyboard copy affordance verified.");
        }
        finally
        {
            root.Children.Remove(preview);
            Window.Current.Content = previous;
        }
    }

}
