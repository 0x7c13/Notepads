// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Settings;
using Notepads.Markdown;
using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    internal static async Task RunYamlAsync(StringBuilder log)
    {
        var key = ApplicationPreferences.MarkdownFrontMatterStyleKey;
        var stored = ApplicationSettingsStore.Read(key);
        var original = ApplicationPreferences.MarkdownFrontMatterView;
        try
        {
            ApplicationSettingsStore.Remove(key);
            ApplicationPreferences.RefreshMarkdownFrontMatterStyle();
            Check(ApplicationPreferences.MarkdownFrontMatterView == MarkdownFrontMatterStyle.Table,
                "An absent YAML preference starts with Table.");
            foreach (object invalid in new object[] { -1, 3, int.MaxValue, "1", true })
            {
                ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.CodeBlock;
                Check(ApplicationSettingsStore.Read(key) is int saved && saved == 1,
                    "The YAML preference persists its integer contract.");
                ApplicationSettingsStore.Write(key, invalid);
                ApplicationPreferences.RefreshMarkdownFrontMatterStyle();
                Check(ApplicationPreferences.MarkdownFrontMatterView == MarkdownFrontMatterStyle.Table,
                    "An invalid persisted YAML mode falls back to Table: " + invalid);
            }
            ApplicationSettingsStore.Write(key, 2);
            ApplicationPreferences.Initialize();
            Check(ApplicationPreferences.MarkdownFrontMatterView == MarkdownFrontMatterStyle.Hide,
                "The actual application initializer reloads the saved YAML preference.");
            ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.Table;
            await using var host = await FeatureTestHost.CreateAsync();
            var source = MarkdownYamlFixture.Source() + "\r\r" +
                string.Join("\r\r", Enumerable.Range(0, 80).Select(index => "Scrollable body paragraph " + index)) +
                "\r\r## Far body heading\r\rfinal body marker";
            var first = await OpenAsync(host, "yaml-first.md", Encoding.UTF8.GetBytes(source));
            var firstTab = host.Core.GetSelectedTab();
            var menu = (TextEditorContextFlyout)first.GetContextFlyout();
            using var menuSession = new ScrollSyncMenuSession(menu);
            await menuSession.OpenAsync(first.TestNativeCore);
            Check(menu.Items.Contains(menu.PreviewToggle) && menu.PreviewToggle.IsLoaded &&
                menu.PreviewToggle.Visibility == Visibility.Visible && menu.PreviewToggle.IsEnabled &&
                menu.Items.All(item => AutomationProperties.GetAutomationId(item) != "MarkdownPreviewFrontMatter") &&
                !first.TestMarkdownPanelOpen,
                "The opened Markdown context menu retains Toggle Preview and has no YAML front matter submenu.");
            await menuSession.CloseAsync();
            first.TestNativeCore.IsEnabled = false;
            try
            {
                await menuSession.OpenAsync(first.TestNativeCore);
                Check(!menu.PreviewToggle.IsEnabled,
                    "Toggle Preview retains its existing read-only command permission.");
                await menuSession.CloseAsync();
            }
            finally { first.TestNativeCore.IsEnabled = true; }
            first.ShowHideContentPreview();
            var extension = await CurrentAsync(first);
            extension.FollowSource = false;
            var session = extension.SourceSession;
            var previewHeight = extension.PreviewView.ActualHeight;
            var beforeBytes = ReadCurrentBytes(first.TestNativeCore);
            var beforeUndo = first.TestNativeCore.CanUndo;
            var beforeModified = first.IsModified;
            await CheckYamlModeAsync(first, source, MarkdownFrontMatterStyle.Table);

            ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.CodeBlock;
            await CheckYamlModeAsync(first, source, MarkdownFrontMatterStyle.CodeBlock);
            Check(ApplicationSettingsStore.Read(key) is int code && code == 1,
                "The typed YAML preference saves the integer CodeBlock mode.");
            ApplicationSettingsStore.Write(key, 2);
            ApplicationPreferences.RefreshMarkdownFrontMatterStyle();
            await CheckYamlModeAsync(first, source, MarkdownFrontMatterStyle.Hide);
            var full = session.FullSubmissions;
            var delta = session.DeltaSubmissions;
            await menuSession.OpenAsync(first.TestNativeCore);
            Check(ApplicationPreferences.MarkdownFrontMatterView == MarkdownFrontMatterStyle.Hide &&
                session.FullSubmissions == full && session.DeltaSubmissions == delta &&
                menu.Items.All(item => AutomationProperties.GetAutomationId(item) != "MarkdownPreviewFrontMatter"),
                "Opening the context menu preserves the persisted YAML mode without resubmitting source.");
            await menuSession.CloseAsync();
            ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.Table;
            await CheckYamlModeAsync(first, source, MarkdownFrontMatterStyle.Table);
            Check(extension.PreviewView.ActualHeight == previewHeight &&
                ReadCurrentBytes(first.TestNativeCore).SequenceEqual(beforeBytes) &&
                first.TestNativeCore.CanUndo == beforeUndo && first.IsModified == beforeModified,
                "Switching all YAML views preserves reading-area height, original UTF-8, Undo and save point.");

            var sameModeFull = session.FullSubmissions;
            var sameModeDelta = session.DeltaSubmissions;
            ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.Table;
            Check(session.FullSubmissions == sameModeFull && session.DeltaSubmissions == sameModeDelta,
                "Assigning the current YAML mode does not resubmit source.");
            var native = first.TestNativeCore.TestFeatureNative;
            var titleStart = Encoding.UTF8.GetByteCount(source[..source.IndexOf("YAML fixture", StringComparison.Ordinal)]);
            native.SetSel(titleStart, titleStart + "YAML fixture".Length);
            first.CopyTextToWindowsClipboard(null);
            Check(await Clipboard.GetContent().GetTextAsync() == "YAML fixture",
                "The actual source Copy command exports unchanged metadata through the isolated clipboard boundary.");

            var second = await OpenAsync(host, "yaml-second.md", Encoding.UTF8.GetBytes(source));
            second.ShowHideContentPreview();
            var secondExtension = await CurrentAsync(second);
            ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.CodeBlock;
            await UntilAsync(() => session.IsCurrent && secondExtension.SourceSession.IsCurrent &&
                session.AcceptedExtensions == session.Extensions &&
                secondExtension.SourceSession.AcceptedExtensions == secondExtension.SourceSession.Extensions &&
                extension.PreviewView.DisplayedDocumentId == session.DocumentId &&
                extension.PreviewView.DisplayedRevision == session.AcceptedRevision &&
                secondExtension.PreviewView.DisplayedDocumentId == secondExtension.SourceSession.DocumentId &&
                secondExtension.PreviewView.DisplayedRevision == secondExtension.SourceSession.AcceptedRevision,
                "Both existing Markdown preview sessions display the accepted global YAML mode.");
            Check(extension.PreviewView.DisplayedText.Contains("title: YAML fixture", StringComparison.Ordinal) &&
                secondExtension.PreviewView.DisplayedText.Contains("title: YAML fixture", StringComparison.Ordinal),
                "A global preference change updates both attached preview models.");

            // Force the real bounded visual cache to unload the first preview.
            for (int index = 0; index < 9; index++)
            {
                var extra = await OpenAsync(host, "yaml-cache-" + index + ".md", Encoding.UTF8.GetBytes("# Cache " + index));
                extra.ShowHideContentPreview();
                await CurrentAsync(extra);
            }
            await UntilAsync(() => !extension.IsLoaded && !extension.PreviewView.IsLoaded,
                "The original YAML preview is detached with its native cache retained.");
            var cachedFull = session.FullSubmissions;
            ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.Hide;
            Check(!session.IsCurrent, "A detached cached document cannot claim the new YAML mode is already synchronized.");
            host.Core.SwitchTo(firstTab);
            await CheckYamlModeAsync(first, source, MarkdownFrontMatterStyle.Hide);
            Check(session.FullSubmissions > cachedFull && session.AcceptedExtensions == session.Extensions,
                "Reattaching a changed-mode cache submits complete source instead of resuming an old mode.");
            first.ShowHideContentPreview();
            await extension.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
            var retiredFull = session.FullSubmissions;
            ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.Table;
            Check(session.FullSubmissions == retiredFull && !session.IsCurrent,
                "A disposed preview stays retired after the global preference changes.");
            first.ShowHideContentPreview();
            extension = await CurrentAsync(first);
            session = extension.SourceSession;
            extension.FollowSource = false;
            Check(!ReferenceEquals(extension.SourceSession, secondExtension.SourceSession), "Each preview retains its own source identity.");
            await CheckYamlModeAsync(first, source, MarkdownFrontMatterStyle.Table);

            // The mode/composition cases above retain the original math input.
            // This race uses a resource-free body so BindMath cannot defer its
            // first actionable viewport event until a later resource reflow.
            source = source.Replace("$x^2$", "x squared", StringComparison.Ordinal);
            first.TestNativeCore.SetText(source);
            await CheckYamlModeAsync(first, source, MarkdownFrontMatterStyle.Table);

            // A new Main resets selection directly without SelectionChanged.
            // ViewportChanged publishes the actual new actionable frame and
            // runs before the source worker consumes its full-submission receipt.
            bool reentered = false, staleRejected = false, publicationWhileUpdating = false;
            var oldRevision = session.AcceptedRevision;
            var oldFull = session.FullSubmissions;
            var preview = extension.PreviewView;
            void Supersede(MarkdownView sender, MarkdownViewportAnchor anchor)
            {
                if (reentered || !anchor.IsAvailable || anchor.DocumentId != session.DocumentId ||
                    anchor.Revision <= oldRevision) return;
                reentered = true;
                publicationWhileUpdating = session.Status == MarkdownSessionStatus.Updating &&
                    session.FullSubmissions > oldFull;
                log.AppendLine($"OBS: Markdown.YamlPublication oldRevision={oldRevision}, publishedRevision={anchor.Revision}, textGeneration={anchor.TextGeneration}, layoutGeneration={anchor.LayoutGeneration}, workerUpdating={publicationWhileUpdating}.");
                ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.CodeBlock;
                staleRejected = !session.IsCurrent;
                native.SetEmptySelection(first.TestNativeCore.DocumentLength);
                first.TypeText("\r\rYAML publication tail Ω");
                ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.Table;
                ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.CodeBlock;
            }
            preview.ViewportChanged += Supersede;
            try
            {
                ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.Hide;
                await UntilAsync(() => reentered, "The pending full YAML submission reaches actual native publication.");
                await CheckYamlModeAsync(first, source + "\r\rYAML publication tail Ω", MarkdownFrontMatterStyle.CodeBlock);
                Check(publicationWhileUpdating && staleRejected && session.AcceptedExtensions == session.Extensions &&
                    preview.DisplayedText.Contains("YAML publication tail Ω", StringComparison.Ordinal),
                    "Rapid mode changes during publication reject the old identity and publish the final mode with the edit.");
            }
            finally { preview.ViewportChanged -= Supersede; }
            first.TestNativeCore.Undo();
            await CheckYamlModeAsync(first, source, MarkdownFrontMatterStyle.CodeBlock);
            var beforeDelta = session.DeltaSubmissions;
            native.SetEmptySelection(first.TestNativeCore.DocumentLength);
            first.TypeText("\r\rordinary YAML delta");
            await CheckYamlModeAsync(first, source + "\r\rordinary YAML delta", MarkdownFrontMatterStyle.CodeBlock);
            Check(session.DeltaSubmissions > beforeDelta && session.AcceptedExtensions == session.Extensions,
                "An ordinary source delta retains the accepted YAML extension identity.");
            first.TestNativeCore.Undo();
            await CheckYamlModeAsync(first, source, MarkdownFrontMatterStyle.CodeBlock);

            var invalidSource = MarkdownYamlFixture.Source(invalid: true);
            var invalidEditor = await OpenAsync(host, "yaml-invalid.md", Encoding.UTF8.GetBytes(invalidSource));
            invalidEditor.ShowHideContentPreview();
            foreach (var style in new[] { MarkdownFrontMatterStyle.Table, MarkdownFrontMatterStyle.CodeBlock, MarkdownFrontMatterStyle.Hide })
            {
                ApplicationPreferences.MarkdownFrontMatterView = style;
                var invalidPreview = (await CurrentAsync(invalidEditor)).PreviewView;
                var text = invalidPreview.DisplayedText;
                Check(text.Contains(MarkdownYamlFixture.BodyHeading, StringComparison.Ordinal) && invalidEditor.GetText() == invalidSource,
                    "Invalid YAML retains the original source and publishes the ordinary body in " + style);
                Check(style switch
                {
                    MarkdownFrontMatterStyle.Table => text.Contains("Failed to parse YAML front matter", StringComparison.Ordinal),
                    MarkdownFrontMatterStyle.CodeBlock => text.Contains("title: [unterminated", StringComparison.Ordinal),
                    _ => !text.Contains("unterminated", StringComparison.Ordinal) && !text.Contains("Failed to parse", StringComparison.Ordinal)
                }, "Invalid YAML uses the declared table diagnostic/code/hide behavior: " + style);
            }
            var plain = await OpenAsync(host, "yaml-plain.txt", Encoding.UTF8.GetBytes("Plain text"));
            var plainMenu = (TextEditorContextFlyout)plain.GetContextFlyout();
            using var plainSession = new ScrollSyncMenuSession(plainMenu);
            await plainSession.OpenAsync(plain.TestNativeCore);
            Check(plainMenu.PreviewToggle.Visibility == Visibility.Collapsed &&
                plainMenu.Items.All(item => AutomationProperties.GetAutomationId(item) != "MarkdownPreviewFrontMatter"),
                "An unsupported plain document keeps preview hidden and has no YAML menu command.");
            await plainSession.CloseAsync();
            await CheckStructuredFrontMatterAsync(host, log);
            await CheckColonDirectivesAsync(host, log);
            await CheckFixedPreviewTextAsync(host, log);
            await CheckCitationsAsync(host, log);
            log.AppendLine("PASS: Markdown.Yaml — typed persistence/default/corruption/Initialize; opened context menu without YAML submenu and retained Toggle Preview; direct mode selection and same-mode no-op; all modes/invalid YAML; original UTF-8/Undo/save point; attached previews, detached mode cache, close/reopen, publication reentry and ordinary delta; UIA selection/export, isolated source Copy, metadata/body mapping and body navigation/scroll. Native OS clipboard export and visual qualification use the external YAML fixture.");
        }
        finally
        {
            ApplicationPreferences.MarkdownFrontMatterView = original;
            if (stored == null) ApplicationSettingsStore.Remove(key);
            else ApplicationSettingsStore.Write(key, stored);
        }
    }

    private static async Task CheckStructuredFrontMatterAsync(FeatureTestHost host, StringBuilder log)
    {
        foreach (var sample in new[]
        {
            (Name: "toml", Source: "+++\rtitle = 'TOML 中文 😀'\rcount = 9007199254740993\r+++\r\r# Body 😀", Title: "TOML 中文 😀", Number: "9007199254740993"),
            (Name: "json", Source: "{\r  \"title\": \"JSON 中文 😀\",\r  \"count\": 1844674407370955161600000\r}\r\r# Body 😀", Title: "JSON 中文 😀", Number: "1844674407370955161600000")
        })
        {
            var bytes = Encoding.UTF8.GetBytes(sample.Source);
            var editor = await OpenAsync(host, "front-matter-" + sample.Name + ".md", bytes);
            editor.ShowHideContentPreview();
            var extension = await CurrentAsync(editor);
            extension.FollowSource = false;
            var view = extension.PreviewView;
            foreach (var mode in new[]
            {
                MarkdownFrontMatterStyle.Table, MarkdownFrontMatterStyle.CodeBlock, MarkdownFrontMatterStyle.Hide
            })
            {
                ApplicationPreferences.MarkdownFrontMatterView = mode;
                await UntilAsync(() =>
                {
                    var shown = view.DisplayedText;
                    if (!extension.SourceSession.IsCurrent || !shown.EndsWith("Body 😀", StringComparison.Ordinal))
                        return false;
                    if (mode == MarkdownFrontMatterStyle.Hide)
                        return shown == "Body 😀";
                    if (!shown.Contains(sample.Title, StringComparison.Ordinal) ||
                        !shown.Contains(sample.Number, StringComparison.Ordinal))
                        return false;
                    return shown.Contains(sample.Name == "toml" ? "title =" : "\"title\":", StringComparison.Ordinal) ==
                        (mode == MarkdownFrontMatterStyle.CodeBlock);
                }, "The " + sample.Name + " front matter uses the selected " + mode + " preview mode.");
                Check(ReadCurrentBytes(editor.TestNativeCore).SequenceEqual(bytes) && editor.GetText() == sample.Source,
                    "Changing the " + sample.Name + " front matter view preserves exact source bytes.");
            }
        }
        ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.Table;
        log.AppendLine("PASS: Markdown.FrontMatter verifies real TOML and JSON Table, Code and Hide views with exact Unicode source and numbers.");
    }

    private static async Task CheckCitationsAsync(FeatureTestHost host, StringBuilder log)
    {
        var metadata = "---\r\nreferences: [{id: note, title: 'Paper 😀', author: [{family: '研究组'}], issued: {date-parts: [[2020]]}}]\r\n---\r\n\r\n";
        var authored = metadata + "See [@note, p. 7] and [@note; @missing].\r\n\r\n" +
            string.Concat(Enumerable.Repeat("Body padding to make the bibliography offscreen.\r\n\r\n", 120));
        var bytes = Encoding.UTF8.GetBytes(authored);
        ApplicationPreferences.MarkdownFrontMatterView = MarkdownFrontMatterStyle.Hide;
        var editor = await OpenAsync(host, "local-citations.md", bytes);
        var loaded = ReadCurrentBytes(editor.TestNativeCore);
        var canonical = Encoding.UTF8.GetBytes(authored.Replace("\r\n", "\r", StringComparison.Ordinal));
        Check(loaded.SequenceEqual(canonical), "The CRLF fixture loads as the editor's canonical CR buffer.");
        var beforeUndo = editor.TestNativeCore.CanUndo;
        editor.ShowHideContentPreview();
        MarkdownExtensionView extension;
        try { extension = await CurrentAsync(editor); }
        catch (TimeoutException)
        {
            var current = editor.TestMarkdownPreview;
            log.AppendLine($"OBS: Citations session={current?.SourceSession?.Status}; ready={current?.PreviewView?.IsSurfaceReady}; render='{current?.PreviewView?.RenderingError}'.");
            throw;
        }
        extension.FollowSource = false;
        var view = extension.PreviewView;
        await UntilAsync(() => view.DisplayedText.Contains("See (研究组 2020, p. 7) and [@note; @missing].", StringComparison.Ordinal) &&
            view.DisplayedText.EndsWith("References\n研究组 (2020). Paper 😀", StringComparison.Ordinal),
            "The application displays one citation, the original unresolved group and a bibliography.");

        var peer = FrameworkElementAutomationPeer.CreatePeerForElement(view);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        MarkdownLinkRequestedEventArgs request = null;
        view.LinkRequested += (_, args) => request = args;
        AutomationPeer citation = null;
        await UntilAsync(() =>
        {
            citation = peer.GetChildren()?.SingleOrDefault(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink);
            return citation != null;
        }, "The citation is exposed as a native accessible hyperlink.");
        ((IInvokeProvider)citation.GetPattern(PatternInterface.Invoke)).Invoke();
        await UntilAsync(() => reading.GetVisibleRanges().Any(range =>
            range.GetText(-1).Contains("研究组 (2020). Paper 😀", StringComparison.Ordinal)),
            "Invoking the citation brings its bibliography entry into the native viewport.");
        var currentBytes = ReadCurrentBytes(editor.TestNativeCore);
        var unchanged = currentBytes.SequenceEqual(loaded) && editor.TestNativeCore.CanUndo == beforeUndo && !editor.IsModified;
        log.AppendLine($"OBS: CitationAction local={request?.LocalPreviewPosition}; editorByteLength={currentBytes.Length}; editorUnchanged={unchanged}.");
        Check(request?.LocalPreviewPosition is ulong && string.IsNullOrEmpty(request.Destination),
            "Citation invokes a typed local position without an external destination.");
        Check(unchanged, "Citation navigation preserves exact editor source.");
        log.AppendLine("PASS: Markdown.Citations resolves references and navigates locally from a native UIA hyperlink.");
    }

    private static async Task CheckColonDirectivesAsync(FeatureTestHost host, StringBuilder log)
    {
        const string source = ":::: info Outer **box**\rIntro 😀\r\r" +
            "::: details Fold me\r[Inner target](#end)\r:::\r" +
            "After fold\r::::\r\r:::{warning}\rWarning text\r:::\r\r## End";
        var bytes = Encoding.UTF8.GetBytes(source);
        var editor = await OpenAsync(host, "colon-directives.md", bytes);
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        extension.FollowSource = false;
        var view = extension.PreviewView;
        var peer = FrameworkElementAutomationPeer.CreatePeerForElement(view);
        IExpandCollapseProvider Fold() => peer.GetChildren()?.SingleOrDefault(child => child.GetName() == "Fold me")
            ?.GetPattern(PatternInterface.ExpandCollapse) as IExpandCollapseProvider;
        bool Link() => peer.GetChildren()?.Any(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink &&
            child.GetName() == "Inner target") == true;

        await UntilAsync(() => Fold()?.ExpandCollapseState == ExpandCollapseState.Collapsed && !Link(),
            "The nested colon disclosure is initially collapsed and hides its link.");
        Fold().Expand();
        await UntilAsync(() => Fold()?.ExpandCollapseState == ExpandCollapseState.Expanded && Link(),
            "Expanding the colon disclosure exposes its native hyperlink.");
        Fold().Collapse();
        await UntilAsync(() => Fold()?.ExpandCollapseState == ExpandCollapseState.Collapsed && !Link(),
            "Collapsing only the nested directive hides the link again.");
        Check(view.DisplayedText.Contains("Outer box\nIntro 😀", StringComparison.Ordinal) &&
            view.DisplayedText.Contains("After fold\nWarning\nWarning text\nEnd", StringComparison.Ordinal) &&
            !view.DisplayedText.Contains(":::", StringComparison.Ordinal) &&
            ReadCurrentBytes(editor.TestNativeCore).SequenceEqual(bytes),
            "Both directive dialects use native callouts while retaining authored bytes.");
        log.AppendLine("PASS: Markdown.ColonDirectives verifies nested disclosure, link visibility, styled titles and original bytes.");
    }

    private static async Task CheckFixedPreviewTextAsync(FeatureTestHost host, StringBuilder log)
    {
        const string source = "\"Quote\" -- once...\rNext line 😀.  \rHard break.\\\rAnother hard break.\r\r`\"Code\" -- ...`";
        const string expected = "\"Quote\" -- once... Next line 😀.\nHard break.\nAnother hard break.\n\"Code\" -- ...";
        var bytes = Encoding.UTF8.GetBytes(source);
        var editor = await OpenAsync(host, "preview-line-breaks.md", bytes);
        editor.ShowHideContentPreview();
        var view = (await CurrentAsync(editor)).PreviewView;
        Check(view.DisplayedText == expected && ReadCurrentBytes(editor.TestNativeCore).SequenceEqual(bytes),
            "The native preview folds soft breaks and preserves hard breaks, punctuation and editor bytes.");
        log.AppendLine("PASS: Markdown.FixedPreviewText verifies fixed line breaks and literal punctuation.");
    }

    private static async Task CheckYamlModeAsync(TextEditor editor, string source, MarkdownFrontMatterStyle style)
    {
        var extension = await CurrentAsync(editor);
        extension.FollowSource = false;
        var session = extension.SourceSession;
        var preview = extension.PreviewView;
        var scroll = Descendants(preview).OfType<ScrollViewer>().Single(element => element.Name == "PreviewScrollViewer");
        bool hasFarHeading = source.Contains("final body marker", StringComparison.Ordinal);
        bool hasFormula = source.Contains("$x^2$", StringComparison.Ordinal);
        (ulong Text, ulong Layout, double Offset, double Extent, double Viewport) previous = default;
        int stable = 0;
        // A current text generation can publish before math reflow or XAML extent arrangement finishes.
        await UntilAsync(() =>
        {
            if (!session.IsCurrent || !extension.TestThemeWidthLinkQuiescent || preview.IsResourceRenderingDeferred ||
                !HasReadingIdentity(preview, session.DocumentId, session.AcceptedRevision) ||
                scroll.ViewportHeight <= 0 || (hasFarHeading && scroll.ScrollableHeight <= 0))
            {
                stable = 0;
                return false;
            }

            try
            {
                if (hasFormula && !MarkdownMermaidTests.ReadObjects(preview).Any(item =>
                    item.Name == "Formula: x^2" && item.State != "Pending"))
                {
                    stable = 0;
                    return false;
                }
            }
            catch (Exception error) when (unchecked((uint)error.HResult) == 0x80040201)
            {
                stable = 0;
                return false;
            }

            var anchor = preview.GetViewportAnchor();
            var current = (anchor.TextGeneration, anchor.LayoutGeneration, scroll.VerticalOffset,
                scroll.ExtentHeight, scroll.ViewportHeight);
            stable = previous == current ? stable + 1 : 1;
            previous = current;
            return stable >= 4;
        }, "The current YAML view has settled before mapping and navigation: " + style, 25000);
        var text = preview.DisplayedText;
        Check(ApplicationPreferences.MarkdownFrontMatterView == style && session.AcceptedExtensions == session.Extensions &&
            editor.GetText() == source && text.Contains(MarkdownYamlFixture.BodyHeading, StringComparison.Ordinal) &&
            text.Contains("HTML body remains visible.", StringComparison.Ordinal),
            "The current YAML view retains original source and composed Markdown/HTML body: " + style);
        Check(style switch
        {
            MarkdownFrontMatterStyle.Table => text.Contains("YAML fixture", StringComparison.Ordinal) && !text.Contains("title: YAML fixture", StringComparison.Ordinal),
            MarkdownFrontMatterStyle.CodeBlock => text.Contains("title: YAML fixture", StringComparison.Ordinal),
            _ => !text.Contains("YAML fixture", StringComparison.Ordinal)
        }, "The actual native Main text distinguishes the requested YAML mode: " + style);
        var frame = preview.GetSelection();
        var sourceBody = source.IndexOf("ordinary", StringComparison.Ordinal) + 3;
        var sourceByte = Encoding.UTF8.GetByteCount(source[..sourceBody]);
        var body = await preview.MapSourcePositionAsync(session.DocumentId, session.AcceptedRevision, frame.TextGeneration,
            (ulong)sourceByte, MarkdownPositionAffinity.After).AsTask().WaitAsync(TimeSpan.FromSeconds(25));
        Check(body.Correspondence == MarkdownCorrespondence.Exact && body.SourceByteOffset == (ulong)sourceByte &&
            body.PreviewUtf16Offset == (ulong)(text.IndexOf("ordinary", StringComparison.Ordinal) + 3),
            "The body retains exact UTF-8 to UTF-16 coordinates after metadata: " + style);
        var meta = await preview.MapSourcePositionAsync(session.DocumentId, session.AcceptedRevision, frame.TextGeneration,
            8, MarkdownPositionAffinity.After).AsTask().WaitAsync(TimeSpan.FromSeconds(25));
        Check(meta.Correspondence == (style == MarkdownFrontMatterStyle.Hide ? MarkdownCorrespondence.Hidden : MarkdownCorrespondence.Atomic),
            "Metadata uses the declared whole-block/hidden correspondence: " + style);
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var provider = peer.GetPattern(PatternInterface.Text) as ITextProvider;
        Check(provider != null, "The actual YAML preview supplies its native text accessibility provider.");
        var selectedText = style == MarkdownFrontMatterStyle.Hide ? "ordinary" : "YAML fixture";
        var range = provider.DocumentRange.FindText(selectedText, false, false);
        Check(range != null && range.GetText(-1) == selectedText, "The native text range exports exact displayed YAML/body text: " + style);
        range.Select();
        Check(preview.GetSelection().Length == (ulong)selectedText.Length,
            "The actual native YAML/body text range can be selected: " + style);
        if (hasFarHeading)
        {
            Check(preview.NavigateToFragment(frame.DocumentId, frame.Revision, frame.TextGeneration, "#far-body-heading") ==
                MarkdownFragmentResult.Applied,
                "The real body heading remains navigable after metadata: " + style);
            await UntilAsync(() => scroll.VerticalOffset > 0 && preview.GetViewportAnchor().IsAvailable,
                "Body navigation advances the actual preview scroll position: " + style);
        }
    }
}
