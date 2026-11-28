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
using CommunityToolkit.WinUI;
using Notepads.Markdown;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace;
using NotepadsWorkspaceTests;
using Windows.Storage;
using Windows.Storage.Streams;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
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
    private const string PngBase64 = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEklEQVR4nGP4z8DwHwyBNBgAAEnICff5q7YNAAAAAElFTkSuQmCC";

    internal static async Task RunSelectionStabilityAsync(StringBuilder log)
    {
        await PendingSourceCaretSelectionAsync(log);
        for (var round = 1; round <= 3; round++)
        {
            log.AppendLine($"OBS: ScriptSelection round={round}.");
            await HtmlSupSubAsync(log);
        }
    }

    internal static async Task RunImageCacheStabilityAsync(StringBuilder log)
    {
        await RunCacheReflowAndAutomationAsync(log, "image");
        await RunCacheReflowAndAutomationAsync(log, "image", previewLed: true);
    }
    internal static Task RunWrappedStabilityAsync(StringBuilder log) => WrappedViewportAsync(log);

    internal static async Task RunQuickWinsAsync(StringBuilder log)
    {
        await ChineseMathTextAsync(log);
        await ResourcesAsync(log);
    }

    internal static async Task RunAsync(StringBuilder log)
    {
        await GitHubAutolinksAsync(log);
        await LinkClickablePointAsync(log);
        await GitHubHeadingFragmentsAsync(log);
        await CrossFileFragmentsAsync(log);
        await PreviewDestinationPathsAsync(log);
        await CustomFragmentsAsync(log);
        await TableOfContentsAsync(log);
        await LocalWikiLinksAsync(log);
        await LocalNoteEmbedsAsync(log);
        await DraftNoteEditRetentionAsync(log);
        await RunBrokerNotesAsync(log);
        await FootnotesAsync(log);
        await CalloutsAsync(log);
        await DisclosuresAsync(log);
        await DefinitionListsAsync(log);
        await MathEscapeLinkAsync(log);
        await MathRenderingAsync(log);
        await MathAliasesAsync(log);
        await ChineseMathTextAsync(log);
        await HtmlBlocksAsync(log);
        await HtmlTagFilterAsync(log);
        await HtmlInlineBreaksAsync(log);
        await HtmlInlineEmphasisAsync(log);
        await HtmlInlineCodeAsync(log);
        await HtmlSupSubAsync(log);
        await InlineScriptSyntaxAsync(log);
        await RubyAnnotationAsync(log);
        await EmojiShortcodesAsync(log);
        await EmptyHtmlSeparatorsAsync(log);
        await CompactSpacingAsync(log);
        await InlineCodeBadgesAsync(log);
        await RunPixelGeometryAsync(log);
        await EditingAsync(log);
        await WrappedViewportAsync(log);
        await CaretScrollArbitrationAsync(log);
        await ScrollProgressFollowAsync(log);
        await WindowMoveMappingAsync(log);
        await ResourcesAsync(log);
        await CompactImageFallbackAsync(log);
        await AnimatedGifAsync(log);
        await ThemedPictureAsync(log);
        var light = Windows.UI.Color.FromArgb(255, 240, 240, 240);
        var dark = Windows.UI.Color.FromArgb(255, 37, 37, 37);
        var blue = Windows.UI.Color.FromArgb(255, 0, 80, 170);
        var adjustedLight = MarkdownExtensionView.GetReadableLinkColor(light, Windows.UI.Colors.Black, light,
            Windows.UI.Color.FromArgb(24, 0, 0, 0));
        var adjustedDark = MarkdownExtensionView.GetReadableLinkColor(dark, Windows.UI.Colors.White, dark,
            Windows.UI.Color.FromArgb(24, 255, 255, 255));
        Check(adjustedLight.R is >= 94 and <= 99 && adjustedDark.R is >= 161 and <= 165 &&
            MarkdownExtensionView.GetReadableLinkColor(blue, Windows.UI.Colors.Black, light,
                Windows.UI.Color.FromArgb(24, 0, 0, 0)) == blue,
            $"Matching accent and background are corrected in both themes, and a readable accent stays exact. Light={adjustedLight}, dark={adjustedDark}.");
        log.AppendLine("PASS: Markdown.LinkContrast — light and dark matching colors are corrected, readable accent is preserved.");
        await CodeBlockChromeAsync(log);
        await CodeCopyNotificationAsync(log);
        await RunCacheLifecycleAsync(log);
        await SourceLimitAsync(log);
    }

    private static async Task GitHubAutolinksAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "> 中😀 www.example.org/path,\r\n> alice&#64;example.net.\r\n\r\n" +
            "(https://example.org/a(b)). `www.literal.example`";
        (string Label, string Destination)[] expectedLinks =
        [
            ("www.example.org/path", "http://www.example.org/path"),
            ("alice@example.net", "mailto:alice@example.net"),
            ("https://example.org/a(b)", "https://example.org/a(b)")
        ];
        var editor = await OpenAsync(host, "github-autolinks.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        await UntilAsync(() =>
        {
            var links = peer.GetChildren()?.Where(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink).ToArray();
            return links?.Length == expectedLinks.Length &&
                expectedLinks.All(expected => links.Any(link => link.GetName() == expected.Label &&
                    link.GetHelpText() == expected.Destination && link.GetBoundingRectangle().Width > 0 &&
                    link.GetPattern(PatternInterface.Invoke) is IInvokeProvider));
        }, "Bare GitHub URLs and email have exact link labels, destinations, geometry and accessible actions.");
        Check(preview.DisplayedText.Contains("(https://example.org/a(b)). www.literal.example", StringComparison.Ordinal),
            "The native preview preserves punctuation and inline code text.");
        Check(editor.GetText() == source.Replace("\r\n", "\r", StringComparison.Ordinal),
            "Previewing autolinks preserves the editor's canonical source.");
        log.AppendLine("PASS: Markdown.GitHubAutolinks verifies bare link text, destinations, geometry, accessible actions and unchanged source.");
    }

    private static async Task LinkClickablePointAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var source = "Before the link, a visible prefix: [link tail  \nZ](https://example.org/point) after the link." +
            string.Concat(Enumerable.Repeat("\n\nMore preview text.", 120));
        var editor = await OpenAsync(host, "link-clickable-point.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        // Keep the source viewport from overriding this programmatic preview scroll.
        editor.TestMarkdownPreview.FollowSource = false;
        var preview = (await CurrentAsync(editor)).PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        AutomationPeer link = null;
        double[] rectangles = null;
        await UntilAsync(() =>
        {
            link = peer.GetChildren()?.SingleOrDefault(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink);
            if (link?.GetPattern(PatternInterface.TextChild) is not ITextChildProvider textChild) return false;
            textChild.TextRange.GetBoundingRectangles(out rectangles);
            return rectangles.Length >= 8;
        }, "The link has accessible rectangles on both sides of a hard break.");

        bool InsideLink(double x, double y)
        {
            for (int i = 0; i < rectangles.Length; i += 4)
                if (x > rectangles[i] && x < rectangles[i] + rectangles[i + 2] &&
                    y > rectangles[i + 1] && y < rectangles[i + 1] + rectangles[i + 3])
                    return true;
            return false;
        }

        var bounds = link.GetBoundingRectangle();
        Check(!InsideLink(bounds.X + bounds.Width / 2, bounds.Y + bounds.Height / 2),
            "The fixture puts the bounding rectangle center outside the link text.");
        var point = link.GetClickablePoint();
        Check(InsideLink(point.X, point.Y), "The accessible click point lies inside one of the visible link fragments.");
        var scroll = (ScrollViewer)preview.FindDescendant("PreviewScrollViewer");
        await UntilAsync(() => scroll.ScrollableHeight > scroll.ViewportHeight, "The preview can scroll past the link.");
        Check(preview.TrySetVerticalScrollProgress(1), "The preview accepts a scroll to the end.");
        await UntilAsync(() => preview.VerticalScrollProgress == 1, "The preview reaches the end.");
        await UntilAsync(link.IsOffscreen, "The link is outside the preview viewport.");
        var offscreen = link.GetClickablePoint();
        Check(double.IsNaN(offscreen.X) && double.IsNaN(offscreen.Y), "The offscreen link has no clickable point.");
        log.AppendLine("PASS: Markdown.LinkClickablePoint — a multiline link reports a point on its text when visible and no point when offscreen.");
    }

    private static async Task GitHubHeadingFragmentsAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "[Jump](#anchor--demo) [Second](#anchor--demo-1) [Unicode](#%E8%BE%B9%E7%BC%98)\r\n\r\n" +
            "## &nbsp;&#32;Anchor  *Demo*&#32;&nbsp;\r\n\r\n## Anchor  Demo\r\n\r\n## &#32;边缘&#32;\r\n";
        var editor = await OpenAsync(host, "github-heading-fragments.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var frame = preview.GetSelection();
        Check(preview.GetOutline().Count == 3 && preview.DisplayedText.Contains("\u00a0 Anchor  Demo \u00a0", StringComparison.Ordinal),
            "Outline headings stay available and retain their authored display text.");
        foreach (var fragment in new[] { "#anchor--demo", "#anchor--demo-1", "#%E8%BE%B9%E7%BC%98" })
        {
            await UntilAsync(() => preview.NavigateToFragment(frame.DocumentId, frame.Revision, frame.TextGeneration, fragment) ==
                MarkdownFragmentResult.Applied, $"The published preview can navigate to {fragment}.");
        }
        Check(preview.NavigateToFragment(frame.DocumentId, frame.Revision, frame.TextGeneration, "#-anchor--demo-") ==
            MarkdownFragmentResult.NotFound,
            "The fragment index rejects an extra-hyphen spelling.");
        log.AppendLine("PASS: Markdown.GitHubHeadingFragments verifies canonical and duplicate anchors, percent-encoded Unicode and displayed text.");
    }

    private static async Task CrossFileFragmentsAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var file = await host.Folder.CreateFileAsync("target #guide.md");
        var targetText = "# Start\r\n\r\n" + string.Concat(Enumerable.Repeat("A paragraph above the target.\r\n\r\n", 70)) +
            "## 安装\r\n\r\nThe destination paragraph.\r\n";
        await FileIO.WriteTextAsync(file, targetText);
        var source = await OpenAsync(host, "cross-file-links.md", Encoding.UTF8.GetBytes(
            "[Open section](target%20%23guide.md?view=local#%E5%AE%89%E8%A3%85) · [Open top](target%20%23guide.md#)"));
        source.ShowHideContentPreview();

        async Task InvokeSourceLinkAsync(string label)
        {
            var preview = (await CurrentAsync(source)).PreviewView;
            var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
            IInvokeProvider action = null;
            await UntilAsync(() =>
            {
                action = peer.GetChildren()?.FirstOrDefault(link => link.GetName() == label)?.GetPattern(PatternInterface.Invoke) as IInvokeProvider;
                return action != null;
            }, $"Find the Markdown link '{label}'.");
            action.Invoke();
        }

        await InvokeSourceLinkAsync("Open section");
        await UntilAsync(() => host.Core.GetSelectedReadyEditor()?.EditingFile?.IsEqual(file) == true,
            "The relative link opens the target with its decoded filename.");
        var target = (TextEditor)host.Core.GetSelectedReadyEditor();
        var targetExtension = await CurrentAsync(target);
        var targetPreview = targetExtension.PreviewView;
        var scroll = Descendants(targetPreview).OfType<ScrollViewer>().Single(element => element.Name == "PreviewScrollViewer");
        var targetPeer = FrameworkElementAutomationPeer.FromElement(targetPreview) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(targetPreview);
        var reading = (ITextProvider)targetPeer.GetPattern(PatternInterface.Text);
        bool SectionVisible() => targetPreview.GetViewportAnchor().IsAvailable && reading.GetVisibleRanges().Any(range =>
            range.GetText(-1).Contains("The destination paragraph.", StringComparison.Ordinal));
        await UntilAsync(() => targetExtension.TestResourceLinkQuiescent && scroll.ViewportHeight > 0 &&
            scroll.VerticalOffset > scroll.ViewportHeight && SectionVisible(),
            "The newly opened preview retains the Unicode destination after viewport follow settles.");

        var tabCount = host.Core.GetTabCount();
        host.Core.SwitchTo(source);
        await InvokeSourceLinkAsync("Open top");
        await UntilAsync(() => ReferenceEquals(host.Core.GetSelectedReadyEditor(), target) &&
            targetExtension.TestResourceLinkQuiescent && scroll.VerticalOffset < 1,
            "The empty fragment reveals the top of the already open target.");
        Check(host.Core.GetTabCount() == tabCount, "A link to an open file reuses its tab.");

        target.TestNativeCore.SetText(targetText + "\r\nBefore a deferred jump.\r\n");
        Check(!targetExtension.SourceSession.IsCurrent, "The first edit retires the published preview.");
        Check(target.QueueMarkdownFragment("#安装"), "The target queues navigation while its preview updates.");
        target.TestNativeCore.SetText(targetText + "\r\nAfter a deferred jump.\r\n");
        await CurrentAsync(target);
        await UntilAsync(() => targetExtension.TestResourceLinkQuiescent && targetPreview.GetViewportAnchor().IsAvailable,
            "The new revision and its viewport follow settle after the queued navigation.");
        Check(scroll.VerticalOffset < 1 && !SectionVisible(), "A source edit supersedes the older pending fragment.");
        var slowFile = await host.Folder.CreateFileAsync("slow-link.md");
        var fastFile = await host.Folder.CreateFileAsync("fast-link.md");
        await FileIO.WriteTextAsync(slowFile, "# Slow destination");
        await FileIO.WriteTextAsync(fastFile, "# Fast destination");
        var started = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
        var unblock = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
        var core = (NotepadsCore)host.Core;
        core.BeforeFilePreparation = () =>
        {
            started.TrySetResult(null);
            return unblock.Task;
        };
        var slowOpen = host.Page.OpenMarkdownLinkAsync(slowFile, null, false);
        try
        {
            await started.Task.WaitAsync(TimeSpan.FromSeconds(10));
            core.BeforeFilePreparation = null;
            Check(await host.Page.OpenMarkdownLinkAsync(fastFile, null, false),
                "The newer link opens while older file preparation is blocked.");
        }
        finally
        {
            core.BeforeFilePreparation = null;
            unblock.TrySetResult(null);
            await slowOpen;
        }
        Check(host.Core.GetSelectedReadyEditor()?.EditingFile?.IsEqual(fastFile) == true &&
            host.Core.FindReadyTab(slowFile) == null,
            "The cancelled file request cannot install its tab or displace the newer link.");

        host.Core.SwitchTo(source);
        var sourceExtension = await CurrentAsync(source);
        started = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
        unblock = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
        core.BeforeFilePreparation = () =>
        {
            started.TrySetResult(null);
            return unblock.Task;
        };
        var originCurrent = true;
        slowOpen = host.Page.OpenMarkdownLinkAsync(slowFile, null, false, () => originCurrent);
        try
        {
            await started.Task.WaitAsync(TimeSpan.FromSeconds(10));
            originCurrent = false;
            sourceExtension.QueueFragment("#");
        }
        finally
        {
            core.BeforeFilePreparation = null;
            unblock.TrySetResult(null);
            await slowOpen;
        }
        Check(ReferenceEquals(host.Core.GetSelectedReadyEditor(), source) && host.Core.FindReadyTab(slowFile) == null,
            "Retiring the originating navigation before publication keeps the source tab selected.");

        foreach (var keepOrigin in new[] { false, true })
        {
            host.Core.SwitchTo(source);
            var sharedFile = await host.Folder.CreateFileAsync($"shared-open-{keepOrigin}.md");
            await FileIO.WriteTextAsync(sharedFile, "# Shared destination");
            started = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
            unblock = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
            core.BeforeFilePreparation = () =>
            {
                started.TrySetResult(null);
                return unblock.Task;
            };
            var ordinaryOpen = host.Page.OpenFileAsync(sharedFile);
            Task<bool> sharedLink = null;
            var current = true;
            try
            {
                await started.Task.WaitAsync(TimeSpan.FromSeconds(10));
                sharedLink = host.Page.OpenMarkdownLinkAsync(sharedFile, null, false,
                    () => current && ReferenceEquals(host.Core.GetSelectedReadyEditor(), source));
                current = keepOrigin;
            }
            finally
            {
                core.BeforeFilePreparation = null;
                unblock.TrySetResult(null);
                Check(await ordinaryOpen, "The ordinary opener publishes its own tab.");
            }
            Check(await sharedLink == keepOrigin,
                "A shared file result carries the Markdown origin's validity at publication.");
        }

        log.AppendLine("PASS: Markdown.CrossFileFragments covers decoded targets, tab reuse, and superseded edits and file opens.");
    }

    private static async Task CustomFragmentsAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var gap = string.Concat(Enumerable.Repeat("A paragraph between fragment targets.\n\n", 45));
        var source = "# Start\n\n" + gap +
            "<a name='legacy'></a>\n\nLegacy anchor target.\n\n" + gap +
            "<p id='中文&amp;Exact'>HTML ID target.</p>\n\n" + gap +
            "## Styled *heading* {#Manual} ##\n\nFirst manual ID target.\n\n" + gap +
            "## Duplicate heading {#Manual}\n\nSecond manual ID target.\n\n" + gap +
            "<h2 id='html-title'>Literal {#visible}</h2>\n\nHTML heading target.\n\n" + gap +
            "Before <span id='inline'>*Inline target.*</span> and <a name='inline-legacy'>another anchor</a>.\n\n" + gap +
            "<hr id='thematic'>\n\nThematic break target.\n\n" + gap +
            "<table id='table'><caption>Anchor table</caption><tr><th>Column</th></tr>" +
            "<tr id='row'><td id='cell'><a name='cell-legacy'>Table cell target.</a></td></tr></table>\n\n" + gap +
            "`<a name='code'></a>`\n";
        var editor = await OpenAsync(host, "custom-fragments.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        extension.FollowSource = false;
        var preview = extension.PreviewView;
        var outline = preview.GetOutline();
        var headings = outline.GetHeadings(0, outline.Count);
        Check(outline.Count == 4 && headings[1].Title == "Styled heading" &&
            headings[1].Anchor == "Manual" && headings[3].Title == "Literal {#visible}" &&
            headings[3].Anchor == "html-title", "Only headings appear in the outline and authored IDs keep their exact spelling.");
        Check(!preview.DisplayedText.Contains("name='legacy'", StringComparison.Ordinal),
            "An empty named anchor leaves no visible tag or label.");

        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        foreach (var (fragment, marker) in new[]
        {
            ("#legacy", "Legacy anchor target."),
            ("#%E4%B8%AD%E6%96%87%26Exact", "HTML ID target."),
            ("#Manual", "First manual ID target."),
            ("#html-title", "HTML heading target."),
            ("#inline", "Inline target."),
            ("#inline-legacy", "another anchor"),
            ("#thematic", "Thematic break target."),
            ("#table", "Anchor table"),
            ("#row", "Table cell target."),
            ("#cell", "Table cell target."),
            ("#cell-legacy", "Table cell target."),
        })
        {
            await UntilAsync(() =>
            {
                var frame = preview.GetSelection();
                return frame.IsAvailable && preview.NavigateToFragment(frame.DocumentId, frame.Revision,
                    frame.TextGeneration, fragment) == MarkdownFragmentResult.Applied;
            }, $"Navigate to {fragment}.");
            await UntilAsync(() => preview.GetViewportAnchor().IsAvailable && reading.GetVisibleRanges().Any(range =>
                range.GetText(-1).Contains(marker, StringComparison.Ordinal)), $"The matching target for {fragment} is visible.");
        }
        var current = preview.GetSelection();
        foreach (var missing in new[] { "#manual", "#code", "#visible" })
        {
            Check(preview.NavigateToFragment(current.DocumentId, current.Revision, current.TextGeneration, missing) ==
                MarkdownFragmentResult.NotFound, $"{missing} is absent from the anchor index.");
        }
        log.AppendLine("PASS: Markdown.CustomFragments reveals authored HTML and Markdown targets, checks exact IDs, duplicates, and literal content.");
    }

    private static async Task CalloutsAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "> [!NOTE]\n> Default note body.\n\n" +
            "> [!WARNING] 部署 **前**\n> Custom warning body.\n\n" +
            "> > [!FAQ]\n> > Nested question body.\n\n" +
            "> [!unknown] Ordinary quote.\n";
        var editor = await OpenAsync(host, "callouts.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var main = preview.DisplayedText;
        Check(main.Contains("Note\nDefault note body.", StringComparison.Ordinal) &&
            main.Contains("部署 前\nCustom warning body.", StringComparison.Ordinal) &&
            main.Contains("Question\nNested question body.", StringComparison.Ordinal) &&
            main.Contains("[!unknown] Ordinary quote.", StringComparison.Ordinal),
            "Built-in labels, authored headings, nested aliases and ordinary quote text reach the native preview.");
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        Check(reading.DocumentRange.GetText(-1) == main,
            "The automation document includes the same selectable callout titles.");
        var canonical = editor.GetText();
        var title = canonical.IndexOf("部署", StringComparison.Ordinal);
        var position = checked((ulong)Encoding.UTF8.GetByteCount(canonical.AsSpan(0, title)));
        var frame = preview.GetSelection();
        var mapped = await preview.MapSourcePositionAsync(frame.DocumentId, frame.Revision, frame.TextGeneration,
            position, MarkdownPositionAffinity.After);
        Check(mapped.PreviewUtf16Offset == checked((ulong)main.IndexOf("部署", StringComparison.Ordinal)),
            "Authored title text maps to its actual preview location.");
        log.AppendLine("PASS: Markdown.Callouts verifies default and authored titles, nested aliases, literal fallback, source positions and UIA text.");
    }

    private static async Task DisclosuresAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string heading = "Inside nested disclosure";
        const string htmlSummary = "HTML disclosure summary";
        const string htmlBody = "Strong text in HTML details";
        var gap = string.Concat(Enumerable.Repeat("> > Spacer in the folded body.\n> >\n", 32));
        var source = "[Jump to nested content](#inside-nested-disclosure)\n\n" +
            "> [!NOTE]- Outer disclosure\n> Hidden [body link](https://example.com/)\n>\n" +
            "> > [!TIP]+ Inner disclosure\n" + gap + "> > ## Inside nested disclosure\n> > Hidden inner body.\n\n" +
            "<details>\n<summary>HTML disclosure <em>summary</em></summary>\n\n" +
            "**Strong text in HTML details**\n\n- Native list item\n\n</details>\n\n" +
            "<details open><summary>Open HTML details</summary><p>Visible HTML body.</p></details>\n\n" +
            "Visible after both.\n";
        var editor = await OpenAsync(host, "disclosure.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var canonical = editor.GetText();
        var initial = preview.GetSelection();
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        var document = reading.DocumentRange;
        var hidden = document.FindText(heading, false, false);
        Check(hidden != null && (bool)hidden.GetAttributeValue((int)AutomationTextAttributesEnum.IsHiddenAttribute),
            "A collapsed region retains its searchable text and marks it hidden in UI Automation.");
        AutomationPeer Child(string name) => peer.GetChildren()?.FirstOrDefault(child => child.GetName() == name);
        IExpandCollapseProvider Pattern(string name) => Child(name)?.GetPattern(PatternInterface.ExpandCollapse) as IExpandCollapseProvider;
        bool Visible(string text) => preview.GetViewportAnchor().IsAvailable && reading.GetVisibleRanges().Any(range =>
            range.GetText(-1).Contains(text, StringComparison.Ordinal));
        Check(Pattern("Outer disclosure")?.ExpandCollapseState == ExpandCollapseState.Collapsed &&
            Child("Inner disclosure") == null && Child("body link") == null && !Visible(heading),
            "The outer control starts collapsed and omits the nested control, link and reading geometry.");
        Check(Pattern(htmlSummary)?.ExpandCollapseState == ExpandCollapseState.Collapsed &&
            Pattern("Open HTML details")?.ExpandCollapseState == ExpandCollapseState.Expanded && !Visible(htmlBody),
            "HTML details uses the same provider for split Markdown bodies and complete HTML fragments.");
        Pattern(htmlSummary).Expand();
        await UntilAsync(() => preview.GetViewportAnchor().IsAvailable && Visible(htmlBody),
            "Expanding HTML details reveals its original Markdown body.");
        Pattern("Outer disclosure").Expand();
        await UntilAsync(() => preview.GetViewportAnchor().IsAvailable &&
            Pattern("Outer disclosure")?.ExpandCollapseState == ExpandCollapseState.Expanded &&
            Pattern("Inner disclosure")?.ExpandCollapseState == ExpandCollapseState.Expanded && Child("body link") != null,
            "Expanding the outer control exposes the nested control and original link.");
        Check(!((bool)hidden.GetAttributeValue((int)AutomationTextAttributesEnum.IsHiddenAttribute)),
            "The same UI Automation range becomes visible in the new layout.");
        Pattern("Inner disclosure").Collapse();
        await UntilAsync(() => preview.GetViewportAnchor().IsAvailable &&
            Pattern("Inner disclosure")?.ExpandCollapseState == ExpandCollapseState.Collapsed && !Visible(heading),
            "Closing the nested control drops the heading geometry.");
        var jump = (IInvokeProvider)Child("Jump to nested content").GetPattern(PatternInterface.Invoke);
        jump.Invoke();
        await UntilAsync(() => preview.GetViewportAnchor().IsAvailable &&
            Pattern("Inner disclosure")?.ExpandCollapseState == ExpandCollapseState.Expanded && Visible(heading),
            "Activating a fragment opens the necessary disclosure and reveals the heading.");
        Pattern("Inner disclosure").Collapse();
        await UntilAsync(() => preview.GetViewportAnchor().IsAvailable &&
            Pattern("Inner disclosure")?.ExpandCollapseState == ExpandCollapseState.Collapsed,
            "The heading can be hidden again after fragment navigation.");
        preview.TrySetVerticalScrollProgress(0);
        hidden.ScrollIntoView(true);
        await UntilAsync(() => Pattern("Inner disclosure")?.ExpandCollapseState == ExpandCollapseState.Expanded &&
            preview.VerticalScrollProgress > 0 && Visible(heading),
            "One UI Automation request expands the disclosure and scrolls after XAML measures the new extent.");
        Pattern("Inner disclosure").Collapse();
        await UntilAsync(() => preview.GetViewportAnchor().IsAvailable &&
            Pattern("Inner disclosure")?.ExpandCollapseState == ExpandCollapseState.Collapsed,
            "The disclosure closes before testing supersession.");
        var headingStart = (ulong)preview.DisplayedText.IndexOf(heading, StringComparison.Ordinal);
        preview.RevealRange(initial.DocumentId, initial.Revision, initial.TextGeneration, headingStart, (ulong)heading.Length, true, true);
        Check(preview.TrySetVerticalScrollProgress(0), "A newer scroll request takes authority while disclosure layout is pending.");
        await UntilAsync(() => preview.GetViewportAnchor().IsAvailable &&
            Pattern("Inner disclosure")?.ExpandCollapseState == ExpandCollapseState.Expanded,
            "The staged disclosure expansion completes after the newer scroll request.");
        Check(preview.VerticalScrollProgress == 0 && !Visible(heading),
            "The superseded disclosure reveal preserves the newer top viewport.");
        var final = preview.GetSelection();
        Check(initial.TextGeneration == final.TextGeneration && document.GetText(-1) == preview.DisplayedText && editor.GetText() == canonical,
            "Disclosure interactions preserve the original source and text identity.");
        log.AppendLine("PASS: Markdown.Disclosures verifies nested callouts, both HTML forms, accessibility, stable text and scroll supersession.");
    }

    private static async Task DefinitionListsAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "術語 **一**\r\n: First definition\r\n: Second definition\r\n\r\n" +
            "Another term\r\n~ Tilde definition\r\n\r\nOrdinary paragraph.\r\n";
        var editor = await OpenAsync(host, "definition-lists.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        var document = reading.DocumentRange;
        const string result = "術語 一\nFirst definition\nSecond definition\nAnother term\nTilde definition\nOrdinary paragraph.";
        Check(document.GetText(-1) == result && preview.DisplayedText == result,
            "The native preview preserves every term and definition in reading order.");
        double Left(string text)
        {
            var range = document.FindText(text, false, false);
            Check(range != null, $"The definition-list range {text} is available.");
            range.GetBoundingRectangles(out var bounds);
            Check(bounds.Length >= 4, $"The definition-list range {text} has visible geometry.");
            return bounds[0];
        }
        var term = Left("術語 一");
        var first = Left("First definition");
        var second = Left("Second definition");
        Check(first > term + 8 && Math.Abs(first - second) < 1 && Math.Abs(term - Left("Another term")) < 1 &&
            Math.Abs(first - Left("Tilde definition")) < 1 && Math.Abs(term - Left("Ordinary paragraph.")) < 1,
            "Each definition shares the definition inset and later terms and paragraphs restore the root offset.");
        var canonical = editor.GetText();
        var offset = canonical.IndexOf("Second definition", StringComparison.Ordinal);
        var identity = preview.GetSelection();
        var mapped = await preview.MapSourcePositionAsync(identity.DocumentId, identity.Revision, identity.TextGeneration,
            (ulong)Encoding.UTF8.GetByteCount(canonical.AsSpan(0, offset)), MarkdownPositionAffinity.After);
        Check(mapped.PreviewUtf16Offset == (ulong)result.IndexOf("Second definition", StringComparison.Ordinal),
            "The second definition maps to the exact accepted UTF-16 position.");
        log.AppendLine("PASS: Markdown.DefinitionLists verifies multiple definitions, tilde syntax, native indentation, source coordinates and UIA text.");
    }

    private static async Task EmojiShortcodesAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "中:family_man_woman_girl_boy:Z\n\n" +
            "Aliases :+1: :thumbsup: :heart: :woman_technologist: :hand_with_index_finger_and_thumb_crossed: :does_not_exist:.\n\n" +
            "Literal `:smile:` and https://example.test/:smile:/docs.";
        var editor = await OpenAsync(host, "emoji-shortcodes.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var document = ((ITextProvider)peer.GetPattern(PatternInterface.Text)).DocumentRange;
        const string family = "👨‍👩‍👧‍👦";
        const string expected = "中" + family + "Z\nAliases 👍 👍 ❤️ 👩‍💻 🫰 :does_not_exist:.\n" +
            "Literal :smile: and https://example.test/:smile:/docs.";
        Check(document.GetText(-1) == expected && preview.DisplayedText == expected,
            "The product exposes exact emoji sequences, aliases and literal fallbacks in UI Automation.");
        var range = document.FindText(family, false, false);
        Check(range != null && range.GetText(-1) == family, "The full family sequence is selectable.");
        range.GetBoundingRectangles(out var bounds);
        Check(bounds.Length >= 4 && bounds[2] > 0 && bounds[3] > 0,
            "The shortcode has visible native glyph geometry.");
        var identity = preview.GetSelection();
        var before = await preview.MapSourcePositionAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, 10, MarkdownPositionAffinity.Before);
        var after = await preview.MapSourcePositionAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, 10, MarkdownPositionAffinity.After);
        Check(before.PreviewUtf16Offset == 1 && after.PreviewUtf16Offset == 12 &&
            before.Correspondence == MarkdownCorrespondence.Atomic &&
            after.Correspondence == MarkdownCorrespondence.Atomic,
            "Each affinity selects an edge of one atomic original shortcode.");
        Check(editor.GetText().StartsWith("中:family_man_woman_girl_boy:Z", StringComparison.Ordinal),
            "The source retains the authored alias.");
        log.AppendLine("PASS: Markdown.EmojiShortcodes verifies native geometry, aliases, literal fallbacks, UIA and atomic source mapping.");
    }

    private static async Task TableOfContentsAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var gap = string.Concat(Enumerable.Repeat("An intervening paragraph.\n\n", 45));
        var source = "[TOC]\n\n" + gap +
            "## Same title {#first}\n\nFirst destination.\n\n" +
            "#### Nested *label* :rocket:\n\n" + gap +
            "## Same title {#second}\n\nSecond destination.\n\n" + gap +
            "<details>\n<summary>Closed section</summary>\n\n" +
            "### Hidden *heading*\n\nDisclosure destination.\n\n</details>\n\n" +
            "Literal `[TOC]`.\n\n[[_TOC_]]\n\n" + gap;
        var editor = await OpenAsync(host, "table-of-contents.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var canonical = editor.GetText();
        const string entries = "Same title\nNested label 🚀\nSame title\nHidden heading";
        var displayed = preview.DisplayedText;
        Check(displayed.StartsWith(entries + "\n", StringComparison.Ordinal) &&
            displayed.Contains("Literal [TOC].\n" + entries, StringComparison.Ordinal),
            "Both standalone markers generate selectable labels and inline code preserves the literal marker.");
        var outline = preview.GetOutline();
        Check(outline.Count == 4, "The generated entries do not become outline headings.");
        var identity = preview.GetSelection();
        var mapped = await preview.MapPreviewSpanAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, 1, 4, MarkdownPositionAffinity.After);
        Check(mapped.Start.SourceByteOffset == 5 && mapped.End.SourceByteOffset == 5,
            "A generated label maps to its authored marker boundary.");

        var peer = FrameworkElementAutomationPeer.FromElement(preview) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        var doc = reading.DocumentRange;
        Check(doc.GetText(-1) == displayed, "UI Automation exposes every generated label as reading text.");
        var first = doc.FindText("Same title", false, false);
        var nested = doc.FindText("Nested label 🚀", false, false);
        first.GetBoundingRectangles(out var firstBounds);
        nested.GetBoundingRectangles(out var nestedBounds);
        Check(firstBounds.Length == 4 && nestedBounds.Length == 4 && nestedBounds[0] > firstBounds[0] + 4,
            "The nested entry has a visibly greater indent.");

        IInvokeProvider[] matches = [];
        IInvokeProvider hidden = null;
        await UntilAsync(() =>
        {
            var children = peer.GetChildren();
            if (children == null) return false;
            matches = children.Where(item => item.GetName() == "Same title")
                .Select(item => item.GetPattern(PatternInterface.Invoke)).OfType<IInvokeProvider>().ToArray();
            hidden = children.FirstOrDefault(item => item.GetName() == "Hidden heading")
                ?.GetPattern(PatternInterface.Invoke) as IInvokeProvider;
            return matches.Length >= 2 && hidden != null;
        }, "Find directory links with repeated labels and a closed destination.");
        async Task InvokeAndRevealAsync(IInvokeProvider action, string expected)
        {
            action.Invoke();
            await UntilAsync(() => preview.GetViewportAnchor().IsAvailable &&
                reading.GetVisibleRanges().Any(range => range.GetText(-1).Contains(expected, StringComparison.Ordinal)),
                $"Reveal {expected} after invoking a table of contents link.");
        }

        await InvokeAndRevealAsync(matches[1], "Second destination.");
        await InvokeAndRevealAsync(matches[0], "First destination.");
        await InvokeAndRevealAsync(hidden, "Disclosure destination.");
        Check(editor.GetText() == canonical, "Directory navigation preserves the canonical source.");
        log.AppendLine("PASS: Markdown.TableOfContents verifies aliases, indentation, selectable labels, source mapping and distinct UI Automation targets.");
    }

    private static async Task LocalWikiLinksAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var gap = string.Concat(Enumerable.Repeat("Paragraph before a wiki destination.\n\n", 45));
        var targetFile = await host.Folder.CreateFileAsync("target 25%20 会议 1.2.md");
        await FileIO.WriteTextAsync(targetFile, "# Start\n\n" + gap + "## 安装 *指南*\n\nCross-file destination.");
        const string assetName = "tile 25%20.png";
        var imageFile = await host.Folder.CreateFileAsync(assetName);
        await FileIO.WriteBytesAsync(imageFile, Convert.FromBase64String(PngBase64));
        var markdown = "[[#same ä|Jump local]] · [[target 25%20 会议 1.2#Start#安装 指南|Open wiki]]\n\n" +
            "![[tile 25%20.png|48x24]]\n\n![[tile 25%20.png|32]]\n\n" +
            "Literal `[[Note]]`.\n\n" + gap +
            "## Same *Ä*\n\nLocal heading destination.\n\n" + gap;
        var source = await OpenAsync(host, "wiki-source.md", Encoding.UTF8.GetBytes(markdown));
        source.ShowHideContentPreview();
        var preview = (await CurrentAsync(source)).PreviewView;
        var canonical = source.GetText();
        Check(preview.DisplayedText.Contains("Jump local · Open wiki", StringComparison.Ordinal) &&
            preview.DisplayedText.Contains("Literal [[Note]].", StringComparison.Ordinal),
            "The App displays wiki aliases and preserves inline code.");

        AutomationPeer GetPeer(MarkdownView view) => FrameworkElementAutomationPeer.FromElement(view) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(view);
        async Task InvokeAsync(string name)
        {
            var current = (await CurrentAsync(source)).PreviewView;
            IInvokeProvider action = null;
            await UntilAsync(() =>
            {
                action = GetPeer(current).GetChildren()?.FirstOrDefault(item => item.GetName() == name)
                    ?.GetPattern(PatternInterface.Invoke) as IInvokeProvider;
                return action != null;
            }, $"Find the accessible wiki link {name}.");
            action.Invoke();
        }
        bool Visible(MarkdownView view, string value) => view.GetViewportAnchor().IsAvailable &&
            ((ITextProvider)GetPeer(view).GetPattern(PatternInterface.Text)).GetVisibleRanges()
                .Any(range => range.GetText(-1).Contains(value, StringComparison.Ordinal));

        AutomationPeer[] images = [];
        await UntilAsync(() =>
        {
            images = GetPeer(preview).GetChildren()?.Where(item =>
                item.GetAutomationControlType() == AutomationControlType.Image &&
                item.GetName().StartsWith(assetName + ", image loaded", StringComparison.Ordinal)).ToArray() ?? [];
            return images.Length == 2;
        }, "Both wiki images resolve the literal percent filename and expose it as alt text.");
        var scale = preview.XamlRoot.RasterizationScale;
        var precise = images[0].GetBoundingRectangle();
        var proportional = images[1].GetBoundingRectangle();
        Check(Math.Abs(precise.Width / scale - 48) < 2 && Math.Abs(precise.Height / scale - 24) < 2 &&
            Math.Abs(proportional.Width / scale - 32) < 2 && Math.Abs(proportional.Height / scale - 32) < 2,
            $"Wiki images honor explicit dimensions and the square PNG aspect ratio. " +
            $"Observed {precise.Width / scale:F1} × {precise.Height / scale:F1} and " +
            $"{proportional.Width / scale:F1} × {proportional.Height / scale:F1} DIP.");

        await InvokeAsync("Jump local");
        await UntilAsync(() => Visible(preview, "Local heading destination."),
            "A wiki heading link resolves the accepted formatted title without slug conversion.");
        await InvokeAsync("Open wiki");
        await UntilAsync(() => host.Core.GetSelectedReadyEditor()?.EditingFile?.IsEqual(targetFile) == true,
            "An extension-omitted wiki link opens the .md file with a dotted stem, literal percent and Unicode.");
        var target = (TextEditor)host.Core.GetSelectedReadyEditor();
        var targetPreview = (await CurrentAsync(target)).PreviewView;
        await UntilAsync(() => Visible(targetPreview, "Cross-file destination."),
            "The target preview reveals its accepted heading.");
        Check(source.GetText() == canonical, "Wiki navigation and image decoding preserve the source text.");
        log.AppendLine("PASS: Markdown.LocalWikiLinks verifies aliases, typed heading navigation, relative files, literal percent paths and image dimensions.");
    }

    private static async Task LocalNoteEmbedsAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var nested = await host.Folder.CreateFolderAsync("Notes");
        var child = await nested.CreateFileAsync("Note 25% 1.2.md");
        var gap = string.Concat(Enumerable.Repeat("Paragraph above the hidden heading.\n\n", 25));
        await FileIO.WriteBytesAsync(child, Encoding.Unicode.GetPreamble().Concat(Encoding.Unicode.GetBytes(
            "# Before section\n\nOutside before.\n\n## Selected *heading*\n\n[Resolved child reference][target]. [Reveal hidden](#hidden-heading)\n\n" +
            "![[Nested]]\n\n### Child heading\n\nChild details.\n\n" + gap +
            "> [!NOTE]- Collapsed note\n> ### Hidden heading\n> Hidden child text.\n\n" +
            "## After section\n\nOutside after.\n\n[target]: #child-heading\n")).ToArray());
        await FileIO.WriteTextAsync(await nested.CreateFileAsync("Nested.md"), "# Nested note\n\nInside nested note.\n\n![[Note 25% 1.2]]");
        var source = await OpenAsync(host, "note-host.md",
            Encoding.UTF8.GetBytes("# Parent document\n\n![[./Notes/./Note 25% 1.2#Selected heading]]\n\n" +
            "![[#Local preview sample]]\n\n## Local preview sample\n\nLocal scope.\n\n## Different\n\nOutside local.\n"));
        source.ShowHideContentPreview();
        var preview = (await CurrentAsync(source)).PreviewView;
        var canonical = source.GetText();

        AutomationPeer GetPeer(MarkdownView view) => FrameworkElementAutomationPeer.FromElement(view) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(view);
        AutomationPeer FindPreview(AutomationPeer parent, string text) =>
            parent.GetChildren()?.FirstOrDefault(item => item.GetPattern(PatternInterface.Text) is ITextProvider provider
                && provider.DocumentRange.GetText(-1).Contains(text, StringComparison.Ordinal));
        var parent = GetPeer(preview);
        AutomationPeer section = null;
        try
        {
            await UntilAsync(() => (section = FindPreview(parent, "Resolved child reference")) != null,
                "The mounted note renders its selected section and accepts UTF-16 source files.");
        }
        catch (TimeoutException)
        {
            var slots = preview.GetVisibleNoteEmbeds();
            log.AppendLine($"OBS: Parent note slots={slots.Count}; {string.Join(", ", slots.Select(item => item.Target))}.");
            var overlay = preview.FindDescendant("ViewportOverlay") as Canvas;
            foreach (var failed in overlay?.Children.OfType<MarkdownView>() ?? Enumerable.Empty<MarkdownView>())
                log.AppendLine($"OBS: Child loaded={failed.IsLoaded}; size={failed.ActualWidth:F1}×{failed.ActualHeight:F1}; " +
                    $"status={failed.LastStatus}; ready={failed.IsSurfaceReady}; text={failed.DisplayedTextLength}; {failed.RenderingError}");
            throw;
        }
        var parentProvider = (ITextProvider)parent.GetPattern(PatternInterface.Text);
        var parentRange = parentProvider.DocumentRange;
        var parentText = parentRange.GetText(-1);
        var parentChildren = parentRange.GetChildren();
        Check(parentChildren.Length == 2 && parentChildren.All(item => parentProvider.RangeFromChild(item).GetText(-1) == "\uFFFC"),
            "Each accessible child resolves back to the authored parent object.");
        var sectionProvider = (ITextProvider)section.GetPattern(PatternInterface.Text);
        var sectionRange = sectionProvider.DocumentRange;
        var sectionText = sectionRange.GetText(-1);
        Check(parentText.Contains('\uFFFC') && !parentText.Contains("Resolved child reference", StringComparison.Ordinal) &&
            sectionText.StartsWith("Selected heading", StringComparison.Ordinal) &&
            sectionText.Contains("Child details.", StringComparison.Ordinal) &&
            !sectionText.Contains("Outside", StringComparison.Ordinal),
            "The parent owns one object projection and the child UIA text owns only the selected section.");
        Check(sectionRange.FindText("Outside before.", false, false) == null &&
            sectionRange.FindText("Child details.", false, false) != null,
            "UIA find cannot escape the selected section.");
        Check(section.GetChildren()?.Any(item => item.GetName() == "Resolved child reference" &&
            item.GetPattern(PatternInterface.Invoke) is IInvokeProvider) == true,
            "The reference defined after the section resolves to a child link.");
        AutomationPeer nestedNote = null;
        await UntilAsync(() => (nestedNote = FindPreview(section, "Inside nested note")) != null,
            "The nested note resolves relative to its embedded owner.");
        Check(FindPreview(nestedNote, "Resolved child reference") == null,
            "A resolved ancestor cycle does not mount another document.");
        await UntilAsync(() => FindPreview(parent, "Local scope.") is { } local &&
            !((ITextProvider)local.GetPattern(PatternInterface.Text)).DocumentRange.GetText(-1)
                .Contains("Outside local.", StringComparison.Ordinal),
            "A same-file heading embed is sourced from the accepted editor document.");
        var sectionView = ((Canvas)preview.FindDescendant("ViewportOverlay")).Children.OfType<MarkdownView>()
            .Single(view => view.DisplayedText.Contains("Resolved child reference", StringComparison.Ordinal));
        MarkdownView localView = null;
        await UntilAsync(() =>
        {
            localView = ((Canvas)preview.FindDescendant("ViewportOverlay")).Children.OfType<MarkdownView>()
                .FirstOrDefault(view => view.DisplayedText.StartsWith("Local preview sample", StringComparison.Ordinal));
            return localView?.GetViewportAnchor().IsAvailable == true && localView.ActualHeight is > 0 and < 224 &&
                localView.FindDescendant("PreviewScrollViewer") is ScrollViewer { ScrollableHeight: <= 1 } &&
                Math.Abs(sectionView.ActualHeight - 224) < 1;
        }, "A short section fits its content while the long embedded note retains its capped viewport.");
        MarkdownView nestedView = null;
        await UntilAsync(() =>
        {
            nestedView = ((Canvas)sectionView.FindDescendant("ViewportOverlay")).Children.OfType<MarkdownView>()
                .FirstOrDefault(view => view.DisplayedText.Contains("Inside nested note", StringComparison.Ordinal));
            return nestedView?.GetViewportAnchor().IsAvailable == true &&
                nestedView.GetVisibleNoteEmbeds().Any(slot => slot.Bounds.Height is > 0 and <= 64);
        }, "An unresolved ancestor cycle occupies a compact caption inside the nested note.");
        IInvokeProvider HiddenLink() => section.GetChildren()?.FirstOrDefault(item => item.GetName() == "Reveal hidden")
            ?.GetPattern(PatternInterface.Invoke) as IInvokeProvider;
        await UntilAsync(() => sectionView.GetViewportAnchor().IsAvailable && HiddenLink() != null,
            "The child fragment leading into a collapsed disclosure is accessible.");
        var layout = sectionView.GetViewportAnchor().LayoutGeneration;
        HiddenLink().Invoke();
        Check(sectionView.TrySetVerticalScrollProgress(0), "A newer explicit scroll claims the child viewport.");
        await UntilAsync(() => sectionView.GetViewportAnchor() is { IsAvailable: true } anchor &&
            anchor.LayoutGeneration > layout, "The disclosure relayout publishes after the newer scroll.");
        Check(sectionView.VerticalScrollProgress == 0 && HiddenLink() != null,
            "The stale fragment retry leaves the child at the newer scroll position.");
        HiddenLink().Invoke();
        await UntilAsync(() => sectionView.VerticalScrollProgress > 0,
            "A new link request can still reveal the expanded destination.");

        var native = source.TestNativeCore.TestFeatureNative;
        var oldNoteSlot = preview.GetVisibleNoteEmbeds().Single(slot => slot.Target.StartsWith("./Notes/", StringComparison.Ordinal));
        const double childPosition = 0.35;
        Check(sectionView.TrySetVerticalScrollProgress(childPosition), "The child accepts an explicit non-animated scroll.");
        await UntilAsync(() => sectionView.GetViewportAnchor().IsAvailable &&
            Math.Abs(sectionView.VerticalScrollProgress - childPosition) < 0.002,
            "The child presents the requested viewport before the parent edit.");
        var childGeneration = sectionView.GetSelection().TextGeneration;
        bool RetainedViewport() => sectionView.IsSurfaceReady &&
            sectionView.GetSelection() is { IsAvailable: true } selection && selection.TextGeneration == childGeneration &&
            Math.Abs(sectionView.VerticalScrollProgress - childPosition) < 0.002;
        var overlayChildren = ((Canvas)preview.FindDescendant("ViewportOverlay")).Children;
        var binding = preview.GetSelection();
        Check(!preview.SetNoteEmbedChild(binding.DocumentId, binding.Revision, 0, 0, null) &&
            overlayChildren.Contains(sectionView), "An unset detach alias cannot remove a mounted note.");
        native.SetEmptySelection(0);
        source.TypeText("Inserted prose\r\r![Earlier object](missing-note-host-image.png)\r\r");
        await CurrentAsync(source);
        await UntilAsync(() => preview.GetVisibleNoteEmbeds().Any(slot => slot.Target == oldNoteSlot.Target &&
            slot.ObjectId != oldNoteSlot.ObjectId) && overlayChildren.Contains(sectionView) && sectionView.IsSurfaceReady,
            "An earlier insertion shifts the object ID without remounting the loaded external section.");
        Check(RetainedViewport(),
            "Unchanged external content preserves its reading generation and independently scrolled viewport.");
        var imageGeneration = source.TestMarkdownPreview.TestImageProviderGeneration;
        await source.SaveContentToFileAndUpdateEditorStateAsync(source.EditingFile);
        await CurrentAsync(source);
        Check(source.TestMarkdownPreview.TestImageProviderGeneration == imageGeneration &&
            overlayChildren.Contains(sectionView) && RetainedViewport(),
            "A same-path save keeps the current image provider and the existing note with its own scroll position.");
        native.Undo();
        await CurrentAsync(source);
        await UntilAsync(() => preview.GetVisibleNoteEmbeds().Any(slot => slot.Target == oldNoteSlot.Target &&
            slot.ObjectId == oldNoteSlot.ObjectId) && overlayChildren.Contains(sectionView) && RetainedViewport(),
            "Undoing the insertion preserves the same external note instance, reading generation and viewport.");
        const string heading = "Selected heading";
        var headingStart = checked((long)Encoding.UTF8.GetByteCount(canonical.AsSpan(0,
            canonical.IndexOf(heading, StringComparison.Ordinal))));
        native.SetSel(headingStart, headingStart + heading.Length);
        source.TypeText("After section");
        await CurrentAsync(source);
        await UntilAsync(() => !overlayChildren.Contains(sectionView) && overlayChildren.OfType<MarkdownView>()
            .Any(view => view.DisplayedText.Contains("Outside after.", StringComparison.Ordinal)),
            "Editing the note destination heading replaces the old child with the newly addressed section.");
        native.Undo();
        await CurrentAsync(source);
        log.AppendLine("PASS: Markdown.NoteEditRetention preserves the external mount, generation and scroll position across a shifted object ID and Undo, and replaces it after a target change.");

        var other = await OpenAsync(host, "note-host-other.txt", Encoding.UTF8.GetBytes("Another tab"));
        await UntilAsync(() => ReferenceEquals(host.Core.GetSelectedReadyEditor(), other) &&
            !((Canvas)preview.FindDescendant("ViewportOverlay")).Children.OfType<MarkdownView>().Any(),
            "Hiding the cached parent retires its mounted notes.");
        await preview.Dispatcher.RunAsync(Windows.UI.Core.CoreDispatcherPriority.Low, () => { });
        host.Core.SwitchTo(source);
        await CurrentAsync(source);
        await UntilAsync(() => FindPreview(parent, "Resolved child reference") != null,
            "The cached parent remounts its visible note without scrolling or editing.");
        Check(source.GetText() == canonical, "Mounting a note does not edit the parent source.");
        log.AppendLine("PASS: Markdown.LocalNoteEmbeds covers encoding, scoped references, fitted/capped heights, compact cycles, scroll supersession, and cached-tab return.");

        var draft = (TextEditor)await host.Core.CreateNewTextEditorAsync("draft-note");
        host.Core.OpenTextEditor(draft);
        draft.SetLanguageOverride("markdown");
        draft.TestNativeCore.SetText("# Start\r\r![[#Excerpt]]\r\r## Excerpt\r\r![[#Nested excerpt]]\r\r## Nested excerpt\r\rNested draft text.");
        draft.ShowHideContentPreview();
        var extension = draft.TestMarkdownPreview;
        var draftPreview = extension.PreviewView;
        draftPreview.Width = 60;
        await CurrentAsync(draft);
        var draftOverlay = (Canvas)draftPreview.FindDescendant("ViewportOverlay");
        await UntilAsync(() => extension.TestHasRejectedRootNoteMount("#Excerpt"),
            "The draft note cannot mount within the initial slot width.");

        var initialDraft = draftPreview.GetSelection();
        draftOverlay.Margin = new Thickness(12, 8, 0, 0);
        draftPreview.Width = 420;
        MarkdownView excerpt = null;
        await UntilAsync(() =>
        {
            excerpt = draftOverlay.Children.OfType<MarkdownView>()
                .FirstOrDefault(view => view.DisplayedText.StartsWith("Excerpt", StringComparison.Ordinal));
            return excerpt?.FindDescendant("ViewportOverlay") is Canvas childOverlay &&
                childOverlay.Children.OfType<MarkdownView>().Any(view =>
                    view.DisplayedText.Contains("Nested draft text.", StringComparison.Ordinal));
        }, "Widening the same preview mounts the draft note and its nested section.");

        await UntilAsync(() =>
        {
            // The nested note can publish before its fitted height reaches the
            // root. Check slot geometry on the same current root layout.
            var identity = draftPreview.GetSelection();
            if (!identity.IsAvailable) return false;
            Check(draft.EditingFile == null && identity.TextGeneration == initialDraft.TextGeneration,
                "Widening the untitled document does not replace its accepted text to retry notes.");
            var slot = draftPreview.GetVisibleNoteEmbeds().SingleOrDefault(item => item.Target == "#Excerpt");
            if (slot.ObjectId == 0) return false;
            var origin = excerpt.TransformToVisual(draftPreview).TransformPoint(default);
            var overlayOrigin = draftOverlay.TransformToVisual(draftPreview).TransformPoint(default);
            return overlayOrigin.X >= 12 && overlayOrigin.Y >= 8 &&
                Math.Abs(origin.X - slot.Bounds.X) < 1 && Math.Abs(origin.Y - slot.Bounds.Y) < 1;
        }, "The mounted child aligns with its preview bounds when the overlay and control origins differ.");
        log.AppendLine("PASS: Markdown.DraftNoteEmbeds covers draft sections, nested mount retry and offset overlay alignment.");
    }

    private static async Task DraftNoteEditRetentionAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string heading = "Excerpt";
        const string excerpt = "Original passage.";
        const string editedExcerpt = "Updated passage 漢字.";
        const string originalDestination = "https://example.test/one";
        const string editedDestination = "https://example.test/two";
        var document = "前😀 Outside paragraph.\r\r![[#" + heading + "]]\r\r## " + heading +
            "\r\r" + excerpt + " [Read guide][guide]\r\r## Later\r\rOutside body.\r\r[guide]: " + originalDestination;
        var source = await OpenAsync(host, "draft-note-edit.md", Encoding.UTF8.GetBytes(document));
        source.ShowHideContentPreview();
        source.TestMarkdownPreview.FollowSource = false;
        var extension = await CurrentAsync(source);
        var overlay = (Canvas)extension.PreviewView.FindDescendant("ViewportOverlay");
        var native = source.TestNativeCore.TestFeatureNative;

        MarkdownView child = null;
        await UntilAsync(() =>
        {
            child = overlay.Children.OfType<MarkdownView>().SingleOrDefault();
            return child != null && extension.TestRootNoteLoadsCompleted &&
                child.DisplayedDocumentId != Guid.Empty && ExcerptMatches(excerpt, originalDestination);
        }, "The original same-file section and its reference definition are published.");
        var childDocument = child.DisplayedDocumentId;
        var childRevision = child.DisplayedRevision;
        var unloads = 0;
        void OnUnloaded(object sender, RoutedEventArgs args) => unloads++;
        child.Unloaded += OnUnloaded;
        try
        {
            native.SetEmptySelection(0);
            native.BeginUndoAction();
            try
            {
                source.TypeText("Inserted line.\r");
                foreach (var part in new[] { "Rapid ", "outside ", "typing.\r\r" })
                    source.TypeText(part);
            }
            finally { native.EndUndoAction(); }
            await VerifyAsync(excerpt, originalDestination, "Typing outside the excerpt retains the same live child.");
            native.Undo();
            await VerifyAsync(excerpt, originalDestination, "Undoing the insertion retains the same live child.");

            Replace(excerpt, editedExcerpt);
            await VerifyAsync(editedExcerpt, originalDestination, "An edit inside the excerpt updates the mounted child.");
            native.Undo();
            await VerifyAsync(excerpt, originalDestination, "Undo restores the original passage in the same child.");

            Replace(originalDestination, editedDestination);
            await VerifyAsync(excerpt, editedDestination, "Editing the external reference definition updates the mounted child's link.");
            native.Undo();
            await VerifyAsync(excerpt, originalDestination, "Undo restores the link destination in the same child.");
            Check(source.GetText() == document, "The draft editor returns to its original source after all three undo operations.");
        }
        finally { child.Unloaded -= OnUnloaded; }
        log.AppendLine("PASS: Markdown.DraftNoteEditRetention covers insertion, rapid typing, section edits, global references and Undo without remounting.");

        bool ExcerptMatches(string expectedText, string destination)
        {
            var state = child.GetSourceState();
            if (!state.StateKnown || !state.HasSource || state.NeedsResync ||
                state.DocumentId != child.DisplayedDocumentId || state.Revision != child.DisplayedRevision ||
                !HasReadingIdentity(child, state.DocumentId, state.Revision) ||
                !child.DisplayedText.Contains(expectedText, StringComparison.Ordinal) ||
                child.DisplayedText.Contains("Outside body.", StringComparison.Ordinal))
                return false;
            var peer = FrameworkElementAutomationPeer.FromElement(child) ??
                FrameworkElementAutomationPeer.CreatePeerForElement(child);
            return peer.GetChildren()?.SingleOrDefault(item => item.GetAutomationControlType() == AutomationControlType.Hyperlink &&
                item.GetName() == "Read guide")?.GetHelpText() == destination;
        }

        async Task VerifyAsync(string expectedText, string destination, string message)
        {
            await CurrentAsync(source);
            Check(overlay.Children.Contains(child) && unloads == 0, "The original draft note remains mounted at parent publication.");
            await UntilAsync(() => overlay.Children.Contains(child) && child.DisplayedDocumentId == childDocument &&
                child.DisplayedRevision > childRevision && ExcerptMatches(expectedText, destination), message);
            childRevision = child.DisplayedRevision;
            Check(unloads == 0, "The draft note remains loaded through the edit.");
        }

        void Replace(string oldText, string newText)
        {
            var text = source.GetText();
            var position = text.IndexOf(oldText, StringComparison.Ordinal);
            var start = (long)Encoding.UTF8.GetByteCount(text.AsSpan(0, position));
            native.SetSel(start, start + Encoding.UTF8.GetByteCount(oldText));
            native.BeginUndoAction();
            try { source.TypeText(newText); }
            finally { native.EndUndoAction(); }
        }
    }

    private static async Task FootnotesAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var gap = string.Concat(Enumerable.Repeat("An intervening paragraph.\n\n", 45));
        const string afterDefinition = "Heading after the definition";
        var source = "Top body.\n\n[^early]: Early definition body.\n\n## " + afterDefinition + "\n\n" +
            "First visible occurrence[^late].\n\n" + gap +
            "Second visible occurrence[^late], then the early note[^early].\n\n" + gap +
            "Inline occurrence^[Inline definition *body*, with `a]b` inside code.].\n\n" +
            "[^late]: Late definition **body**.\n\n    Another paragraph.\n\n[^unused]: Hidden unused note.\n";
        var editor = await OpenAsync(host, "footnotes.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var extension = editor.TestMarkdownPreview;
        extension.FollowSource = false;
        await CurrentAsync(editor);
        var preview = extension.PreviewView;
        var displayed = preview.DisplayedText;
        Check(displayed.Contains("First visible occurrence1.", StringComparison.Ordinal) &&
            displayed.Contains("Second visible occurrence1, then the early note2.", StringComparison.Ordinal) &&
            displayed.Contains("Late definition body.", StringComparison.Ordinal) &&
            displayed.IndexOf("Late definition body.", StringComparison.Ordinal) <
            displayed.IndexOf("Early definition body.", StringComparison.Ordinal) &&
            !displayed.Contains("Hidden unused note.", StringComparison.Ordinal),
            "Named notes use first-reference order, preserve inline formatting and hide the unused definition.");
        Check(displayed.Contains("Inline occurrence3.", StringComparison.Ordinal) &&
            displayed.Contains("Inline definition body, with a]b inside code.", StringComparison.Ordinal),
            "The inline note shares numbering and preserves its formatting and embedded bracket.");

        var canonical = editor.GetText();
        var first = canonical.IndexOf("First visible occurrence", StringComparison.Ordinal);
        var previewIdentity = preview.GetSelection();
        var headingStart = canonical.IndexOf("## " + afterDefinition, StringComparison.Ordinal);
        var headingByte = (ulong)Encoding.UTF8.GetByteCount(canonical.AsSpan(0, headingStart));
        var headingEndByte = headingByte + (ulong)Encoding.UTF8.GetByteCount("## " + afterDefinition);
        var headingSpan = await preview.MapSourceSpanAsync(previewIdentity.DocumentId, previewIdentity.Revision,
            previewIdentity.TextGeneration, headingByte, headingEndByte, MarkdownPositionAffinity.After);
        var headingOffset = (ulong)displayed.IndexOf(afterDefinition, StringComparison.Ordinal);
        var boundary = await preview.MapSourcePositionAsync(previewIdentity.DocumentId, previewIdentity.Revision,
            previewIdentity.TextGeneration, headingByte, MarkdownPositionAffinity.After);
        Check(boundary.PreviewUtf16Offset > headingOffset + (ulong)afterDefinition.Length &&
            boundary.Correspondence == MarkdownCorrespondence.Synthetic,
            "The standalone After boundary includes the relocated footnote tail.");
        Check(!headingSpan.IsDiscontinuous && headingSpan.Start.PreviewUtf16Offset == headingOffset &&
            headingSpan.Start.Correspondence == MarkdownCorrespondence.Exact && headingSpan.Start.Effects == 0 &&
            headingSpan.End.PreviewUtf16Offset == headingOffset + (ulong)afterDefinition.Length,
            "A viewport range on the following heading ignores the relocated footnote tail at its start boundary.");
        var crossing = await preview.MapSourceSpanAsync(previewIdentity.DocumentId, previewIdentity.Revision,
            previewIdentity.TextGeneration, 0, (ulong)Encoding.UTF8.GetByteCount(canonical.AsSpan(0, first + 5)),
            MarkdownPositionAffinity.After);
        Check(crossing.IsDiscontinuous && crossing.Start.PreviewUtf16Offset < crossing.End.PreviewUtf16Offset,
            "A source span across a relocated definition reports discontinuity even with ordered endpoints.");

        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        async Task InvokeAndRevealAsync(string actionName, string content)
        {
            IInvokeProvider action = null;
            await UntilAsync(() =>
            {
                action = peer.GetChildren()?.FirstOrDefault(link => link.GetName() == actionName)
                    ?.GetPattern(PatternInterface.Invoke) as IInvokeProvider;
                return action != null;
            }, $"Find accessible link {actionName}.");
            action.Invoke();
            await UntilAsync(() => preview.GetViewportAnchor().IsAvailable && reading.GetVisibleRanges().Any(range =>
                range.GetText(-1).Contains(content, StringComparison.Ordinal)), $"Reveal {content} after invoking {actionName}.");
        }

        await InvokeAndRevealAsync("Footnote 1", "Late definition body.");
        await InvokeAndRevealAsync("Back to reference 2 of footnote 1", "Second visible occurrence");
        await InvokeAndRevealAsync("Footnote 1", "Late definition body.");
        await InvokeAndRevealAsync("Back to reference 1 of footnote 1", "First visible occurrence");
        await InvokeAndRevealAsync("Footnote 3", "Inline definition body");
        await InvokeAndRevealAsync("Back to reference 1 of footnote 3", "Inline occurrence");
        const string earlyBody = "Early definition body.";
        var searchEnd = canonical.IndexOf(earlyBody, StringComparison.Ordinal) + earlyBody.Length;
        editor.TestNativeCore.TestFeatureNative.SetSel(0, 0);
        Check(await editor.TestSourceFindAsync(new SearchContext(@"(?s)Top body\..*?Early definition body\.", useRegex: true)),
            "Source Find matches a range that includes a relocated definition.");
        await AssertSourceFindAsync(editor, extension, 0, Encoding.UTF8.GetByteCount(canonical.AsSpan(0, searchEnd)), 0, 0);
        var earlyPreview = displayed.IndexOf(earlyBody, StringComparison.Ordinal);
        Check(await editor.TestSourceFindAsync(new SearchContext(earlyBody)),
            "Source Find matches contiguous content within a relocated definition.");
        await AssertSourceFindAsync(editor, extension, searchEnd - earlyBody.Length, searchEnd, (ulong)earlyPreview,
            (ulong)earlyBody.Length);
        Check(editor.GetText() == canonical, "Footnote navigation preserves the canonical source.");
        log.AppendLine("PASS: Markdown.Footnotes verifies shared numbering, exact source mapping, and named and inline accessible backlinks.");
    }

}
