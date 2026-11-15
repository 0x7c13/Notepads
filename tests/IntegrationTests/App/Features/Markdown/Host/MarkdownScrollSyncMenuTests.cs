// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    internal static async Task RunScrollSyncMenuAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var body = string.Join("\r\r", System.Linq.Enumerable.Repeat("A paragraph for scrolling and editing.", 100));
        var editor = await OpenAsync(host, "scroll-sync-menu.md", Encoding.UTF8.GetBytes(body));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        var menu = (TextEditorContextFlyout)editor.GetContextFlyout();
        var height = preview.ActualHeight;

        using var menuSession = new ScrollSyncMenuSession(menu);
        IToggleProvider Toggle()
        {
            var item = menu.PreviewScrollSync;
            var peer = FrameworkElementAutomationPeer.FromElement(item) ?? FrameworkElementAutomationPeer.CreatePeerForElement(item);
            Check(!string.IsNullOrWhiteSpace(peer.GetName()), "The real context toggle has a localized accessible name.");
            return peer.GetPattern(PatternInterface.Toggle) as IToggleProvider;
        }

        await menuSession.OpenAsync(editor.TestNativeCore);
        await UntilAsync(() => menu.PreviewScrollSync.Visibility == Visibility.Visible && menu.PreviewScrollSync.IsEnabled,
            "The actual source context menu exposes sync only for an active editable Markdown preview.");
        Check(menu.PreviewScrollSync.IsChecked && extension.FollowSource, "Scroll sync starts enabled.");
        var action = Toggle();
        Check(action != null, "The native checkable menu provides an actual automation Toggle action.");
        action.Toggle();
        Check(!extension.FollowSource && !preview.ShowInactiveCaretMarker && !menu.PreviewScrollSync.IsChecked,
            "Actual menu automation disables the existing follow setter and inactive marker.");
        await menuSession.CloseAsync();
        Check(preview.ActualHeight == height, "The menu entry adds no toolbar or reading-area height change.");

        editor.TestNativeCore.TestFeatureNative.SetEmptySelection(editor.TestNativeCore.DocumentLength);
        editor.TypeText("\r\rEdited while sync is off.");
        await CurrentAsync(editor);
        Check(preview.DisplayedText.Contains("Edited while sync is off.", StringComparison.Ordinal) && !extension.FollowSource,
            "Disabling scroll sync still updates the real Markdown document.");

        await menuSession.OpenAsync(editor.TestNativeCore);
        await UntilAsync(() => menu.PreviewScrollSync.Visibility == Visibility.Visible, "The menu reopens.");
        Check(!menu.PreviewScrollSync.IsChecked, "Reopening reads the current follow state.");
        Toggle().Toggle();
        Check(extension.FollowSource && preview.ShowInactiveCaretMarker, "The same real command enables follow again.");
        await menuSession.CloseAsync();
        extension.FollowSource = false;
        await menuSession.OpenAsync(editor.TestNativeCore);
        Check(!menu.PreviewScrollSync.IsChecked, "Programmatic follow changes are reflected on menu opening.");
        await menuSession.CloseAsync();

        editor.ShowHideContentPreview();
        await extension.DisposalCompletion;
        await menuSession.OpenAsync(editor.TestNativeCore);
        Check(menu.PreviewScrollSync.Visibility == Visibility.Collapsed && !menu.PreviewScrollSync.IsEnabled,
            "Closing preview removes the stale menu action.");
        await menuSession.CloseAsync();
        editor.ShowHideContentPreview();
        var reopened = await CurrentAsync(editor);
        await menuSession.OpenAsync(editor.TestNativeCore);
        Check(menu.PreviewScrollSync.IsChecked && reopened.FollowSource, "A fresh preview keeps the default enabled behavior; persistence is separate.");
        await menuSession.CloseAsync();

        var plain = await OpenAsync(host, "plain.txt", Encoding.UTF8.GetBytes("Plain text"));
        var plainMenu = (TextEditorContextFlyout)plain.GetContextFlyout();
        using var plainMenuSession = new ScrollSyncMenuSession(plainMenu);
        await plainMenuSession.OpenAsync(plain.TestNativeCore);
        Check(plainMenu.PreviewScrollSync.Visibility == Visibility.Collapsed && !plainMenu.PreviewScrollSync.IsEnabled,
            "An unsupported plain document never exposes the Markdown sync command.");
        await plainMenuSession.CloseAsync();
        log.AppendLine("PASS: Markdown.ScrollSyncContextMenu — native localized Toggle, on/off, edits while off, unchanged reading height, programmatic refresh, close/reopen and unsupported file.");
    }

    // Each assertion observes a completed XAML lifecycle, including automatic
    // menu dismissal after an automation action. No delay substitutes for it.
    private sealed partial class ScrollSyncMenuSession : IDisposable
    {
        private readonly TextEditorContextFlyout _menu;
        private TaskCompletionSource<object> _closed;

        internal ScrollSyncMenuSession(TextEditorContextFlyout menu)
        {
            _menu = menu;
            _menu.Opening += Opening;
            _menu.Closed += Closed;
        }

        private void Opening(object sender, object args) =>
            _closed = new(TaskCreationOptions.RunContinuationsAsynchronously);

        private void Closed(object sender, object args) => _closed?.TrySetResult(null);

        internal async Task OpenAsync(FrameworkElement target)
        {
            await CloseAsync();
            var opened = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
            void Opened(object sender, object args) => opened.TrySetResult(null);
            _menu.Opened += Opened;
            try
            {
                _menu.ShowAt(target);
                await opened.Task.WaitAsync(TimeSpan.FromSeconds(10));
            }
            finally { _menu.Opened -= Opened; }
        }

        internal async Task CloseAsync()
        {
            _menu.Hide();
            if (_closed != null)
                await _closed.Task.WaitAsync(TimeSpan.FromSeconds(10));
        }

        public void Dispose()
        {
            _menu.Hide();
            _menu.Opening -= Opening;
            _menu.Closed -= Closed;
        }
    }
}
