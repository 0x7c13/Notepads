// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Workspace;
using Windows.System;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    private static async Task PreviewInputAndHiddenRestoreAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "host-input.md", Encoding.UTF8.GetBytes("# Source\r\r" +
            string.Join("\r\r", Enumerable.Range(0, 80).Select(index => "Body paragraph " + index))));
        var sourceTab = host.Core.GetSelectedTab();
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var otherText = string.Join("\r\r", Enumerable.Range(0, 120).Select(index => "Other tab paragraph " + index));
        var otherEditor = await OpenAsync(host, "host-input-other.txt", Encoding.UTF8.GetBytes(otherText));
        var otherTab = host.Core.GetSelectedTab();
        var otherView = (WinUIEditor.EditorBaseControl)((Grid)otherEditor.TestNativeCore.Content).Children[0];
        host.Core.SwitchTo(sourceTab);
        await CurrentAsync(editor);

        Action release = null;
        try
        {
            var mapped = extension.TestHoldNextSourceViewport(out release);
            extension.TestBeginSessionViewportRestore();
            await mapped.WaitAsync(TimeSpan.FromSeconds(15));
            Check(extension.TestSessionViewportRestoring && extension.TestViewportRestoreDeadlineArmed,
                "A held real source mapping leaves a ready session waiting under its restore deadline.");
            host.Core.SwitchTo(otherTab);
            Check(extension.PreviewView.Visibility == Visibility.Collapsed && extension.TestSessionViewportRestoring &&
                !extension.TestViewportRestoreDeadlineArmed,
                "Caching a still-restoring preview stops its deadline without finishing its restore.");
            host.Core.SwitchTo(sourceTab);
            Check(extension.TestSessionViewportRestoring && extension.TestViewportRestoreDeadlineArmed,
                "Returning to the ready retained preview rearms its restore deadline.");
            release();
            await UntilAsync(() => !extension.TestSessionViewportRestoring && extension.TestPreservingPreviewViewport,
                "The admitted source mapping finishes restoration and preserves its aligned viewport pair.");
        }
        finally { release?.Invoke(); }

        foreach (var key in new[] { VirtualKey.Control, VirtualKey.Shift, VirtualKey.Menu, VirtualKey.C })
        {
            extension.TestPreviewKeyIntent(key);
            Check(extension.TestPreservingPreviewViewport,
                "A modifier or non-navigation key preserves the restored viewport: " + key);
        }
        extension.TestPreviewKeyIntent(VirtualKey.Down);
        Check(!extension.TestPreservingPreviewViewport, "A real preview navigation key takes scroll authority.");

        var source = (WinUIEditor.EditorBaseControl)((Grid)editor.TestNativeCore.Content).Children[0];
        await UntilAsync(() => source.VerticalScrollProgress is >= 0 and < 1 &&
            extension.PreviewView.VerticalScrollProgress is >= 0 and < 1 &&
            !extension.TestSourceFindBusy, "Both panes have settled before holding a normal viewport map.");
        var mappedFollow = extension.TestHoldNextSourceViewport(out var releaseFollow);
        try
        {
            extension.TestBeginViewportScroll(preview: false);
            Check(source.TrySetVerticalScrollProgress(1), "The original source admits its exact bottom.");
            await mappedFollow.WaitAsync(TimeSpan.FromSeconds(15));
            var publication = extension.TestSourceLinkCompletion;
            host.Core.SwitchTo(otherTab);
            Check(extension.PreviewView.Visibility == Visibility.Collapsed && extension.PreviewView.IsLoaded,
                "The original preview is retained and hidden while its map is held.");
            await UntilAsync(() => otherView.VerticalScrollProgress >= 0 && !otherView.IsViewportRestorePending,
                "The neighboring source has finished its own viewport restoration.");
            Check(otherView.TrySetVerticalScrollProgress(0.35), "The foreground editor admits an interior position.");
            await UntilAsync(() => otherView.VerticalScrollProgress is > 0.25 and < 0.5 && !otherView.IsScrollAnimating,
                "The foreground editor's viewport can detect a leaked scroll.");
            var otherNative = otherEditor.TestNativeCore.TestFeatureNative;
            var otherBefore = (otherNative.CurrentPos, otherNative.SelectionRevision, otherNative.FirstVisibleLine,
                otherView.VerticalScrollProgress);
            Check(!publication.IsCompleted, "The original viewport publication is still held while the neighbor is active.");
            releaseFollow();
            await publication.WaitAsync(TimeSpan.FromSeconds(15));
            Check(ReferenceEquals(host.Core.GetSelectedTab(), otherTab) &&
                (otherNative.CurrentPos, otherNative.SelectionRevision, otherNative.FirstVisibleLine,
                    otherView.VerticalScrollProgress) == otherBefore,
                "Releasing the hidden preview's map leaves the foreground editor's selection and viewport alone.");
            host.Core.SwitchTo(sourceTab);
            await CurrentAsync(editor);
            await UntilAsync(() => source.VerticalScrollProgress == 1 && extension.PreviewView.VerticalScrollProgress == 1,
                "The returning cached preview follows its own source bottom.");
        }
        finally { releaseFollow(); }

        foreach (var (key, modifiers) in new[]
        {
            (VirtualKey.F, VirtualKeyModifiers.Control),
            (VirtualKey.H, VirtualKeyModifiers.Control),
            (VirtualKey.F, VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift),
            (VirtualKey.G, VirtualKeyModifiers.Control),
            (VirtualKey.F3, VirtualKeyModifiers.None)
        })
        {
            Check(extension.PreviewView.Focus(FocusState.Programmatic), "The real preview owns focus before a host command.");
            Check(editor.TestHandleMarkdownPreviewCommand(key, modifiers), "The preview routes an existing host command: " + key);
            Check(key == VirtualKey.G ? editor.TestGoToVisible : editor.TestFindVisible,
                "The requested source command opens its real host control: " + key);
        }
        Check(extension.PreviewView.Focus(FocusState.Programmatic) &&
            editor.TestHandleMarkdownPreviewCommand(VirtualKey.P, VirtualKeyModifiers.Menu) && !editor.TestMarkdownPanelOpen,
            "Alt+P closes the Markdown split while its preview owns focus.");
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var selection = (native.CurrentPos, native.Anchor, native.SelectionRevision);
        core.IsEnabled = false;
        try
        {
            core.MarkdownSetCaretByte(native.Length);
            Check((native.CurrentPos, native.Anchor, native.SelectionRevision) == selection,
                "Preview caret mapping cannot invalidate the source selection while a line operation disables the editor.");
        }
        finally { core.IsEnabled = true; }
        core.MarkdownSetCaretByte(native.Length);
        Check(native.CurrentPos == native.Length && native.Anchor == native.Length,
            "Caret mapping resumes when the source editor is enabled again.");
        log.AppendLine("PASS: Markdown.HostInput — focused-preview command dispatch, modifier-preserved follow state, navigation authority, and hide/rearm of a held real source-viewport restoration. Command dispatch uses a test seam, not injected keyboard input.");
        log.AppendLine("PASS: Markdown.CachedFollow — releasing a held viewport map leaves the foreground tab unchanged and the returning preview follows its own source.");
    }

    private static async Task EvictedPreviewImmediateReselectionAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        await OpenAsync(host, "cache-plain-neighbor.txt", Encoding.UTF8.GetBytes("Plain neighbor"));
        var plain = host.Core.GetSelectedTab();
        var waiting = new WorkspaceTab(await host.Core.CreateNewTextEditorAsync("waiting for presentation"));
        host.Core.IsEditorPresentationPaused = true;
        try
        {
            host.Core.OpenTab(waiting, select: false);
            host.Core.SwitchTo(waiting);
            Check(!waiting.HasBeenPresented, "Selecting a placeholder during a presentation pause must not report an editor load.");
        }
        finally
        {
            host.Core.IsEditorPresentationPaused = false;
            host.Core.ResumeEditorPresentation();
        }
        await UntilAsync(() => waiting.HasBeenPresented, "The editor reports presentation when its visual actually loads.");
        WorkspaceTab first = null;
        MarkdownExtensionView firstPreview = null;
        Notepads.Presentation.Controls.TextEditor.TextEditor firstEditor = null;
        for (int index = 0; index < 9; index++)
        {
            var editor = await OpenAsync(host, "immediate-cache-" + index + ".md", Encoding.UTF8.GetBytes("# Cached " + index));
            editor.ShowHideContentPreview();
            var preview = await AwaitCurrentAsync(editor, "initial tab " + index);
            if (index != 0) continue;
            first = host.Core.GetSelectedTab();
            firstPreview = preview;
            firstEditor = editor;
        }
        Check(firstEditor.TestCachedPresentationHidden && firstPreview.IsLoaded,
            "The first Markdown tab is still retained and hidden in the full eight-entry cache.");
        // No await/layout boundary: eviction and reselection happen before XAML
        // has an opportunity to deliver the evicted editor's Unloaded event.
        host.Core.SwitchTo(plain);
        host.Core.SwitchTo(first);
        Check(!firstEditor.TestCachedPresentationHidden && firstPreview.PreviewView.Visibility == Visibility.Visible,
            "Selecting an immediately evicted editor explicitly clears its cached-hidden state.");
        await AwaitCurrentAsync(firstEditor, "immediate reselection");
        log.AppendLine("PASS: Markdown.ImmediateCacheReselection — fills eight retained entries, evicts through a plain tab, then synchronously reselects the evicted Markdown editor before a XAML layout boundary.");

        async Task<MarkdownExtensionView> AwaitCurrentAsync(Notepads.Presentation.Controls.TextEditor.TextEditor editor, string stage)
        {
            try { return await CurrentAsync(editor); }
            catch (TimeoutException)
            {
                var extension = editor.TestMarkdownPreview;
                var session = extension?.SourceSession;
                LogViewportDiagnostics(log, extension?.PreviewView, stage, includeParts: false);
                log.AppendLine($"OBS: {stage} editor/extension loaded={editor.IsLoaded}/{extension?.IsLoaded}; hidden={editor.TestCachedPresentationHidden}; visibility={extension?.PreviewView.Visibility}; session={session?.Status}/{session?.IsCurrent}; cached={extension?.PreviewView.HasCachedDocument}.");
                throw;
            }
        }
    }
}
