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
    private static async Task HtmlTagFilterAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "<div>中😀 <ScRiPt>**原样** <b>Wide letters</b></sCrIpT> " +
            "<iframe src='https://example.invalid/'>普通</iframe></div>\r\r" +
            "**Wide letters**\r\r行内 <textarea><b>行内粗体</b><!-- 隐藏 --></textarea>\r\r" +
            "~~~html\r<style><b>sample</b></style>\r~~~";
        var editor = await OpenAsync(host, "html-tagfilter.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var document = ((ITextProvider)peer.GetPattern(PatternInterface.Text)).DocumentRange;
        Check(preview.DisplayedText.Contains(
            "中😀 <ScRiPt>**原样** Wide letters</sCrIpT> <iframe src='https://example.invalid/'>普通</iframe>",
            StringComparison.Ordinal) && preview.DisplayedText.Contains("行内 <textarea>行内粗体</textarea>", StringComparison.Ordinal) &&
            preview.DisplayedText.Contains("<style><b>sample</b></style>", StringComparison.Ordinal),
            "GFM tagfilter keeps blocked HTML visible while safe formatting remains active and fenced code remains literal.");
        await UntilAsync(() =>
        {
            var native = document.FindText("Wide letters", false, false);
            var reference = document.FindText("Wide letters", true, false);
            if (native == null || reference == null)
                return false;
            native.GetBoundingRectangles(out var first);
            reference.GetBoundingRectangles(out var second);
            return first.Length == 4 && second.Length == 4 && second[1] > first[1] &&
                Math.Abs(first[2] - second[2]) < 0.1;
        }, "Safe bold inside filtered raw HTML has the same glyph advances as Markdown bold.");
        Check(editor.GetText() == source &&
            ReadCurrentBytes(editor.TestNativeCore).SequenceEqual(Encoding.UTF8.GetBytes(source)),
            "Filtering preserves the exact original editor source.");
        log.AppendLine("PASS: Markdown.HtmlTagFilter — literal blocked inline/block tags, native bold geometry and exact source.");
    }

    private static async Task HtmlInlineBreaksAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string markdown = "中😀<BR />尾\r\r[Link before<br>Link after](#target)\r\r" +
            "| Column |\r| --- |\r| Cell before<br>Cell after |\r\r" +
            "Literal `<br>` and A<br onclick='noop()'>B\r\r## target";
        var editor = await OpenAsync(host, "html-inline-breaks.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var text = (ITextProvider)peer.GetPattern(PatternInterface.Text);

        await UntilAsync(() =>
        {
            var before = text.DocumentRange.FindText("中😀", false, false);
            var after = text.DocumentRange.FindText("尾", false, false);
            if (before == null || after == null)
                return false;
            before.GetBoundingRectangles(out var first);
            after.GetBoundingRectangles(out var second);
            return first.Length == 4 && second.Length == 4 && second[1] > first[1];
        }, "The admitted inline break creates separate native line rectangles.");

        var originalPreview = preview.DisplayedText;
        Check(originalPreview.Contains("中😀\n尾", StringComparison.Ordinal) &&
            originalPreview.Contains("Link before\nLink after", StringComparison.Ordinal) &&
            originalPreview.Contains("Cell before\nCell after", StringComparison.Ordinal) &&
            originalPreview.Contains("Literal <br> and A<br onclick='noop()'>B", StringComparison.Ordinal),
            "Inline HTML breaks render inside paragraphs, links and table cells, with literal code and attributes preserved.");
        var link = peer.GetChildren().Single(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink);
        string destination = null;
        preview.LinkRequested += (_, args) => destination = args.Destination;
        ((IInvokeProvider)link.GetPattern(PatternInterface.Invoke)).Invoke();
        Check(destination == "#target", "Both lines of the link retain one action and its original destination.");

        var session = extension.SourceSession;
        var fullBefore = session.FullSubmissions;
        var deltaBefore = session.DeltaSubmissions;
        var core = editor.TestNativeCore;
        var start = Encoding.UTF8.GetByteCount("中😀<");
        core.TestFeatureNative.SetSel(start, start + 2);
        editor.TypeText("zz");
        await CurrentAsync(editor);
        Check(preview.DisplayedText.Contains("中😀<zz />尾", StringComparison.Ordinal) &&
            session.DeltaSubmissions > deltaBefore && session.FullSubmissions == fullBefore,
            "Editing the tag through native delta restores literal text without a full resubmission.");
        await CompareFullSyncAsync(host, editor, "inline HTML break edited into a literal tag", log);

        core.Undo();
        await CurrentAsync(editor);
        Check(editor.GetText() == markdown && preview.DisplayedText == originalPreview,
            "Native undo restores the exact tag source and all of its preview line breaks.");
        await CompareFullSyncAsync(host, editor, "inline HTML break restored by native Undo", log);
        log.AppendLine("PASS: Markdown.HtmlInlineBreaks verifies native line geometry, safe fallbacks, one link action and incremental Undo.");
    }

    private static async Task HtmlInlineEmphasisAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string markdown = "Native <b>Wide letters</b>\r\rMarkdown **Wide letters**\r\r" +
            "中😀 <strong>[Link <em>尾</em> <s>old</s>](#target)</strong>\r\r" +
            "<b>Opening at a new paragraph</b>\r\r" +
            "| Column |\r| --- |\r| <i>cell</i> B |\r\r" +
            "Literal <b class='x'>open</b> and <b><i>cross</b></i>\r\r## target";
        var editor = await OpenAsync(host, "html-inline-emphasis.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var text = (ITextProvider)peer.GetPattern(PatternInterface.Text);

        await UntilAsync(() =>
        {
            var native = text.DocumentRange.FindText("Wide letters", false, false);
            var reference = text.DocumentRange.FindText("Wide letters", true, false);
            if (native == null || reference == null)
                return false;
            native.GetBoundingRectangles(out var first);
            reference.GetBoundingRectangles(out var second);
            return first.Length == 4 && second.Length == 4 && second[1] > first[1] &&
                Math.Abs(first[2] - second[2]) < 0.1;
        }, "Native HTML bold and Markdown bold have matching visible glyph advances.");

        var originalPreview = preview.DisplayedText;
        Check(originalPreview.Contains("Native Wide letters\nMarkdown Wide letters", StringComparison.Ordinal) &&
            originalPreview.Contains("中😀 Link 尾 old\nOpening at a new paragraph", StringComparison.Ordinal) &&
            originalPreview.Contains("Column\ncell B", StringComparison.Ordinal) &&
            originalPreview.Contains("Literal <b class='x'>open</b> and <b><i>cross</b></i>", StringComparison.Ordinal),
            "Paired tags compose with links and table cells while malformed and attributed tags remain literal.");
        var link = peer.GetChildren().Single(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink);
        string destination = null;
        preview.LinkRequested += (_, args) => destination = args.Destination;
        ((IInvokeProvider)link.GetPattern(PatternInterface.Invoke)).Invoke();
        Check(destination == "#target", "The link keeps one action through bold, italic and strike runs.");

        var session = extension.SourceSession;
        var fullBefore = session.FullSubmissions;
        var deltaBefore = session.DeltaSubmissions;
        var core = editor.TestNativeCore;
        var close = Encoding.UTF8.GetByteCount("Native <b>Wide letters</");
        core.TestFeatureNative.SetSel(close, close + 1);
        editor.TypeText("i");
        await CurrentAsync(editor);
        Check(preview.DisplayedText.Contains("Native <b>Wide letters</i>", StringComparison.Ordinal) &&
            session.DeltaSubmissions > deltaBefore && session.FullSubmissions == fullBefore,
            "A mismatched close restores both literal tags on native delta.");
        await CompareFullSyncAsync(host, editor, "inline HTML emphasis made unbalanced", log);

        core.Undo();
        await CurrentAsync(editor);
        Check(editor.GetText() == markdown && preview.DisplayedText == originalPreview,
            "Undo restores the original source and emphasis projection.");
        await CompareFullSyncAsync(host, editor, "inline HTML emphasis restored by native Undo", log);
        log.AppendLine("PASS: Markdown.HtmlInlineEmphasis verifies native glyph advances, safe fallbacks, one link action and incremental Undo.");
    }

    private static async Task HtmlInlineCodeAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string markdown = "Native <code>wide letters</code>\r\rMarkdown `wide letters`\r\r" +
            "<kbd>Ctrl+P</kbd> and <samp>output</samp>\r\r" +
            "<code>中😀 **raw** ![x](img) $x$ &amp;</code>\r\r" +
            "[Outer <code>[label](#inside)</code> link](#target)\r\r## target";
        var editor = await OpenAsync(host, "html-inline-code.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var text = (ITextProvider)peer.GetPattern(PatternInterface.Text);

        await UntilAsync(() =>
        {
            var native = text.DocumentRange.FindText("wide letters", false, false);
            var reference = text.DocumentRange.FindText("wide letters", true, false);
            if (native == null || reference == null)
                return false;
            native.GetBoundingRectangles(out var first);
            reference.GetBoundingRectangles(out var second);
            return first.Length == 4 && second.Length == 4 && second[1] > first[1] &&
                Math.Abs(first[2] - second[2]) < 0.1;
        }, "HTML code and Markdown code have matching visible glyph advances.");

        var originalPreview = preview.DisplayedText;
        Check(originalPreview.Contains("Native wide letters\nMarkdown wide letters", StringComparison.Ordinal) &&
            originalPreview.Contains("Ctrl+P and output", StringComparison.Ordinal) &&
            originalPreview.Contains("中😀 **raw** ![x](img) $x$ &amp;", StringComparison.Ordinal) &&
            originalPreview.Contains("Outer [label](#inside) link", StringComparison.Ordinal),
            "Code-like tags render literal contents and compose with the enclosing Markdown link.");
        var link = peer.GetChildren().Single(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink);
        string destination = null;
        preview.LinkRequested += (_, args) => destination = args.Destination;
        ((IInvokeProvider)link.GetPattern(PatternInterface.Invoke)).Invoke();
        Check(destination == "#target", "Literal inner link syntax exposes no separate action.");

        var session = extension.SourceSession;
        var fullBefore = session.FullSubmissions;
        var deltaBefore = session.DeltaSubmissions;
        var core = editor.TestNativeCore;
        var close = Encoding.UTF8.GetByteCount("Native <code>wide letters</");
        core.TestFeatureNative.SetSel(close, close + 4);
        editor.TypeText("samp");
        await CurrentAsync(editor);
        Check(preview.DisplayedText.Contains("Native <code>wide letters</samp>", StringComparison.Ordinal) &&
            session.DeltaSubmissions > deltaBefore && session.FullSubmissions == fullBefore,
            "A mismatched closing tag restores literal tags through a native delta.");
        await CompareFullSyncAsync(host, editor, "inline HTML code made unbalanced", log);

        core.Undo();
        await CurrentAsync(editor);
        Check(editor.GetText() == markdown && preview.DisplayedText == originalPreview,
            "Undo restores the original source and opaque code projection.");
        await CompareFullSyncAsync(host, editor, "inline HTML code restored by native Undo", log);
        log.AppendLine("PASS: Markdown.HtmlInlineCode verifies native glyph advances, literal body, enclosing link and incremental Undo.");
    }

    private static async Task HtmlBlocksAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var png = await host.Folder.CreateFileAsync("logo.png", CreationCollisionOption.FailIfExists);
        await FileIO.WriteBytesAsync(png, Convert.FromBase64String(PngBase64));
        const string markdown = "<p align='center'><a href='#native-heading'>" +
            "<img width='48' height='24' src='logo.png' alt='Test logo'></a></p>\r" +
            "<h1 align='center'>Native heading</h1>\r" +
            "<p>Text &amp; more <a href='javascript:alert(1)'>unsafe</a></p>\r\r" +
            "## Following Markdown";
        var editor = await OpenAsync(host, "html-blocks.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        await UntilAsync(() => ImageState(preview, "Test logo", true),
            "The HTML image resolves relative to the actual Markdown file.", 25000);
        Check(editor.GetText() == markdown && preview.DisplayedText.Contains("Text & more unsafe", StringComparison.Ordinal) &&
            preview.DisplayedText.Contains("Following Markdown", StringComparison.Ordinal) &&
            !preview.DisplayedText.Contains("<img", StringComparison.Ordinal) && preview.GetOutline().Count == 2,
            "The source stays literal while HTML text, entities and headings enter the normal preview and outline.");

        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var children = peer.GetChildren();
        var image = children.Single(child => child.GetAutomationControlType() == AutomationControlType.Image);
        var link = children.Single(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink);
        var bounds = image.GetBoundingRectangle();
        var scale = preview.XamlRoot.RasterizationScale;
        var provider = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        provider.DocumentRange.FindText("Native heading", false, false).GetBoundingRectangles(out var heading);
        Check(Math.Abs(bounds.Width / scale - 48) < 2 && Math.Abs(bounds.Height / scale - 24) < 2 &&
            heading.Length == 4 && Math.Abs(bounds.X + bounds.Width / 2 - (heading[0] + heading[2] / 2)) < 3 * scale,
            "The real HTML image honors width and height and shares the centered heading's midpoint.");

        string target = null;
        preview.LinkRequested += (_, args) => target = args.Destination;
        ((IInvokeProvider)link.GetPattern(PatternInterface.Invoke)).Invoke();
        Check(target == "#native-heading", "The linked image uses native link activation and rejects the unsafe link.");

        var native = editor.TestNativeCore.TestFeatureNative;
        var start = Encoding.UTF8.GetByteCount(markdown[..markdown.IndexOf("Native heading", StringComparison.Ordinal)]);
        var delta = extension.SourceSession.DeltaSubmissions;
        native.SetSel(start, start + "Native".Length);
        editor.TypeText("Updated");
        await CurrentAsync(editor);
        Check(preview.DisplayedText.Contains("Updated heading", StringComparison.Ordinal) &&
            extension.SourceSession.DeltaSubmissions > delta,
            "Editing the original HTML updates the same preview through a source delta.");
        await CompareFullSyncAsync(host, editor, "HTML block edit", log);
        log.AppendLine("PASS: Markdown.HtmlBlocks renders and edits the README-style header with native image sizing, alignment, links and outline.");
    }

    private static async Task EmptyHtmlSeparatorsAsync(StringBuilder log)
    {
        var originalBounds = Window.Current.CoreWindow.Bounds;
        try
        {
            foreach (var separator in new[]
            {
                "<!-- divider -->", "<p></p>", "<p align='center'> </p>",
                "<!-- one -->\r\r<p></p>\r\r<!-- two -->"
            })
            {
                await using var host = await FeatureTestHost.CreateAsync();
                const string prefix = "[0-0-0] [0-0-1]\r\r1. First Hé😀\r2. First tail\r\r";
                var source = prefix + separator + "\r\r1. Second 中é\r2. Second tail\r\r" +
                    separator + "\r\r    first_code()\r\r" + separator + "\r\r    second_code()";
                const string expected = "[0-0-0] [0-0-1]\nFirst Hé😀\nFirst tail\nSecond 中é\nSecond tail\n" +
                    "first_code()\n\nsecond_code()\n";
                var editor = await OpenAsync(host, "empty-html-separators.md", Encoding.UTF8.GetBytes(source));
                var core = editor.TestNativeCore;
                var originalZoom = core.GetFontZoomFactor();
                try
                {
                    Check(ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(1100, 760)),
                        "The empty-HTML fixture admits a visible window for list and code geometry.");
                    core.SetFontZoomFactor(100);
                    editor.ShowHideContentPreview();
                    var extension = await CurrentAsync(editor);
                    extension.FollowSource = false;
                    var preview = extension.PreviewView;
                    var session = extension.SourceSession;
                    var native = core.TestFeatureNative;

                    async Task AssertPreviewAsync(string literal, string reading, string first, string second)
                    {
                        await CurrentAsync(editor);
                        await TabDragTestHost.FrameAsync();
                        var peer = FrameworkElementAutomationPeer.FromElement(preview) ??
                            FrameworkElementAutomationPeer.CreatePeerForElement(preview);
                        var provider = (ITextProvider)peer.GetPattern(PatternInterface.Text);
                        var document = provider.DocumentRange;
                        Check(editor.GetText() == literal && ReadCurrentBytes(core).SequenceEqual(Encoding.UTF8.GetBytes(literal)),
                            "Invisible HTML separators preserve the literal native source and its complete UTF-8 bytes.");
                        Check(preview.DisplayedText == reading && document.GetText(-1) == reading,
                            "Native reading and UIA text export contain only authored visible text, including exact code newlines.");
                        Check(document.FindText("<!--", false, false) == null && document.FindText("<p", false, false) == null &&
                            document.FindText("-->", false, false) == null,
                            "Neither supported HTML markers nor their comment text enter the UIA DocumentRange.");
                        Check(peer.GetChildren().All(child => child.GetAutomationControlType() != AutomationControlType.Hyperlink),
                            "Private separators cannot resolve the authored unresolved [0-0-0] and [0-0-1] references.");

                        double[] Bounds(string text)
                        {
                            var range = document.FindText(text, false, false);
                            Check(range != null && range.GetText(-1) == text, "The separator fixture exposes the exact text probe: " + text);
                            range.GetBoundingRectangles(out var rectangles);
                            Check(rectangles.Length == 4 && rectangles[2] > 0 && rectangles[3] > 0,
                                "The separator probe has one visible native line rectangle: " + text);
                            return rectangles;
                        }
                        var firstHead = Bounds(first);
                        var firstTail = Bounds("First tail");
                        var secondHead = Bounds(second);
                        var secondTail = Bounds("Second tail");
                        var scale = preview.XamlRoot.RasterizationScale;
                        var advance = core.FontSize * 1.55;
                        Check(Math.Abs((firstTail[1] - firstHead[1]) / scale - advance) < 1 &&
                            Math.Abs((secondTail[1] - secondHead[1]) / scale - advance) < 1 &&
                            secondHead[1] - firstTail[1] > firstTail[1] - firstHead[1] + 2 * scale &&
                            Math.Abs(firstHead[0] - secondHead[0]) < scale,
                            "HTML separators retain two tight root lists and the visible block gap between them.");
                        var firstCode = Bounds("first_code()");
                        var secondCode = Bounds("second_code()");
                        Check(Math.Abs(firstCode[0] - secondCode[0]) < scale &&
                            secondCode[1] - firstCode[1] - firstCode[3] > 28 * scale,
                            "The indented code remains two visible separately padded native code blocks.");
                    }

                    await AssertPreviewAsync(source, expected, "First Hé😀", "Second 中é");
                    var full = session.FullSubmissions;
                    var delta = session.DeltaSubmissions;
                    native.BeginUndoAction();
                    try
                    {
                        var start = Encoding.UTF8.GetByteCount(source[..source.IndexOf("First Hé😀", StringComparison.Ordinal)]);
                        native.SetSel(start, start + "First".Length);
                        editor.TypeText("Edited");
                        source = source.Replace("First Hé😀", "Edited Hé😀", StringComparison.Ordinal);
                        start = Encoding.UTF8.GetByteCount(source[..source.IndexOf("Second 中é", StringComparison.Ordinal)]);
                        native.SetSel(start, start + "Second".Length);
                        editor.TypeText("Changed");
                        source = source.Replace("Second 中é", "Changed 中é", StringComparison.Ordinal);
                    }
                    finally { native.EndUndoAction(); }
                    var edited = expected.Replace("First Hé😀", "Edited Hé😀", StringComparison.Ordinal)
                        .Replace("Second 中é", "Changed 中é", StringComparison.Ordinal);
                    await AssertPreviewAsync(source, edited, "Edited Hé😀", "Changed 中é");
                    Check(session.DeltaSubmissions > delta && session.FullSubmissions == full,
                        "Actual native edits on both sides of the invisible HTML barrier publish through source deltas.");
                    await CompareFullSyncAsync(host, editor, "empty HTML separator edits", log);
                    session.RequestFullSync();
                    await UntilAsync(() => session.FullSubmissions > full,
                        "The separator fixture completes an explicit production full-source resubmission.");
                    await AssertPreviewAsync(source, edited, "Edited Hé😀", "Changed 中é");
                    log.AppendLine("OBS: Markdown.EmptyHtmlSeparators — " + separator.Replace("\r", "\\r", StringComparison.Ordinal));
                }
                finally { core.SetFontZoomFactor(originalZoom); }
            }
            log.AppendLine("PASS: Markdown.EmptyHtmlSeparators — comments, empty/whitespace paragraphs and consecutive barriers stay invisible; real list/code geometry, unresolved references, literal source, native delta edits and full sync remain stable.");
        }
        finally
        {
            ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(originalBounds.Width, originalBounds.Height));
        }
    }

}
