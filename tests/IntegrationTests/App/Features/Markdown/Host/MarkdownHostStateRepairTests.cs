// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsWorkspaceTests;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using WinUIEditor;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    internal static async Task RunHostStateRepairAsync(StringBuilder log)
    {
        await DiffQueuedOwnershipAsync(log);
        await SessionPreviewRestorationAsync(log);
        await CodeFontFamilyAsync(log);
        await PreviewInputAndHiddenRestoreAsync(log);
        await EvictedPreviewImmediateReselectionAsync(log);
        await RunDeviceRecoveryAsync(log);
    }

    private static async Task DiffQueuedOwnershipAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "queued-diff.md", Encoding.UTF8.GetBytes("# Saved\r\rbody"));
        editor.ShowHideContentPreview();
        var originalPreview = await CurrentAsync(editor);
        await editor.OpenSideBySideDiffViewerAsync();
        Check(editor.TestMarkdownPanelOpen && ReferenceEquals(editor.TestMarkdownPreview, originalPreview),
            "Requesting Diff for an unmodified document keeps the current Markdown preview.");

        var native = editor.TestNativeCore.TestFeatureNative;
        native.GotoPos(0);
        editor.TypeText("x");
        native.DeleteBack();
        Check(editor.TestNativeCore.IsDocumentModified, "Recreating the saved text leaves a native modification.");
        await editor.OpenSideBySideDiffViewerAsync();
        Check(editor.Mode == TextEditorMode.Editing && editor.TestMarkdownPanelOpen &&
            ReferenceEquals(editor.TestMarkdownPreview, originalPreview),
            "A prepared Diff with identical text keeps the original Markdown preview.");
        await CurrentAsync(editor);
        native.GotoPos(0);
        editor.TypeText("changed ");
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var held = editor.TestHoldDocumentOperationAsync(async () =>
        {
            entered.SetResult();
            await release.Task;
        });
        await entered.Task;
        Task pending = null;
        try
        {
            pending = editor.OpenSideBySideDiffViewerAsync();
            Check(!pending.IsCompleted && editor.TestMarkdownPanelOpen && editor.TestNativeCore.IsEnabled &&
                ReferenceEquals(editor.TestMarkdownPreview, originalPreview),
                "A Diff queued behind the document coordinator leaves the current preview open.");
            editor.ShowHideContentPreview();
            editor.ShowHideContentPreview();
            Check(editor.TestMarkdownPanelOpen && editor.Mode == TextEditorMode.Editing,
                "Preview can reopen while the real Diff operation is still waiting for coordinator ownership.");
            release.SetResult();
            await held;
            await pending.WaitAsync(TimeSpan.FromSeconds(20));
            Check(editor.Mode == TextEditorMode.DiffPreview && !editor.TestMarkdownPanelOpen,
                "Taking actual Diff ownership closes the preview reopened during the coordinator wait.");
            editor.ShowHideContentPreview();
            Check(!editor.TestMarkdownPanelOpen,
                "The published Diff continues to exclude Markdown preview.");
        }
        finally
        {
            release.TrySetResult();
            await held;
            if (pending != null) await pending.WaitAsync(TimeSpan.FromSeconds(20));
            editor.CloseSideBySideDiffViewer();
        }
        Check(editor.TestNativeCore.IsEnabled && editor.Mode == TextEditorMode.Editing,
            "Closing Diff restores source editing without a retained pending-preview state.");
        editor.ShowHideContentPreview();
        await CurrentAsync(editor);
        log.AppendLine("PASS: Markdown.DiffQueuedOwnership — real held coordinator, preview reopen while Diff waits, exclusive publication and normal close/reopen.");
    }

    private static async Task SessionPreviewRestorationAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var longLine = new string('a', 320);
        const int ruleCount = 64;
        var text = string.Concat(Enumerable.Repeat(new string('-', 30) + "\r\r", ruleCount)) +
            string.Join('\r', Enumerable.Range(0, 120).Select(index => longLine + index));
        const int restoredLine = ruleCount * 2 + 25;
        var source = await OpenAsync(host, "session-source.md", Encoding.UTF8.GetBytes(text));
        source.ShowHideContentPreview();
        var sourcePreview = await CurrentAsync(source);
        Check(sourcePreview.FollowSource && sourcePreview.FindName("FollowToggle") == null &&
            sourcePreview.FindName("ImageFolderButton") == null && sourcePreview.FindName("RemoteImagesToggle") == null,
            "New previews follow by default without showing the three option controls.");
        using var recovery = source.CaptureRecoveryState();
        Check(recovery.MetaData.IsContentPreviewPanelOpened,
            "The actual persisted metadata captures an open Markdown preview.");
        source.ShowHideContentPreview();
        await sourcePreview.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
        var unsupported = await host.Folder.CreateFileAsync("session-unsupported.txt", CreationCollisionOption.FailIfExists);

        foreach (var path in new[] { "checkpoint", "file", "legacy" })
        foreach (var state in new[] { "open", "closed", "unsupported" })
        {
            var metadata = recovery.MetaData;
            metadata.IsContentPreviewPanelOpened = state != "closed";
            metadata.LanguageOverride = state == "unsupported" ? "plaintext" : null;
            metadata.WrapWord = false;
            metadata.SelectionStartPosition = 12;
            metadata.SelectionEndPosition = 21;
            metadata.FirstVisibleDocumentLine = restoredLine;
            metadata.FirstVisibleSubLine = 0;
            metadata.FirstVisibleRowOffset = 0;
            metadata.ScrollViewerHorizontalOffset = 24;
            var file = state == "unsupported" ? unsupported : source.EditingFile;
            var candidate = (TextEditor)host.Core.CreateTextEditor(Guid.NewGuid(), file, file.Name);
            if (path == "checkpoint")
                await candidate.RestoreV2Async(recovery.SavedSnapshot, recovery.RecoveryBaseline,
                    recovery.DocumentJournal, recovery.Checkpoint, file, metadata, recovery.TextDirty);
            else if (path == "file")
            {
                await candidate.InitAsync(recovery.SavedSnapshot, file);
                candidate.ResetEditorState(metadata);
            }
            else
                await candidate.RestoreAsync(recovery.SavedSnapshot, file, metadata, recovery.RecoveryBaseline);

            var expectedOpen = state == "open";
            Check(candidate.TestNativeCore.IsEnabled && candidate.TestMarkdownPanelOpen == expectedOpen,
                $"Successful {path}/{state} restoration applies its saved preview intent after enabling the core.");
            var beforeAttach = candidate.TestMarkdownPreview;
            if (beforeAttach != null)
                Check(!beforeAttach.IsLoaded && beforeAttach.SourceSession.FullSubmissions == 0,
                    $"The unattached {path} receiver records preview state without reading native source before Loaded.");
            var core = candidate.TestNativeCore;
            var view = (EditorBaseControl)((Grid)core.Content).Children[0];
            MarkdownViewportAnchor firstExposedFrame = null;
            void ObserveExposedPreview(object sender, object args)
            {
                if (firstExposedFrame == null && beforeAttach?.PreviewView is { Opacity: > 0, IsSurfaceReady: true } preview)
                    firstExposedFrame = preview.GetViewportAnchor();
            }
            CompositionTarget.Rendering += ObserveExposedPreview;
            try
            {
                host.Core.OpenTextEditor(candidate);
                await candidate.TestStopFileChecksAsync();
                await UntilAsync(() => core.IsLoaded && !view.IsViewportRestorePending,
                    $"The {path}/{state} restored native source is loaded and finishes its viewport restoration.");
                if (expectedOpen)
                {
                    var restoredPreview = await CurrentAsync(candidate);
                    await UntilAsync(() => firstExposedFrame != null, $"The {path} preview exposes a rendered frame.");
                    Check(restoredPreview.FollowSource, "Restored previews follow by default.");
                    Check(firstExposedFrame.IsAvailable && firstExposedFrame.PreviewUtf16Offset > 10 * (ulong)longLine.Length,
                        $"The first exposed {path} frame is near the saved source line, offset={firstExposedFrame.PreviewUtf16Offset}.");
                    var preview = restoredPreview.PreviewView;
                    Check(preview.IsEnabled && preview.Focus(FocusState.Pointer),
                        $"The exposed {path} preview accepts pointer focus.");
                    await TabDragTestHost.FrameAsync();
                    var afterFocus = preview.GetViewportAnchor();
                    Check(afterFocus.IsAvailable && afterFocus.PreviewUtf16Offset == firstExposedFrame.PreviewUtf16Offset &&
                        afterFocus.OffsetWithinLine == firstExposedFrame.OffsetWithinLine,
                        $"First focus leaves the {path} preview at its first exposed viewport.");
                }
            }
            finally { CompositionTarget.Rendering -= ObserveExposedPreview; }
            await TabDragTestHost.FrameAsync();
            core.GetViewportAnchor(out var line, out var subLine);
            core.GetScrollViewerPosition(out var horizontal, out _);
            Selection(candidate, 12, 21, $"The {path}/{state} restoration retains its saved selection.");
            Check(line == restoredLine && subLine == 0 && Math.Abs(horizontal - 24) <= 1,
                $"The {path}/{state} restoration retains source viewport: line={line}, subline={subLine}, leading={horizontal}.");
            candidate.TestSetMarkdownPreviewOpen(expectedOpen);
            Check(candidate.TestMarkdownPanelOpen == expectedOpen && ReferenceEquals(beforeAttach, candidate.TestMarkdownPreview),
                $"Reapplying the {path}/{state} preview intent is idempotent and retains its extension owner.");
            core.GetViewportAnchor(out var sameLine, out var sameSubLine);
            core.GetScrollViewerPosition(out var sameHorizontal, out _);
            Check(sameLine == line && sameSubLine == subLine && sameHorizontal == horizontal,
                $"The idempotent {path}/{state} preview apply leaves the restored viewport untouched.");

            var native = core.TestFeatureNative;
            if (expectedOpen)
            {
                core.Focus(FocusState.Programmatic);
                native.SetEmptySelection(native.PositionFromLine(line.Value));
                native.PageDown();
                await UntilAsync(() =>
                {
                    var anchor = beforeAttach.PreviewView.GetViewportAnchor();
                    return native.FirstVisibleLine > line.Value && anchor.IsAvailable &&
                        anchor.PreviewUtf16Offset > firstExposedFrame.PreviewUtf16Offset;
                }, $"Page Down in the restored {path} source still scrolls the preview.");
            }
            native.GotoPos(10);
            native.ChooseCaretX();
            native.LineDown();
            Check(native.CurrentPos == native.GetLineEndPosition(1),
                "The short source line clamps the caret while retaining a preferred column.");
            candidate.TestSetMarkdownPreviewOpen(expectedOpen);
            native.LineDown();
            Check(native.CurrentPos == native.PositionFromLine(2) + 10,
                $"Idempotent {path}/{state} preview apply preserves the native preferred column.");
            log.AppendLine($"OBS: Markdown.SessionPreviewRestoration path={path}, state={state}, open={candidate.TestMarkdownPanelOpen}, sourceLine={line}, leading={horizontal}.");
        }
        log.AppendLine("PASS: Markdown.SessionPreviewRestoration — three real restore implementations with open/closed/unsupported state, pre-Loaded read gate, source selection/viewport and idempotent preferred-column retention.");
    }

    private static async Task CodeFontFamilyAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string body = "plain font reference";
        const string code = "MMMMiiiiWWWWllll0000";
        var editor = await OpenAsync(host, "code-font.md", Encoding.UTF8.GetBytes(body + "\r\r```text\r" + code + "\r```"));
        var core = editor.TestNativeCore;
        var original = core.FontFamily;
        try
        {
            core.FontFamily = new FontFamily("Consolas");
            editor.ShowHideContentPreview();
            var extension = await CurrentAsync(editor);
            var preview = extension.PreviewView;
            var beforeCode = TextMetrics(preview, code);
            var beforeBody = TextMetrics(preview, body);
            var theme = preview.ThemeGeneration;
            var layout = preview.GetViewportAnchor().LayoutGeneration;
            var revision = extension.SourceSession.AcceptedRevision;
            var fontSize = core.FontSize;
            var actualTheme = extension.ActualTheme;
            if (!new Windows.UI.ViewManagement.AccessibilitySettings().HighContrast)
            {
                extension.TestApplyPreviewTheme();
                Check(preview.ThemeGeneration == theme && preview.GetViewportAnchor().LayoutGeneration == layout,
                    "Reapplying the same standard theme does not invalidate the native frame.");
            }
            core.FontFamily = new FontFamily("Segoe UI");
            await WaitForFontLayoutAsync(preview, extension.SourceSession, theme, layout);
            var afterCode = TextMetrics(preview, code);
            var afterBody = TextMetrics(preview, body);
            Check(Math.Abs(afterCode.Width - beforeCode.Width) > 0.5,
                $"The actual code text obtains new font metrics: Consolas={beforeCode.Width}, Segoe UI={afterCode.Width}.");
            Check(Math.Abs(afterBody.Width - beforeBody.Width) <= 0.1 && Math.Abs(afterBody.Height - beforeBody.Height) <= 0.1 &&
                core.FontSize == fontSize && extension.ActualTheme == actualTheme && extension.SourceSession.AcceptedRevision == revision,
                "Changing only source FontFamily preserves body font, size, theme and source revision.");
            editor.ShowHideContentPreview();
            await extension.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
            var retiredTheme = preview.ThemeGeneration;
            core.FontFamily = new FontFamily("Consolas");
            Check(preview.ThemeGeneration == retiredTheme,
                "A disposed preview does not receive later source font changes.");
            editor.ShowHideContentPreview();
            var reopened = await CurrentAsync(editor);
            Check(Math.Abs(TextMetrics(reopened.PreviewView, code).Width - beforeCode.Width) <= 0.1,
                "A fresh active preview reads the latest source FontFamily after inactive changes.");
            log.AppendLine($"PASS: Markdown.CodeFontFamily — FontFamily alone updates native theme/layout and actual code width {beforeCode.Width}->{afterCode.Width}; body metrics and source revision stay fixed; dispose/reopen follows latest font.");
        }
        finally { core.FontFamily = original; }
    }

    private static (double Width, double Height) TextMetrics(MarkdownView preview, string text)
    {
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var provider = peer.GetPattern(PatternInterface.Text) as ITextProvider;
        Check(provider != null, "Font verification requires the actual native text provider.");
        var range = provider.DocumentRange.FindText(text, false, false);
        Check(range != null, "The measured font sample must exist in the actual rendered text: " + text);
        range.GetBoundingRectangles(out var rectangles);
        Check(rectangles.Length == 4 && rectangles[2] > 0 && rectangles[3] > 0,
            "The font sample must have one visible, nonempty native rectangle: " + text);
        return (rectangles[2], rectangles[3]);
    }
}
