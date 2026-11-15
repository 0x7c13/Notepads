// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Features.Preferences;
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
    internal static async Task RunCacheLifecycleAsync(StringBuilder log)
    {
        await ExplicitReopenAsync(log);
        await LifecycleAsync(log);
        await ClosingActiveCacheEntryAsync(log);
    }

    private static async Task ExplicitReopenAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var text = "# Reopen\r\r" +
            string.Join("\r\r", Enumerable.Range(1, 180).Select(index => $"reopen paragraph {index}"));
        var editor = await OpenAsync(host, "explicit-reopen.md", Encoding.UTF8.GetBytes(text));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var session = extension.SourceSession;
        var counters = (session.AcceptedRevision, session.FullSubmissions, session.DeltaSubmissions, session.CopiedBytes);
        extension.FollowSource = false;
        editor.ShowHideContentPreview();
        await extension.SuspensionCompletion.WaitAsync(TimeSpan.FromSeconds(20));
        Check(editor.TestHasMarkdownReopenEntry && !editor.TestMarkdownPanelOpen &&
            !extension.IsExtensionEnabled && !session.IsCurrent &&
            extension.PreviewView.IsCachedPresentationSuspended && extension.PreviewView.HasCachedDocument,
            "Explicit close retains one suspended model without an enabled source session.");
        await Task.Delay(170);
        Check(counters == (session.AcceptedRevision, session.FullSubmissions, session.DeltaSubmissions, session.CopiedBytes),
            "A dormant preview does not copy or submit source in the background.");
        var native = editor.TestNativeCore.TestFeatureNative;
        native.FirstVisibleLine = 120;
        await UntilAsync(() => native.FirstVisibleLine == 120,
            "The closed preview's source moves to a different viewport.");
        editor.ShowHideContentPreview();
        var reopened = await CurrentAsync(editor);
        await UntilAsync(() => reopened.PreviewView.Opacity == 1 && reopened.PreviewView.IsEnabled &&
            reopened.PreviewView.GetViewportAnchor().PreviewUtf16Offset > 0,
            "Explicit reopen reveals the current source viewport instead of the old reading position.");
        Check(ReferenceEquals(reopened, extension) && !editor.TestHasMarkdownReopenEntry && reopened.FollowSource &&
            counters == (session.AcceptedRevision, session.FullSubmissions, session.DeltaSubmissions, session.CopiedBytes),
            "An unchanged explicit reopen reuses its model with no copy, full submission or delta and resets follow.");

        editor.ShowHideContentPreview();
        await reopened.SuspensionCompletion.WaitAsync(TimeSpan.FromSeconds(20));
        var editAt = text.IndexOf("reopen paragraph 1", StringComparison.Ordinal);
        native.SetSel(editAt, editAt + 1);
        editor.TypeText("R");
        editor.ShowHideContentPreview();
        var edited = await CurrentAsync(editor);
        await reopened.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
        Check(!ReferenceEquals(edited, reopened) && edited.SourceSession.FullSubmissions == 1 &&
            !reopened.PreviewView.GetSourceState().HasSource &&
            edited.PreviewView.DisplayedText.Contains("Reopen paragraph 1", StringComparison.Ordinal),
            "A hidden same-length edit rejects and discards the old source before one fresh full submission.");
        await CompareFullSyncAsync(host, editor, "same-length hidden reopen edit", log);

        var originalStyle = ApplicationPreferences.MarkdownFrontMatterView;
        try
        {
            editor.ShowHideContentPreview();
            await edited.SuspensionCompletion.WaitAsync(TimeSpan.FromSeconds(20));
            ApplicationPreferences.MarkdownFrontMatterView = originalStyle == MarkdownFrontMatterStyle.Hide
                ? MarkdownFrontMatterStyle.Table : MarkdownFrontMatterStyle.Hide;
            editor.ShowHideContentPreview();
            var styled = await CurrentAsync(editor);
            await edited.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
            Check(!ReferenceEquals(styled, edited) && styled.SourceSession.FullSubmissions == 1 &&
                styled.SourceSession.AcceptedExtensions == styled.SourceSession.Extensions,
                "A changed parse setting rejects the dormant model and submits the current extensions once.");
            editor.ShowHideContentPreview();
            await styled.SuspensionCompletion.WaitAsync(TimeSpan.FromSeconds(20));
            var second = await OpenAsync(host, "second-reopen.md", Encoding.UTF8.GetBytes("# Second reopen"));
            second.ShowHideContentPreview();
            var secondPreview = await CurrentAsync(second);
            second.ShowHideContentPreview();
            await secondPreview.SuspensionCompletion.WaitAsync(TimeSpan.FromSeconds(20));
            await styled.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
            Check(!editor.TestHasMarkdownReopenEntry && second.TestHasMarkdownReopenEntry &&
                !styled.PreviewView.GetSourceState().HasSource,
                "The single dispatcher cache evicts and releases the previous dormant source.");
            second.TestSetMarkdownPreviewOpen(false);
            await secondPreview.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
            Check(!second.TestHasMarkdownReopenEntry && !secondPreview.PreviewView.GetSourceState().HasSource,
                "A terminal host close discards a dormant model immediately.");
            second.ShowHideContentPreview();
            var closingPreview = await CurrentAsync(second);
            second.ShowHideContentPreview();
            await host.Page.TestCloseTabAsync(host.Core.GetSelectedTab());
            await second.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
            Check(!second.TestHasMarkdownReopenEntry && closingPreview.SourceSession.Completion.IsCompleted &&
                !closingPreview.PreviewView.GetSourceState().HasSource,
                "Closing the editor evicts its dormant model and awaits source shutdown before native disposal.");
        }
        finally { ApplicationPreferences.MarkdownFrontMatterView = originalStyle; }
        log.AppendLine("PASS: Markdown.ExplicitReopen — bounded dormant owner, unchanged model hit, current source viewport and follow reset, same-length edit/settings misses and terminal release.");
    }

    private static async Task ClosingActiveCacheEntryAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var retained = new List<MarkdownExtensionView>();
        for (int index = 1; index <= 8; index++)
        {
            var editor = await OpenAsync(host, $"retained-{index}.md", Encoding.UTF8.GetBytes($"# Retained {index}"));
            editor.ShowHideContentPreview();
            retained.Add(await CurrentAsync(editor));
        }
        await OpenAsync(host, "uncached-neighbor.txt", Encoding.UTF8.GetBytes("plain text"));
        var neighbor = host.Core.GetSelectedTab();
        var closing = await OpenAsync(host, "closing.md", Encoding.UTF8.GetBytes("# Closing"));
        var closingTab = host.Core.GetSelectedTab();
        closing.ShowHideContentPreview();
        var closingPreview = await CurrentAsync(closing);
        Check(retained.All(preview => preview.IsLoaded), "Eight background Markdown presentations fill the cache.");
        int unloads = 0;
        retained[0].Unloaded += (_, _) => unloads++;
        await host.Page.TestCloseTabAsync(closingTab);
        await closing.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
        await UntilAsync(() => !closingPreview.IsLoaded, "The closed Markdown presentation detaches.");
        Check(ReferenceEquals(host.Core.GetSelectedTab(), neighbor) &&
            retained.All(preview => preview.IsLoaded) && unloads == 0,
            "Closing onto an uncached text tab preserves all eight live cached Markdown presentations.");
        log.AppendLine("PASS: Markdown.CloseCache — closing an active preview onto an uncached neighbor never evicts a live cached tab.");
    }

    private static async Task LifecycleAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var first = await OpenAsync(host, "first.md", Encoding.UTF8.GetBytes("# First\r\rfirst body"));
        var firstTab = host.Core.GetSelectedTab();
        first.ShowHideContentPreview();
        var loading = (Border)first.TestMarkdownPreview.FindName("PreviewLoading");
        bool HasSkeleton() => loading.Visibility == Visibility.Visible &&
            loading.Child is Notepads.Presentation.Controls.DocumentLoadingPlaceholder;
        // A short document can publish its first frame between two polling ticks.
        var sawSkeleton = HasSkeleton();
        var token = loading.RegisterPropertyChangedCallback(UIElement.VisibilityProperty, (_, _) => sawSkeleton |= HasSkeleton());
        try
        {
            await UntilAsync(() => sawSkeleton, "An initial Markdown load shows the document skeleton.");
        }
        finally { loading.UnregisterPropertyChangedCallback(UIElement.VisibilityProperty, token); }
        var oldExtension = await CurrentAsync(first);
        Check(oldExtension.FindName("PreviewLoading") is Border { Visibility: Visibility.Collapsed, Child: null },
            "Publishing the first Markdown frame removes the loading skeleton.");
        var oldSession = oldExtension.SourceSession;
        first.ShowHideContentPreview();
        await oldExtension.SuspensionCompletion.WaitAsync(TimeSpan.FromSeconds(20));
        Check(!first.TestMarkdownPanelOpen && !oldSession.IsCurrent,
            "Hiding the real preview closes its panel and suspends the old source lifetime.");
        var oldFull = oldSession.FullSubmissions;
        var oldDelta = oldSession.DeltaSubmissions;
        first.TestNativeCore.TestFeatureNative.SetSel(0, 0);
        first.TypeText("hidden edit ");
        await Task.Delay(170);
        Check(oldSession.FullSubmissions == oldFull && oldSession.DeltaSubmissions == oldDelta,
            "A hidden, retired preview receives no subsequent native edits.");
        first.ShowHideContentPreview();
        var firstExtension = await CurrentAsync(first);
        await oldExtension.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
        Check(!ReferenceEquals(firstExtension, oldExtension), "Reopening creates a fresh preview owner and complete source generation.");
        await CompareFullSyncAsync(host, first, "reopen after hidden edits", log);

        var secondText = "# Second\r\r" + string.Join("\r\r", Enumerable.Range(1, 100).Select(index => $"second body {index}"));
        var second = await OpenAsync(host, "second.md", Encoding.UTF8.GetBytes(secondText));
        var secondTab = host.Core.GetSelectedTab();
        second.ShowHideContentPreview();
        var secondExtension = await CurrentAsync(second);
        var secondSession = secondExtension.SourceSession;
        var secondVersion = (secondSession.AcceptedRevision, secondSession.FullSubmissions,
            secondSession.DeltaSubmissions, secondSession.CopiedBytes);
        bool secondReloaded = false;
        secondSession.Changed += (_, _) => secondReloaded |= secondSession.Status == MarkdownSessionStatus.Updating;
        var secondPreview = secondExtension.PreviewView;
        var secondScroll = Descendants(secondPreview).OfType<ScrollViewer>()
            .Single(element => element.Name == "PreviewScrollViewer");
        secondExtension.FollowSource = false;
        await UntilAsync(() => secondScroll.ScrollableHeight > 0,
            "The native scroll viewer has arranged the long document before requesting its midpoint.");
        var requestedOffset = secondScroll.ScrollableHeight * 0.5;
        log.AppendLine($"OBS: Markdown.Lifecycle midpoint request extent={secondScroll.ScrollableHeight:R}, viewport={secondScroll.ViewportHeight:R}, offset={secondScroll.VerticalOffset:R}.");
        Check(secondPreview.TrySetVerticalScrollProgress(0.5), "The UI cache fixture can scroll the actual preview.");
        try
        {
            await UntilAsync(() => secondScroll.VerticalOffset > 0 &&
                Math.Abs(secondScroll.VerticalOffset - requestedOffset) < 1,
                "The second tab scrolls away from the top before switching.");
        }
        catch (TimeoutException error)
        {
            throw new TimeoutException($"The requested midpoint was {requestedOffset:R}, actual offset {secondScroll.VerticalOffset:R} of {secondScroll.ScrollableHeight:R}.", error);
        }
        var extent = secondScroll.ScrollableHeight;
        var offset = secondScroll.VerticalOffset;
        var layout = secondPreview.GetViewportAnchor().LayoutGeneration;
        int unloads = 0;
        secondPreview.Unloaded += (_, _) => unloads++;
        for (int pass = 0; pass < 2; pass++)
        {
            host.Core.SwitchTo(firstTab);
            await CurrentAsync(first);
            Check(secondPreview.IsLoaded, "A recently viewed Markdown preview stays attached off screen.");
            host.Core.SwitchTo(secondTab);
            await CurrentAsync(second);
            Check(unloads == 0 && !secondReloaded && secondPreview.IsSurfaceReady &&
                secondPreview.GetViewportAnchor().LayoutGeneration == layout &&
                Math.Abs(secondScroll.ScrollableHeight - extent) < 1 && Math.Abs(secondScroll.VerticalOffset - offset) < 1,
                "Reusing the visual preserves the frame, scrollbar range and scroll position.");
        }

        host.Core.SwitchTo(secondTab);
        await CurrentAsync(second);
        second.TestNativeCore.TestFeatureNative.SetSel(0, 0);
        second.TypeText("pending tab update ");
        Check(!secondSession.IsCurrent, "The tab-switch fixture has a real pending source update.");
        host.Core.SwitchTo(firstTab);
        Check(secondPreview.IsLoaded && unloads == 0,
            "Switching away from a pending preview retains its provider and visual lifetime.");
        host.Core.SwitchTo(secondTab);
        await CurrentAsync(second);
        var originalHeight = secondPreview.Height;
        try
        {
            secondPreview.Height = 0;
            await UntilAsync(() => secondPreview.ActualHeight == 0,
                "A real zero-height preview blocks pending source publication.");
            var blockedVersion = (secondSession.FullSubmissions, secondSession.DeltaSubmissions, secondSession.CopiedBytes);
            secondSession.RequestFullSync();
            await Task.Delay(180);
            Check(blockedVersion == (secondSession.FullSubmissions, secondSession.DeltaSubmissions, secondSession.CopiedBytes),
                "A source request waits without copying or submitting while its preview has no render area.");
            secondPreview.Height = originalHeight;
            await CurrentAsync(second);
            Check(secondSession.FullSubmissions == blockedVersion.FullSubmissions + 1 &&
                secondSession.DeltaSubmissions == blockedVersion.DeltaSubmissions,
                "The actual SizeChanged event wakes one pending full submission without a new editor modification.");
        }
        finally { secondPreview.Height = originalHeight; }
        secondReloaded = false;
        secondVersion = (secondSession.AcceptedRevision, secondSession.FullSubmissions,
            secondSession.DeltaSubmissions, secondSession.CopiedBytes);

        host.Core.SwitchTo(firstTab);
        await CurrentAsync(first);
        Check(secondPreview.Visibility == Visibility.Collapsed, "The cached preview explicitly identifies its hidden state.");
        // Native image tests exercise the real capacity-denial notification.
        // Here invoke that same suspension API to isolate actual host resume/stamp handling.
        secondPreview.SuspendCachedDocument();
        Check(secondPreview.IsCachedPresentationSuspended && secondPreview.HasCachedDocument,
            "A pressure-retired cached preview keeps its explicit source model pin.");
        await Task.Delay(60);
        Check(secondPreview.IsCachedPresentationSuspended,
            "An enabled hidden source session cannot spontaneously reopen the retired presentation.");
        host.Core.SwitchTo(secondTab);
        await CurrentAsync(second);
        Check(!secondPreview.IsCachedPresentationSuspended && secondPreview.IsSurfaceReady && !secondReloaded &&
            secondVersion == (secondSession.AcceptedRevision, secondSession.FullSubmissions,
                secondSession.DeltaSubmissions, secondSession.CopiedBytes),
            "Showing a pressure-retired preview validates and resumes its unchanged source without a full submission.");

        foreach (var name in new[] { "third.md", "fourth.md" })
        {
            var extra = await OpenAsync(host, name, Encoding.UTF8.GetBytes("# " + name));
            extra.ShowHideContentPreview();
            await CurrentAsync(extra);
        }
        Check(firstExtension.IsLoaded && secondExtension.IsLoaded && secondPreview.IsLoaded && unloads == 0,
            "Four open Markdown previews retain their existing visuals instead of repeatedly unloading images.");
        for (int index = 5; index <= 10; index++)
        {
            var name = $"cached-{index}.md";
            var extra = await OpenAsync(host, name, Encoding.UTF8.GetBytes("# " + name));
            extra.ShowHideContentPreview();
            await CurrentAsync(extra);
        }
        await OpenAsync(host, "plain.txt", Encoding.UTF8.GetBytes("plain text"));
        await UntilAsync(() => !firstExtension.IsLoaded && !secondExtension.IsLoaded && !secondPreview.IsLoaded && unloads > 0,
            "Exceeding eight cached Markdown previews detaches the two least recently used visuals and raises the child Unloaded event.");
        Check(unloads == 1, $"Eviction unloads the second preview once. Observed {unloads} events.");
        first.TestNativeCore.TestFeatureNative.SetSel(first.TestNativeCore.DocumentLength, first.TestNativeCore.DocumentLength);
        first.TypeText(" background Ω");
        host.Core.SwitchTo(firstTab);
        await UntilAsync(() => first.IsLoaded && first.ActualWidth > 0, "The original Markdown tab loads again in the actual workspace.");
        firstExtension = await CurrentAsync(first);
        Check(firstExtension.PreviewView.DisplayedText.Contains("background Ω", StringComparison.Ordinal) &&
            !firstExtension.PreviewView.DisplayedText.Contains("second body", StringComparison.Ordinal),
            "Tab unload/reload publishes only its own latest native source.");
        await CompareFullSyncAsync(host, first, "tab reload after a detached edit", log);
        Check(unloads == 1, $"The second preview remains detached after exactly one Unloaded event. Observed {unloads} events.");
        host.Core.SwitchTo(secondTab);
        await CurrentAsync(second);
        Check(secondExtension.PreviewView.DisplayedText.Contains("second body", StringComparison.Ordinal),
            "The other Markdown tab keeps its independent source identity.");
        Check(secondExtension.PreviewView.HasCachedDocument && !secondReloaded &&
            secondVersion == (secondSession.AcceptedRevision, secondSession.FullSubmissions,
                secondSession.DeltaSubmissions, secondSession.CopiedBytes),
            "Reattaching an unchanged tab reuses the native model without updating status or submitting source bytes.");
        host.Core.SwitchTo(firstTab);
        await CurrentAsync(first);
        host.Core.SwitchTo(secondTab);
        await CurrentAsync(second);
        Check(!secondReloaded && secondSession.FullSubmissions == secondVersion.FullSubmissions,
            "Repeated tab switches reuse the same cached revision.");

        second.TestNativeCore.TestFeatureNative.SetSel(0, 0);
        second.TypeText("edit after cache ");
        await CurrentAsync(second);
        Check(secondSession.FullSubmissions == secondVersion.FullSubmissions + 1 &&
            secondExtension.PreviewView.DisplayedText.Contains("edit after cache", StringComparison.Ordinal),
            "The first real edit after reattachment resynchronizes the native source once.");
        second.TestNativeCore.TestFeatureNative.SetSel(0, 0);
        second.TypeText("pending close ");
        Check(!secondSession.IsCurrent, "The close fixture has an outstanding source update.");
        await ChooseDialogAsync(host.Page.TestCloseTabAsync(secondTab), "SecondaryButton");
        await second.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
        await secondExtension.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
        Check(!host.Core.ContainsTab(secondTab) && secondSession.Completion.IsCompleted,
            "Confirmed tab close awaits the real source/preview shutdown before native editor disposal.");

        host.Core.SwitchTo(firstTab);
        await CurrentAsync(first);
        first.TestNativeCore.TestFeatureNative.SetSel(0, 0);
        first.TypeText("diff modification ");
        await first.ToggleDiffPreviewAsync();
        Check(first.Mode == TextEditorMode.DiffPreview && !first.TestMarkdownPanelOpen,
            "Entering actual Diff Preview closes the Markdown panel.");
        first.ShowHideContentPreview();
        Check(!first.TestMarkdownPanelOpen && first.Mode == TextEditorMode.DiffPreview,
            "The Markdown command cannot reopen a split while Diff Preview owns the document surface.");
        first.CloseSideBySideDiffViewer();
        Check(first.Mode == TextEditorMode.Editing && first.GetText().Contains("diff modification", StringComparison.Ordinal),
            "Leaving Diff Preview retains the original native edits.");
        log.AppendLine("PASS: Markdown.Lifecycle — four-tab and pending-update visual retention; explicit hidden pressure suspension and stamp-validated resume; eight-background LRU eviction, detached source reuse, pending close disposal and Diff Preview exclusion.");
    }

    private static async Task SourceLimitAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "limit.md", Encoding.UTF8.GetBytes("small source"));
        editor.TestNativeCore.SetText(new string('x', checked((int)MarkdownSourceSession.MaximumSourceBytes + 1)));
        editor.ShowHideContentPreview();
        await UntilAsync(() => editor.TestMarkdownPreview?.SourceSession?.Status == MarkdownSessionStatus.TooLarge,
            "An over-limit source receives the explicit preview limit state.");
        var session = editor.TestMarkdownPreview.SourceSession;
        Check(session.CopiedBytes == 0 && session.FullSubmissions == 0 && editor.TestNativeCore.IsEnabled,
            "The host checks its source limit before copying bytes and leaves source editing enabled.");
        editor.TestNativeCore.SetText("# Recovered\r\rsmall source");
        await CurrentAsync(editor);
        Check(editor.TestMarkdownPreview.PreviewView.DisplayedText.Contains("small source", StringComparison.Ordinal),
            "Reducing the actual source below the limit recovers a current preview.");
        log.AppendLine("PASS: Markdown.SourceLimit — 4MiB admission precedes copying; source editing remains enabled and a smaller document recovers.");
    }

}
