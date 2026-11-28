// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsWorkspaceTests;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    internal static async Task RunSourceFindAsync(StringBuilder log)
    {
        await SourceFindNavigationAsync(log);
        await SourceFindUnicodeAndEmptyAsync(log);
        await SourceFindAllAndReplaceAsync(log);
        await SourceFindInvalidationAsync(log);
        await SourceFindReentryAsync(log);
        await SourceFindClearReentryAsync(log);
        await SourceFindClearCommandReentryAsync(log);
        await SourceGeometryNavigationAsync(log);
    }

    private static async Task SourceGeometryNavigationAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var source = string.Join("\r\r", Enumerable.Range(0, 180).Select(index => "# REFLOW" + index.ToString("D4")));
        var editor = await OpenAsync(host, "source-geometry-navigation.md", Encoding.UTF8.GetBytes(source));
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        core.TextWrapping = TextWrapping.NoWrap;
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        editor.Focus();
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        bool Visible(string marker) => reading.GetVisibleRanges().Any(range => range.GetText(-1).Contains(marker, StringComparison.Ordinal));
        long Byte(string marker) => Encoding.UTF8.GetByteCount(source.AsSpan(0, source.IndexOf(marker, StringComparison.Ordinal)));
        Check(core.GoTo(1), "The source-only reflow case begins with a real Go To Line command.");
        await UntilAsync(() => !extension.TestSourceFindBusy && Visible("REFLOW0000") && !Visible("REFLOW0120"),
            "The initial target is visible and the later navigation target is outside the preview.");
        var before = preview.GetViewportAnchor();
        Check(before.IsAvailable, "The preview exposes an admitted initial layout.");
        int geometry = 0;
        void GeometryChanged(object sender, EventArgs args) => ++geometry;
        core.MarkdownViewportGeometryChanged += GeometryChanged;
        var held = extension.TestHoldNextSourceCaret(out var release);
        try
        {
            Check(core.GoTo(checked((int)native.LineFromPosition(Byte("REFLOW0120"))) + 1),
                "The real offscreen Go To Line command accepts the pending navigation target.");
            await held.WaitAsync(TimeSpan.FromSeconds(15));
            var sourceSelection = (native.Anchor, native.CurrentPos, native.SelectionRevision);
            // Toggle the actual native source wrapping property. The preview does
            // not subscribe to this setting, so it supplies no target-layout retry.
            int baseline = geometry;
            core.TextWrapping = TextWrapping.Wrap;
            await UntilAsync(() => geometry > baseline,
                "The native source wrapping command emits an actual host geometry notification while mapping is held.");
            await TabDragTestHost.FrameAsync();
            await TabDragTestHost.FrameAsync();
            var reflowed = preview.GetViewportAnchor();
            Check(reflowed.IsAvailable && reflowed.DocumentId == before.DocumentId && reflowed.Revision == before.Revision &&
                reflowed.TextGeneration == before.TextGeneration && reflowed.LayoutGeneration == before.LayoutGeneration &&
                sourceSelection == (native.Anchor, native.CurrentPos, native.SelectionRevision) && !Visible("REFLOW0120"),
                "Only source geometry changed: preview document/text/layout and the pending source selection stay fixed, with no early reveal.");
            release();
            var target = checked((ulong)preview.DisplayedText.IndexOf("REFLOW0120", StringComparison.Ordinal));
            await UntilAsync(() => !extension.TestSourceFindBusy && extension.SourceSession.IsCurrent &&
                preview.GetSelection().Start == target && preview.GetSelection().Length == 0 && Visible("REFLOW0120"),
                "A source-only reflow retries the current explicit navigation and reveals its fixed target without a second input.");
            await TabDragTestHost.FrameAsync();
            await TabDragTestHost.FrameAsync();
            Check(Visible("REFLOW0120") && sourceSelection == (native.Anchor, native.CurrentPos, native.SelectionRevision),
                "Natural Rendering keeps the retried target visible and preserves the exact source selection.");
            log.AppendLine("PASS: Markdown.SourceGeometryNavigation — held actual Go To mapping, real native source wrapping/geometry callback, unchanged preview layout, and eventual reveal without new input.");
        }
        finally
        {
            release();
            core.MarkdownViewportGeometryChanged -= GeometryChanged;
        }
    }

    private static async Task SourceFindNavigationAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "alpha ALPHA alpha 中😀 &amp; **bold** `code`";
        var editor = await OpenAsync(host, "source-find-navigation.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        extension.FollowSource = false;
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var bytes = ReadCurrentBytes(core);
        var undo = core.CanUndo;
        Check(extension.PreviewView.DisplayedText == "alpha ALPHA alpha 中😀 & bold code" &&
            !Descendants(extension).OfType<FrameworkElement>().Any(element => element.Name.StartsWith("PreviewFind", StringComparison.Ordinal)),
            "The fixed Main projection remains while the standalone preview Find controls are absent.");
        native.SetSel(0, 0);
        foreach (var expected in new long[] { 0, 6, 12, 0 })
        {
            Check(await editor.TestSourceFindAsync(new SearchContext("alpha")), "Source next search succeeds.");
            await AssertSourceFindAsync(editor, extension, expected, expected + 5, (ulong)expected, 5);
        }
        foreach (var expected in new long[] { 12, 6, 0, 12 })
        {
            Check(await editor.TestSourceFindAsync(new SearchContext("alpha"), previous: true), "Source previous search succeeds.");
            await AssertSourceFindAsync(editor, extension, expected, expected + 5, (ulong)expected, 5);
        }
        native.SetSel(8, 8);
        Check(await editor.TestSourceFindAsync(new SearchContext("ALPHA", matchCase: true)), "Case-sensitive source wrap succeeds.");
        await AssertSourceFindAsync(editor, extension, 6, 11, 6, 5);
        extension.FollowSource = true;
        var preview = extension.PreviewView;
        bool PreviewHasFocus()
        {
            for (var element = Windows.UI.Xaml.Input.FocusManager.GetFocusedElement(preview.XamlRoot) as DependencyObject;
                element != null; element = Windows.UI.Xaml.Media.VisualTreeHelper.GetParent(element))
                if (ReferenceEquals(element, preview)) return true;
            return false;
        }
        bool previewFocusCompleted = PreviewHasFocus();
        void PreviewGotFocus(object sender, RoutedEventArgs args) => previewFocusCompleted = true;
        preview.GotFocus += PreviewGotFocus;
        try
        {
            Check(preview.Focus(FocusState.Programmatic), "The actual preview admits programmatic focus before source Find.");
            await UntilAsync(() => previewFocusCompleted && PreviewHasFocus() && !extension.TestSourceFindBusy,
                "Preview GotFocus and its previous mapping complete before testing a source Find while preview remains focused.");
        }
        finally { preview.GotFocus -= PreviewGotFocus; }
        native.SetSel(0, 0);
        Check(await editor.TestSourceFindAsync(new SearchContext("alpha")),
            "A source search can take authority while the preview remains focused.");
        await AssertSourceFindAsync(editor, extension, 0, 5, 0, 5);
        Check(PreviewHasFocus(), "The source Find projects its range while the preview actually retains focus.");
        Check(ReadCurrentBytes(core).SequenceEqual(bytes) && core.CanUndo == undo && !editor.IsModified,
            "Source Find projection preserves exact bytes, save point and Undo.");
        log.AppendLine("PASS: Markdown.SourceFindNavigation — source next/previous/full wrap and case drive the same Main range, no standalone bar, bytes/Undo/save point and exact source selection preserved.");
    }

    private static async Task SourceFindUnicodeAndEmptyAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "中文😀 **bold** &amp; a\u0301 👩‍💻";
        const string main = "中文😀 bold & a\u0301 👩‍💻";
        var editor = await OpenAsync(host, "source-find-unicode.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        extension.FollowSource = false;
        var native = editor.TestNativeCore.TestFeatureNative;
        Check(extension.PreviewView.DisplayedText == main, "The Unicode/entity/strong fixture has the authored Main string.");
        foreach (var query in new[] { "中文😀", "bold", "amp", "**" })
        {
            native.SetSel(0, 0);
            var index = source.IndexOf(query, StringComparison.Ordinal);
            var start = Encoding.UTF8.GetByteCount(source.AsSpan(0, index));
            var end = start + Encoding.UTF8.GetByteCount(query);
            Check(await editor.TestSourceFindAsync(new SearchContext(query, matchCase: true)), "The source finds raw text: " + query);
            var visible = query == "amp" ? "&" : query == "**" ? "bold" : query;
            await AssertSourceFindAsync(editor, extension, start, end, (ulong)main.IndexOf(visible, StringComparison.Ordinal),
                query == "**" ? 0UL : (ulong)visible.Length);
        }
        foreach (var item in new[] { (Pattern: "(?=\u0301)", Text: "a\u0301", Point: "\u0301"),
            (Pattern: "(?=\u200d)", Text: "👩‍💻", Point: "\u200d") })
        {
            native.SetSel(0, 0);
            var index = source.IndexOf(item.Point, StringComparison.Ordinal);
            var position = Encoding.UTF8.GetByteCount(source.AsSpan(0, index));
            Check(await editor.TestSourceFindAsync(new SearchContext(item.Pattern, useRegex: true)), "The source regex finds an internal scalar boundary.");
            await AssertSourceFindAsync(editor, extension, position, position,
                (ulong)(main.IndexOf(item.Text, StringComparison.Ordinal) + item.Text.Length), 0);
        }
        log.AppendLine("PASS: Markdown.SourceFindUnicodeEmpty — UTF8 CJK/emoji and visible strong text, tight hidden delimiter, atomic entity interior, and zero-width regex inside combining/ZWJ clusters map to authored Main UTF16 ranges without moving the source selection.");
    }

    private static async Task SourceFindAllAndReplaceAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "source-find-all.md", Encoding.UTF8.GetBytes("alpha **alpha** alpha"));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        extension.FollowSource = false;
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        editor.ShowFindAndReplaceControl(false);
        editor.TestFindControl.TestFindInput.Text = "alpha";
        await TabDragTestHost.FrameAsync();
        Check(await editor.TestSourceFindAsync(new SearchContext("alpha"), mode: FindAndReplaceMode.FindAll), "Find All completes.");
        Check(!core.TryGetMarkdownSearchMatch(out _) && editor.TestFindAllList.Items.Count == 3,
            "Listing Find All rows does not publish a fabricated current match.");
        editor.TestFindAllList.SelectedIndex = 1;
        editor.TestConfirmSourceFindAll();
        await AssertSourceFindAsync(editor, extension, 8, 13, 6, 5);
        Check(!editor.TestFindAllVisible && core.TryGetMarkdownSearchMatch(out _),
            "The actual Enter confirmation command keeps its newly published match after CancelSearch/dismissal.");
        var results = await core.FindAllAsync(new SearchContext("alpha"), CancellationToken.None);
        Check(core.NavigateSearchResult(results, 2), "The same result navigation used by ItemClick selects the third row.");
        await AssertSourceFindAsync(editor, extension, 16, 21, 12, 5);
        native.SetSel(0, 0);
        Check(await editor.TestSourceFindAsync(new SearchContext("alpha"), mode: FindAndReplaceMode.Replace, replacement: "beta"),
            "The existing source Replace command succeeds.");
        extension = await CurrentAsync(editor);
        await AssertSourceFindAsync(editor, extension, 7, 12, 5, 5);
        Check(editor.GetText() == "beta **alpha** alpha", "Replace modifies only its source match and projects the following current match.");
        Check(!core.NavigateSearchResult(results, 0), "Old Find All rows cannot select after replacement.");
        log.AppendLine("PASS: Markdown.SourceFindAllReplace — list enumeration is not a current match; shared row navigation and actual Enter command publish exact ranges; Replace projects its following match and stale rows reject navigation.");
    }

    private static async Task SourceFindInvalidationAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var source = string.Join("\r\r", Enumerable.Range(0, 60).Select(index => $"needle{index:D4} plain paragraph"));
        var editor = await OpenAsync(host, "source-find-invalidation.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        extension.FollowSource = false;
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var view = extension.PreviewView;
        var scroll = Descendants(view).OfType<ScrollViewer>().Single(element => element.Name == "PreviewScrollViewer");
        var before = scroll.VerticalOffset;
        native.SetSel(0, 0);
        Check(await editor.TestSourceFindAsync(new SearchContext("needle0040")), "The offscreen source search succeeds with Follow off.");
        await UntilAsync(() => extension.TestOwnsSourceMatch && !extension.TestSourceFindBusy,
            "The source match maps without revealing while Follow is off.");
        Check(Math.Abs(scroll.VerticalOffset - before) < 0.001, "Follow off preserves the actual preview viewport offset.");
        native.SetSel(native.SelectionStart, native.SelectionEnd);
        Check(!core.TryGetMarkdownSearchMatch(out _), "A native selection claim at the same bytes invalidates the old match revision.");
        await UntilAsync(() => !extension.TestOwnsSourceMatch && !extension.TestSourceFindBusy,
            "The deferred native notification clears only the owned highlight.");
        Check(view.GetSelection().Length == 0, "The obsolete preview match is folded.");

        editor.ShowFindAndReplaceControl(false);
        editor.TestFindControl.TestFindInput.Text = "needle0040";
        await TabDragTestHost.FrameAsync();
        Check(await editor.TestSourceFindAsync(new SearchContext("needle0040")), "A fresh source match can be selected.");
        await UntilAsync(() => extension.TestOwnsSourceMatch, "A fresh preview match is owned.");
        editor.TestFindControl.TestFindInput.Text = "different";
        await UntilAsync(() => !core.TryGetMarkdownSearchMatch(out _) && !extension.TestOwnsSourceMatch,
            "Changing the ordinary source query clears the old highlight without needing a visible Find All panel.");
        Check(!await editor.TestSourceFindAsync(new SearchContext("missing-value")), "NotFound keeps no current match.");
        native.SetSel(0, 0);
        var stale = editor.TestSourceFindAsync(new SearchContext("needle[0-9]+", useRegex: true));
        native.SetSel(1, 1);
        Check(!await stale && !core.TryGetMarkdownSearchMatch(out _),
            "A native selection claim while the regex worker is in flight rejects its obsolete match.");
        Check(await editor.TestSourceFindAsync(new SearchContext("needle0040")), "Search can resume after NotFound.");
        await UntilAsync(() => extension.TestOwnsSourceMatch, "The resumed match appears.");
        editor.HideFindAndReplaceControl();
        Check(!core.TryGetMarkdownSearchMatch(out _) && !extension.TestOwnsSourceMatch, "Closing source Find clears its owned range.");
        Check(await editor.TestSourceFindAsync(new SearchContext("needle0040")), "Source matching can run while the query panel is closed.");
        core.TypeText("changed");
        await CurrentAsync(editor);
        Check(!core.TryGetMarkdownSearchMatch(out _) && !extension.TestOwnsSourceMatch,
            "Source editing and the new accepted revision cannot revive the old range.");
        editor.TestSetMarkdownPreviewOpen(false);
        await extension.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
        Check(!extension.TestOwnsSourceMatch, "Preview disposal releases range ownership and drains mapping admission.");
        log.AppendLine("PASS: Markdown.SourceFindInvalidation — Follow-off actual offset unchanged, same-byte new native selection revision rejected, query/NotFound/close/edit and preview disposal discard stale ownership.");
    }

    private static async Task SourceFindReentryAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "source-find-reentry.md", Encoding.UTF8.GetBytes("alpha beta gamma"));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        extension.FollowSource = false;
        var view = extension.PreviewView;
        var native = editor.TestNativeCore.TestFeatureNative;
        native.SetSel(0, 0);
        int calls = 0;
        void Reenter(MarkdownView sender, MarkdownSelection range)
        {
            if (range.Length != 5 || range.Start != 0 || calls++ != 0) return;
            sender.SelectRange(range.DocumentId, range.Revision, range.TextGeneration, 6, 4);
        }
        view.SelectionChanged += Reenter;
        try
        {
            Check(await editor.TestSourceFindAsync(new SearchContext("alpha")), "The source match is found before reentrant preview publication.");
            await UntilAsync(() => calls > 0 && !extension.TestSourceFindBusy, "The actual source projection completed its nested notification.");
            var selection = view.GetSelection();
            Check(selection.Start == 6 && selection.Length == 4 && !extension.TestOwnsSourceMatch,
                "A same-document nested beta selection remains external, and is not claimed as the alpha match.");
            editor.HideFindAndReplaceControl();
            selection = view.GetSelection();
            Check(selection.Start == 6 && selection.Length == 4, "Clearing source Find cannot fold the external nested selection.");
        }
        finally { view.SelectionChanged -= Reenter; }
        await UntilAsync(() => !editor.TestFindVisible,
            "The previous source Find close completes before starting the clear/close publication cases.");
        foreach (bool closeQuery in new[] { false, true })
        {
            editor.ShowFindAndReplaceControl(false);
            native.SetSel(0, 0);
            bool cleared = false;
            void ClearDuringPublication(MarkdownView sender, MarkdownSelection range)
            {
                if (cleared || range.Start != 0 || range.Length != 5) return;
                cleared = true;
                if (closeQuery) editor.HideFindAndReplaceControl();
                else editor.TestNativeCore.ClearMarkdownSearchMatch();
            }
            view.SelectionChanged += ClearDuringPublication;
            try
            {
                Check(await editor.TestSourceFindAsync(new SearchContext("alpha")),
                    "Source Find succeeds before its preview notification synchronously clears/closes the query.");
                await UntilAsync(() => cleared && !extension.TestSourceFindBusy,
                    "The source-match publication unwinds after synchronous clear/close.");
                var selection = view.GetSelection();
                Check(!editor.TestNativeCore.TryGetMarkdownSearchMatch(out _) && selection.Length == 0 &&
                    !extension.TestOwnsSourceMatch,
                    "Clear/close immediately retires the match and folds the proved tuple before any UI-close wait.");
                if (closeQuery)
                {
                    await UntilAsync(() => !editor.TestFindVisible,
                        "The source Find dismissal animation reaches its actual Closed state.");
                    selection = view.GetSelection();
                }
                Check(!editor.TestNativeCore.TryGetMarkdownSearchMatch(out _) && selection.Length == 0 &&
                    !extension.TestOwnsSourceMatch && (!closeQuery || !editor.TestFindVisible),
                    "Follow-off clear/close during the first publication folds its proved tuple and leaves no owned or pending match.");
                view.SelectRange(selection.DocumentId, selection.Revision, selection.TextGeneration, 6, 4);
                editor.HideFindAndReplaceControl();
                selection = view.GetSelection();
                Check(selection.Start == 6 && selection.Length == 4 && !extension.TestOwnsSourceMatch,
                    "A later source-query clear cannot fold an external beta selection after reentrant clear/close.");
                await UntilAsync(() => !editor.TestFindVisible,
                    "The source Find close completes before the next publication case can reopen it.");
            }
            finally { view.SelectionChanged -= ClearDuringPublication; }
        }
        native.SetSel(0, 0);
        bool superseded = false;
        Task<WinUIEditor.EditorSearchResult> newerSearch = null;
        MarkdownSearchMatch? newerMatch = null;
        void SupersedeDuringPublication(MarkdownView sender, MarkdownSelection range)
        {
            if (superseded || range.Start != 0 || range.Length != 5) return;
            superseded = true;
            sender.SelectRange(range.DocumentId, range.Revision, range.TextGeneration, 6, 4);
            newerSearch = editor.TestNativeCore.FindAsync(new SearchContext("beta", matchCase: true));
            if (editor.TestNativeCore.TryGetMarkdownSearchMatch(out var current)) newerMatch = current;
        }
        view.SelectionChanged += SupersedeDuringPublication;
        try
        {
            Check(await editor.TestSourceFindAsync(new SearchContext("alpha")),
                "Source A is found before its preview callback selects B and starts a newer source literal Find B.");
            await UntilAsync(() => superseded && !extension.TestSourceFindBusy,
                "The original A publication retires and the sole mapping admission publishes the newer source B.");
            Check(newerSearch is { IsCompletedSuccessfully: true } &&
                newerSearch.Result.Status == WinUIEditor.EditorSearchStatus.Found && newerMatch.HasValue,
                "The nested source literal Find B published its actual full match stamp synchronously.");
            await AssertSourceFindAsync(editor, extension, 6, 10, 6, 4);
            Check(editor.TestNativeCore.TryGetMarkdownSearchMatch(out var current) && current == newerMatch.Value,
                "Old A cleanup preserves B's complete serial/attachment/native/content/selection stamp and source selection.");
        }
        finally { view.SelectionChanged -= SupersedeDuringPublication; }
        log.AppendLine("PASS: Markdown.SourceFindReentry — nested B is never claimed as A; clear/close fold only proved A, external B survives later clear, and a newer synchronous source Find B retains its full stamp/selection and becomes the owned projection.");
    }

    private static async Task SourceFindClearReentryAsync(StringBuilder log)
    {
        await SourceFindClearReentryCaseAsync(log, "stable-clear", closeQuery: false, rollback: false);
        await SourceFindClearReentryCaseAsync(log, "stable-close", closeQuery: true, rollback: false);
        await SourceFindClearReentryCaseAsync(log, "publication-rollback", closeQuery: false, rollback: true);
        log.AppendLine("PASS: Markdown.SourceFindClearReentry — three isolated hosts verify stable owned clear/close and initial-publication rollback preserve a nested external range with Follow on; later ordinary caret, new Find and normal clear continue to synchronize.");
    }

    private static async Task SourceFindClearReentryCaseAsync(StringBuilder log, string label, bool closeQuery, bool rollback)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, $"source-find-clear-reentry-{label}.md", Encoding.UTF8.GetBytes("alpha beta gamma"));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        extension.FollowSource = true;
        editor.Focus();
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var view = extension.PreviewView;

        bool SourceHasFocus()
        {
            for (var element = Windows.UI.Xaml.Input.FocusManager.GetFocusedElement(view.XamlRoot) as DependencyObject;
                element != null; element = Windows.UI.Xaml.Media.VisualTreeHelper.GetParent(element))
                if (ReferenceEquals(element, core)) return true;
            return false;
        }

        void LogState(string stage)
        {
            var selected = view.GetSelection();
            bool hasMatch = core.TryGetMarkdownSearchMatch(out var match);
            var focused = Windows.UI.Xaml.Input.FocusManager.GetFocusedElement(view.XamlRoot);
            log.AppendLine($"OBS: SourceFindClearCase {label}/{stage}: native={native.Anchor}..{native.CurrentPos}@{native.SelectionRevision}; nativeRevision={native.DocumentRevision}; sourceFocus={SourceHasFocus()}; focused={focused?.GetType().Name}; previewFocus={view.FocusState}; findVisible={editor.TestFindVisible}; query='{editor.TestFindControl?.TestFindInput.Text}'; match={hasMatch}:{match}; owned={extension.TestOwnsSourceMatch}; busy={extension.TestSourceFindBusy}; session={extension.SourceSession.Status}/{extension.SourceSession.IsCurrent}@{extension.SourceSession.AcceptedRevision}; preview={selected.Start}+{selected.Length}, available={selected.IsAvailable}, revision={selected.Revision}, textGeneration={selected.TextGeneration}.");
        }

        async Task AssertFindAsync(long start, long end, ulong previewStart, ulong previewLength, string stage)
        {
            try { await AssertSourceFindAsync(editor, extension, start, end, previewStart, previewLength); }
            catch (TimeoutException) { LogState(stage); throw; }
        }

        async Task AssertExternalThenResumeAsync(bool closingQuery = false)
        {
            await UntilAsync(() => !extension.TestSourceFindBusy, "Clearing the old match retires its mapping work.");
            var beforeRendering = (native.Anchor, native.CurrentPos, native.SelectionRevision);
            void DelayedSourceSelection(object sender, RoutedEventArgs args) => LogState("natural-source-selection");
            core.SelectionChanged += DelayedSourceSelection;
            try
            {
                await TabDragTestHost.FrameAsync();
                await TabDragTestHost.FrameAsync();
            }
            finally { core.SelectionChanged -= DelayedSourceSelection; }
            var selected = view.GetSelection();
            bool preserved = selected.Start == 6 && selected.Length == 4 && !extension.TestOwnsSourceMatch;
            bool sourceUnchanged = beforeRendering == (native.Anchor, native.CurrentPos, native.SelectionRevision);
            if (!preserved || !sourceUnchanged) LogState("external-after-render");
            Check(sourceUnchanged, "Rendering alone preserves the native source selection and its revision after the external beta range.");
            Check(preserved,
                "The beta selection made inside the old match's collapse survives natural Rendering with Follow on.");
            // Dismiss has a real asynchronous Closed/focus-restoration phase.
            // Verify B before returning to the source, then act as actual source input.
            if (closingQuery)
                await UntilAsync(() => !editor.TestFindVisible, "The source Find dismissal reaches Closed before returning to source input.");
            editor.Focus();
            await UntilAsync(SourceHasFocus, "The actual source editor owns focus before its next ordinary caret input.");
            if (!closingQuery && !rollback)
            {
                var beforeRight = (Position: native.CurrentPos, Anchor: native.Anchor, Revision: native.SelectionRevision);
                Check(beforeRight.Position == 5 && beforeRight.Anchor == 0,
                    "The stable-clear source retains alpha's selected range before a same-byte Right collapse.");
                native.CharRight();
                Check(native.CurrentPos == beforeRight.Position && native.Anchor == beforeRight.Position &&
                    native.SelectionRevision > beforeRight.Revision,
                    "The real native Right command collapses alpha at the same caret byte with a new selection revision.");
                try
                {
                    await UntilAsync(() => !extension.TestSourceFindBusy && view.GetSelection().Start == 5 &&
                        view.GetSelection().Length == 0 && !extension.TestOwnsSourceMatch,
                        "A same-byte native Right collapse replaces the external beta range with the current source caret.");
                }
                catch (TimeoutException) { LogState("same-byte-right-collapse"); throw; }
            }
            var beforeCaret = (Position: native.CurrentPos, Anchor: native.Anchor, Revision: native.SelectionRevision);
            Check(beforeCaret.Position != 11 || beforeCaret.Anchor != 11,
                "The recovery input really changes the native selection rather than claiming the same caret.");
            int sourceSelections = 0;
            void SourceSelection(object sender, RoutedEventArgs args) => ++sourceSelections;
            core.SelectionChanged += SourceSelection;
            try
            {
                native.SetEmptySelection(11);
                Check(native.CurrentPos == 11 && native.Anchor == 11 && native.SelectionRevision > beforeCaret.Revision,
                    "The next ordinary source input advances the native selection revision at the requested caret.");
                await UntilAsync(() => !extension.TestSourceFindBusy && view.GetSelection().Start == 11 &&
                    view.GetSelection().Length == 0, "A later ordinary source caret still synchronizes after the external selection.");
            }
            catch (TimeoutException)
            {
                var after = view.GetSelection();
                log.AppendLine($"OBS: SourceFindClearResume {label}: before={beforeCaret}; native={native.Anchor}..{native.CurrentPos}@{native.SelectionRevision}; sourceFocus={SourceHasFocus()}; previewFocus={view.FocusState}; findVisible={editor.TestFindVisible}; sourceSelectionEvents={sourceSelections}; match={core.TryGetMarkdownSearchMatch(out _)}; owned={extension.TestOwnsSourceMatch}; busy={extension.TestSourceFindBusy}; preview={after.Start}+{after.Length}, available={after.IsAvailable}.");
                throw;
            }
            finally { core.SelectionChanged -= SourceSelection; }
            native.SetSel(0, 0);
            Check(await editor.TestSourceFindAsync(new SearchContext("beta")), "A later explicit source Find still works.");
            await AssertFindAsync(6, 10, 6, 4, "later-beta-find");
            core.ClearMarkdownSearchMatch();
            await UntilAsync(() => !extension.TestSourceFindBusy && view.GetSelection().Start == 10 &&
                view.GetSelection().Length == 0, "A normal clear still synchronizes the source caret after the match.");
        }

        // Only the close scenario needs a real query bar. Establish its Loaded,
        // focus and query-notification state before publishing the source match.
        if (closeQuery)
        {
            editor.ShowFindAndReplaceControl(false);
            await UntilAsync(() => editor.TestFindVisible && editor.TestFindControl is { IsLoaded: true } loadedFind &&
                loadedFind.TestFindInput.IsLoaded && ReferenceEquals(Windows.UI.Xaml.Input.FocusManager.GetFocusedElement(view.XamlRoot), loadedFind.TestFindInput),
                "The isolated close case has a loaded, focused source Find input before its first search.");
            var find = editor.TestFindControl;
            string observedQuery = null;
            void QueryChanged(object sender, SearchContext query) => observedQuery = query.SearchText;
            find.SearchQueryChanged += QueryChanged;
            try
            {
                find.TestFindInput.Text = "clear-reentry-fixture-reset";
                await UntilAsync(() => observedQuery == "clear-reentry-fixture-reset",
                    "The real source query-change callback consumes the fixture's setup query.");
                find.TestFindInput.Text = "alpha";
                await UntilAsync(() => observedQuery == "alpha",
                    "The real source query-change callback consumes alpha before its current match is published.");
            }
            finally { find.SearchQueryChanged -= QueryChanged; }
        }
        editor.Focus();
        await UntilAsync(SourceHasFocus, "The isolated case starts its source Find from the actual source editor.");
        if (!rollback)
        {
            native.SetSel(0, 0);
            Check(await editor.TestSourceFindAsync(new SearchContext("alpha")), "The stable alpha match is found.");
            await AssertFindAsync(0, 5, 0, 5, "initial-alpha-find");
            bool selected = false;
            void SelectDuringClear(MarkdownView sender, MarkdownSelection range)
            {
                if (selected || range.Start != 0 || range.Length != 0) return;
                selected = true;
                sender.SelectRange(range.DocumentId, range.Revision, range.TextGeneration, 6, 4);
            }
            view.SelectionChanged += SelectDuringClear;
            try
            {
                if (closeQuery) editor.HideFindAndReplaceControl();
                else core.ClearMarkdownSearchMatch();
                Check(selected, "Clearing a stable owned match really raises its collapsed selection notification.");
            }
            finally { view.SelectionChanged -= SelectDuringClear; }
            await AssertExternalThenResumeAsync(closeQuery);
            return;
        }

        // The first A publication can be cleared before an owned tuple exists.
        // Its subsequent rollback collapse needs the same notification proof.
        native.SetSel(0, 0);
        int stage = 0;
        void SelectDuringRollback(MarkdownView sender, MarkdownSelection range)
        {
            if (range.Start != 0) return;
            if (stage == 0 && range.Length == 5)
            {
                stage = 1;
                core.ClearMarkdownSearchMatch();
            }
            else if (stage == 1 && range.Length == 0)
            {
                stage = 2;
                sender.SelectRange(range.DocumentId, range.Revision, range.TextGeneration, 6, 4);
            }
        }
        view.SelectionChanged += SelectDuringRollback;
        try
        {
            Check(await editor.TestSourceFindAsync(new SearchContext("alpha")), "The rollback case finds alpha.");
            await UntilAsync(() => stage == 2 && !extension.TestSourceFindBusy,
                "A first-publication clear folds its proved old range and invokes the nested beta selection.");
        }
        finally { view.SelectionChanged -= SelectDuringRollback; }
        await AssertExternalThenResumeAsync();
    }

    private static async Task SourceFindClearCommandReentryAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var source = string.Join("\r\r", Enumerable.Range(0, 150).Select(index => $"# token{index:D4}"));
        var editor = await OpenAsync(host, "source-find-clear-command.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        extension.FollowSource = true;
        editor.Focus();
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var view = extension.PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(view) ?? FrameworkElementAutomationPeer.CreatePeerForElement(view);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        const string target = "token0120";
        long targetByte = Encoding.UTF8.GetByteCount(source.AsSpan(0, source.IndexOf(target, StringComparison.Ordinal)));
        ulong targetPreview = (ulong)view.DisplayedText.IndexOf(target, StringComparison.Ordinal);
        bool TargetVisible() => reading.GetVisibleRanges().Any(range => range.GetText(-1).Contains(target, StringComparison.Ordinal));

        foreach (bool find in new[] { false, true })
        {
            native.SetSel(0, 0);
            Check(await editor.TestSourceFindAsync(new SearchContext("token0000")), "The old stable top match is found.");
            await AssertSourceFindAsync(editor, extension, 2, 11, 0, 9);
            await UntilAsync(() => !TargetVisible(), "The newer command's target starts outside the preview viewport.");
            bool commanded = false, accepted = false;
            Task<WinUIEditor.EditorSearchResult> newerSearch = null;
            MarkdownSearchMatch? newerMatch = null;
            void CommandDuringClear(MarkdownView sender, MarkdownSelection range)
            {
                if (commanded || range.Start != 0 || range.Length != 0) return;
                commanded = true;
                if (find)
                {
                    newerSearch = core.FindAsync(new SearchContext(target, matchCase: true));
                    if (core.TryGetMarkdownSearchMatch(out var current)) newerMatch = current;
                }
                else accepted = core.GoTo(checked((int)native.LineFromPosition(targetByte)) + 1);
            }
            view.SelectionChanged += CommandDuringClear;
            try
            {
                core.ClearMarkdownSearchMatch();
                Check(commanded, "The stable-match collapse synchronously issues the newer source command.");
                await UntilAsync(() => !extension.TestSourceFindBusy && TargetVisible(),
                    "The newer source command retains its reveal after the older clear returns.");
                if (find)
                {
                    Check(newerSearch is { IsCompletedSuccessfully: true } &&
                        newerSearch.Result.Status == WinUIEditor.EditorSearchStatus.Found && newerMatch.HasValue,
                        "The nested literal Find publishes its actual full match stamp synchronously.");
                    await AssertSourceFindAsync(editor, extension, targetByte, targetByte + target.Length, targetPreview, (ulong)target.Length);
                    Check(core.TryGetMarkdownSearchMatch(out var current) && current == newerMatch.Value,
                        "The older clear retains the newer Find's complete source match identity.");
                }
                else
                {
                    var selected = view.GetSelection();
                    Check(accepted && native.CurrentPos == native.PositionFromLine(native.LineFromPosition(targetByte)) &&
                        selected.Start == targetPreview && selected.Length == 0,
                        "The newer Go To keeps its source position and preview caret projection.");
                }
            }
            finally { view.SelectionChanged -= CommandDuringClear; }
        }
        log.AppendLine("PASS: Markdown.SourceFindClearCommandReentry — a stable-match collapse can issue a newer source Go To or Find; the old clear preserves its real source identity, preview projection and offscreen-target reveal.");
    }

    private static async Task AssertSourceFindAsync(TextEditor editor, MarkdownExtensionView extension,
        long start, long end, ulong previewStart, ulong previewLength)
    {
        await UntilAsync(() =>
        {
            var selection = extension.PreviewView.GetSelection();
            return !extension.TestSourceFindBusy && extension.TestOwnsSourceMatch && selection.IsAvailable &&
                selection.Start == previewStart && selection.Length == previewLength;
        }, $"Source UTF8 [{start},{end}) maps to fixed Main UTF16 [{previewStart},{previewStart + previewLength}).");
        var native = editor.TestNativeCore.TestFeatureNative;
        Check(editor.TestNativeCore.TryGetMarkdownSearchMatch(out var match) && match.Start == start && match.End == end &&
            native.SelectionStart == start && native.SelectionEnd == end && native.SelectionRevision == match.SelectionRevision,
            "The preview projection preserves the exact selected native UTF8 match and its selection revision.");
    }
}
