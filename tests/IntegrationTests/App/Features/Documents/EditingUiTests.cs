// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Controls;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.System;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class EditingUiTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string original = "cat Cat scatter cat\rsecond line\rthird";
        var editor = await OpenAsync(host, "find.txt", Encoding.UTF8.GetBytes(original));
        await AccessibilityFeatureTests.CheckPageAsync(host, log);
        editor.TestNativeCore.SetTextSelectionPosition(0, 3);
        Selection(editor, 0, 3, "The cold Find fixture must begin with the intended native selection");
        Check(editor.TestNativeCore.GetSearchString() == "cat", "The native selection must provide cat before opening Find.");
        editor.ShowFindAndReplaceControl(showReplaceBar: false);
        var find = editor.TestFindControl;
        // FocusManager can report the new target before its routed GotFocus
        // handler runs. Wait for its observable selection effect as well.
        await UntilUiAsync(() => editor.TestFindVisible && find.IsLoaded &&
            ReferenceEquals(FocusManager.GetFocusedElement(), find.TestFindInput) &&
            find.TestFindInput.Text == "cat" && find.TestFindInput.SelectionStart == 0 && find.TestFindInput.SelectionLength == 3,
            "Find must prefill, focus and select the editor's selected search text.", () => find.TestInputState);
        Check(!find.TestReplaceVisible && !find.TestReplaceButton.IsEnabled && !find.TestReplaceAllButton.IsEnabled,
            "Find-only mode must disable replacement actions.");
        find.TestFindInput.Text = string.Empty;
        await UntilUiAsync(() => find.TestFindInput.Text == string.Empty &&
            !find.TestNextButton.IsEnabled && !find.TestPreviousButton.IsEnabled &&
            !find.TestReplaceButton.IsEnabled && !find.TestReplaceAllButton.IsEnabled,
            "An empty query must disable search and replacement actions.", () => find.TestInputState);
        find.TestFindInput.Text = "cat";
        await UntilUiAsync(() => find.TestFindInput.Text == "cat" && find.TestNextButton.IsEnabled && find.TestPreviousButton.IsEnabled,
            "A nonempty query must re-enable both search directions.", () => find.TestInputState);

        find.TestMatchCase.IsChecked = true;
        find.TestWholeWord.IsChecked = true;
        find.TestOptionsChanged();
        Check(!find.TestRegex.IsEnabled && find.TestOptionIndicatorVisible,
            "Whole-word mode must disable regex and display the options indicator.");
        Invoke(find.TestNextButton);
        await UntilAsync(() => Selected(editor, 16, 19), "Case-sensitive whole-word Next must skip Cat and scatter.");
        Invoke(find.TestPreviousButton);
        await UntilAsync(() => Selected(editor, 0, 3), "Previous must navigate back to the first whole-word match.");
        Check(editor.GetText() == original && !editor.IsModified, "Find navigation must preserve document text and save point.");
        find.TestWholeWord.IsChecked = false;
        find.TestRegex.IsChecked = true;
        find.TestOptionsChanged();
        Check(!find.TestWholeWord.IsEnabled && find.TestRegex.IsEnabled,
            "Regex mode must disable whole-word matching while retaining the active option.");
        find.TestFindInput.Text = "c.t";
        Invoke(find.TestNextButton);
        await UntilAsync(() => Selected(editor, 9, 12), "Regex Next must match the embedded cat in scatter.");
        log.AppendLine("PASS: editing/find actual UI prefill, focus, direction, case/word/regex options and selection.");

        find.TestRegex.IsChecked = false;
        find.TestWholeWord.IsChecked = true;
        find.TestOptionsChanged();
        find.TestFindInput.Text = "cat";
        find.TestToggleReplace();
        await UntilUiAsync(() => find.TestFindInput.Text == "cat" && find.TestReplaceVisible &&
            find.TestReplaceButton.IsEnabled && find.TestReplaceAllButton.IsEnabled,
            "The actual replace toggle must show and enable replacement controls for a nonempty query.", () => find.TestInputState);
        AccessibilityFeatureTests.CheckFindIcons(find);
        find.TestReplaceInput.Text = "dog";
        var routed = 0;
        FindAndReplaceEventArgs lastRequest = null;
        find.OnFindAndReplaceButtonClicked += (_, request) => { routed++; lastRequest = request; };
        Invoke(find.TestReplaceAllButton);
        await UntilAsync(() => editor.GetText() == "dog Cat scatter dog\rsecond line\rthird",
            "Replace All must apply the active case/whole-word options through the production search route.");
        Check(routed == 1 && lastRequest.FindAndReplaceMode == FindAndReplaceMode.ReplaceAll,
            "One Replace All invocation must route exactly one replace-all request.");
        editor.TestNativeCore.Undo();
        Check(editor.GetText() == original && !editor.IsModified, "One undo must restore every Replace All match and the save point.");
        Check(find.TestReplaceAllButton.KeyboardAccelerators.Any(accelerator => accelerator.Key == VirtualKey.Enter &&
            accelerator.Modifiers == (VirtualKeyModifiers.Control | VirtualKeyModifiers.Menu)),
            "Replace All's actual XAML accelerator must remain Ctrl+Alt+Enter.");
        Check(find.TestNextButton.KeyboardAccelerators.Any(accelerator => accelerator.Key == VirtualKey.F3 && accelerator.Modifiers == VirtualKeyModifiers.None) &&
            find.TestPreviousButton.KeyboardAccelerators.Any(accelerator => accelerator.Key == VirtualKey.F3 && accelerator.Modifiers == VirtualKeyModifiers.Shift),
            "The actual search buttons must declare forward/backward F3 accelerators.");

        editor.TestNativeCore.SetTextSelectionPosition(0, 3);
        find.TestSubmitReplaceFromInput();
        await UntilAsync(() => editor.GetText() == "dog Cat scatter cat\rsecond line\rthird" &&
            ReferenceEquals(FocusManager.GetFocusedElement(), find.TestReplaceInput) &&
            find.TestReplaceInput.SelectionStart == 0 && find.TestReplaceInput.SelectionLength == 3,
            "Replacement initiated from the input must replace the current match and restore input focus.");
        Check(routed == 2 && lastRequest.FindAndReplaceMode == FindAndReplaceMode.Replace,
            "Input replacement must route exactly one Replace request.");
        editor.TestNativeCore.Undo();
        Check(editor.GetText() == original && !editor.IsModified, "Single Replace must undo independently.");
        editor.TestNativeCore.SetTextSelectionPosition(0, 3);
        find.TestSubmitFindFromInput();
        await UntilAsync(() => Selected(editor, 16, 19) && ReferenceEquals(FocusManager.GetFocusedElement(), find.TestFindInput) &&
            find.TestFindInput.SelectionStart == 0 && find.TestFindInput.SelectionLength == 3,
            "Search initiated from the input must restore find focus after selecting the next match.");
        editor.TestEscape();
        await UntilAsync(() => !editor.TestFindVisible, "Escape must dismiss the active Find panel.");
        log.AppendLine("PASS: editing/replace routes one action, restores field focus and undoes replace-all in one step; XAML keyboard bindings verified.");

        editor.TestNativeCore.SetTextSelectionPosition(20, 20);
        editor.ShowGoToControl();
        var goTo = editor.TestGoToControl;
        await UntilUiAsync(() => editor.TestGoToVisible && ReferenceEquals(FocusManager.GetFocusedElement(), goTo.TestInput) &&
            goTo.TestInput.Text == "2" && goTo.TestInput.SelectionStart == 0 && goTo.TestInput.SelectionLength == 1,
            "Go To must prefill, focus and select the current one-based line.",
            () => $"text=[{goTo.TestInput.Text}], selection=[{goTo.TestInput.SelectionStart},{goTo.TestInput.SelectionLength}], focus={goTo.TestInput.FocusState}");
        AccessibilityFeatureTests.CheckGoToIcons(goTo);
        foreach (var rejected in new[] { "0", "4", "-1", "2 0", "no", "2147483648" })
        {
            var validation = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            void BeforeTextChanging(TextBox sender, TextBoxBeforeTextChangingEventArgs args)
            {
                if (args.NewText == rejected) validation.TrySetResult(args.Cancel);
            }
            goTo.TestInput.BeforeTextChanging += BeforeTextChanging;
            try
            {
                goTo.TestInput.Text = rejected;
                var canceled = await validation.Task.WaitAsync(TimeSpan.FromSeconds(10));
                Check(canceled && goTo.TestInput.Text == "2",
                    "Go To must reject invalid/out-of-range input without replacing the current value: " + rejected);
            }
            finally { goTo.TestInput.BeforeTextChanging -= BeforeTextChanging; }
        }
        goTo.TestInput.Text = string.Empty;
        await UntilAsync(() => goTo.TestInput.Text == string.Empty && goTo.TestSearchButton.Visibility == Visibility.Collapsed,
            "Clearing Go To must collapse its bound navigation button.");
        goTo.TestInput.Text = "3";
        await UntilAsync(() => goTo.TestInput.Text == "3" && goTo.TestSearchButton.Visibility == Visibility.Visible,
            "Valid Go To input must reveal its bound navigation button.");
        Invoke(goTo.TestSearchButton);
        await UntilAsync(() => !editor.TestGoToVisible && Selected(editor, 32, 32),
            "Go To's actual button must navigate to line three and dismiss the control.");
        Check(editor.GetText() == original && !editor.IsModified, "Go To validation and navigation must preserve document text.");

        editor.ShowFindAndReplaceControl(showReplaceBar: false);
        await UntilAsync(() => editor.TestFindVisible, "Find must reopen after Go To.");
        editor.ShowGoToControl();
        await UntilAsync(() => editor.TestGoToVisible && !editor.TestFindVisible, "Showing Go To must dismiss Find.");
        editor.TestEscape();
        await UntilAsync(() => !editor.TestGoToVisible, "Escape must dismiss Go To.");
        // Drain callbacks before host disposal; no synthetic physical keyboard or clipboard input.
        await editor.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
        log.AppendLine("PASS: editing/go-to validates actual TextBox input, live button binding, current-line prefill and panel exclusivity.");
        await CheckPanelCloseAcrossTabsAsync(host, editor, log);
    }

    private static async Task CheckPanelCloseAcrossTabsAsync(FeatureTestHost host, TextEditor editor, StringBuilder log)
    {
        var originalTab = host.Core.GetTab(editor);
        var other = await OpenAsync(host, "other-panel-tab.txt", Encoding.UTF8.GetBytes("other tab"));
        var otherTab = host.Core.GetTab(other);
        host.Core.SwitchTo(originalTab);
        await UntilAsync(() => editor.IsLoaded, "The panel fixture must be attached before opening.");

        foreach (var isFind in new[] { true, false })
        {
            void Open()
            {
                if (isFind) editor.ShowFindAndReplaceControl(showReplaceBar: false);
                else editor.ShowGoToControl();
            }
            Open();
            var panel = isFind ? editor.TestFindNotification : editor.TestGoToNotification;
            var input = isFind ? editor.TestFindControl.TestFindInput : editor.TestGoToControl.TestInput;
            var duration = panel.AnimationDuration;
            var closed = 0;
            void Closed(object sender, InAppNotificationClosedEventArgs args)
            {
                Check(args.DismissKind == InAppNotificationDismissKind.Programmatic,
                    "Unloading must preserve the accepted dismissal reason.");
                closed++;
            }
            void CancelClose(object sender, InAppNotificationClosingEventArgs args) => args.Cancel = true;
            async Task SwitchAwayAsync()
            {
                var unloaded = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
                void Unloaded(object sender, RoutedEventArgs args) => unloaded.TrySetResult(true);
                panel.Unloaded += Unloaded;
                try
                {
                    host.Core.SwitchTo(otherTab);
                    // IsLoaded becomes false before the routed Unloaded handlers run.
                    await unloaded.Task.WaitAsync(TimeSpan.FromSeconds(10));
                }
                finally { panel.Unloaded -= Unloaded; }
            }
            panel.Closed += Closed;
            try
            {
                await UntilAsync(() => input.IsLoaded && ReferenceEquals(FocusManager.GetFocusedElement(), input),
                    "The panel must open and focus its actual input.");
                // An ordinary tab switch, including a vetoed close, must not invent Closed.
                foreach (var cancelClose in new[] { false, true })
                {
                    if (cancelClose)
                    {
                        panel.Closing += CancelClose;
                        try { editor.TestEscape(); }
                        finally { panel.Closing -= CancelClose; }
                    }
                    await SwitchAwayAsync();
                    Check(closed == 0 && panel.Visibility == Visibility.Visible,
                        "Unloading an open panel or a cancelled close must preserve its open state.");
                    host.Core.SwitchTo(originalTab);
                    await UntilAsync(() => editor.IsLoaded && panel.IsLoaded, "Returning must reattach the same panel.");
                    Open();
                    await UntilAsync(() => ReferenceEquals(FocusManager.GetFocusedElement(), input),
                        "A preserved panel must remain focusable after returning.");
                }

                // Hold the close pending without racing the normal 50ms animation.
                // Dismiss and tab selection occur in one dispatcher turn; no timer is invoked by the test.
                panel.AnimationDuration = TimeSpan.FromMinutes(1);
                editor.TestEscape();
                Check(closed == 0 && panel.Visibility == Visibility.Visible, "Closing must initially be pending.");
                await SwitchAwayAsync();
                Check(closed == 1 && panel.Visibility == Visibility.Collapsed,
                    "Unloading must complete the accepted close exactly once, including the owner's visibility change.");

                panel.AnimationDuration = duration;
                host.Core.SwitchTo(originalTab);
                await UntilAsync(() => editor.IsLoaded, "The editor must reattach after closing its panel.");
                Open();
                await UntilAsync(() => panel.Visibility == Visibility.Visible && input.IsLoaded &&
                    ReferenceEquals(FocusManager.GetFocusedElement(), input), "The dismissed panel must reopen and focus normally.");
                Check(closed == 1, "Reopening must not deliver the retired close a second time.");
                editor.TestEscape();
                await UntilAsync(() => panel.Visibility == Visibility.Collapsed && closed == 2,
                    "The reopened panel must still complete an ordinary animated close exactly once.");
            }
            finally
            {
                panel.AnimationDuration = duration;
                panel.Closed -= Closed;
            }
        }

        // Destroy both lazy XAML panels while close completion is still pending.
        // Their fields may be cleared before Unloaded delivers Closed to the owner.
        var findPanel = editor.TestFindNotification;
        var goToPanel = editor.TestGoToNotification;
        var disposedCloses = 0;
        void DisposedClose(object sender, InAppNotificationClosedEventArgs args) => disposedCloses++;
        findPanel.Closed += DisposedClose;
        goToPanel.Closed += DisposedClose;
        try
        {
            findPanel.AnimationDuration = goToPanel.AnimationDuration = TimeSpan.FromMinutes(1);
            editor.ShowFindAndReplaceControl(showReplaceBar: false);
            editor.ShowGoToControl();
            editor.HideGoToControl();
            await host.Page.TestCloseTabAsync(originalTab);
            await UntilAsync(() => disposedCloses == 2, "Closing the tab must complete both pending panel closes safely.");
            Check(!host.Core.ContainsTab(originalTab) && findPanel.Visibility == Visibility.Collapsed &&
                goToPanel.Visibility == Visibility.Collapsed, "Disposed panels must close without relying on cleared XAML fields.");
        }
        finally
        {
            findPanel.Closed -= DisposedClose;
            goToPanel.Closed -= DisposedClose;
        }
        log.AppendLine("PASS: Find/Go To unload preserves open/vetoed panels, completes pending closes once, reopens and disposes safely.");
    }

    private static bool Selected(Notepads.Presentation.Controls.TextEditor.TextEditor editor, int start, int end)
    {
        editor.TestNativeCore.GetTextSelectionPosition(out var actualStart, out var actualEnd);
        return actualStart == start && actualEnd == end;
    }

    private static async Task UntilUiAsync(Func<bool> condition, string message, Func<string> describe)
    {
        try { await UntilAsync(condition, message); }
        catch (TimeoutException error) { throw new TimeoutException(message + " Actual " + describe(), error); }
    }
}
