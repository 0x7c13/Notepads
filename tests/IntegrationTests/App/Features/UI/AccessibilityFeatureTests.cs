// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using Windows.ApplicationModel.Resources;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class AccessibilityFeatureTests
{
    private static AutomationPeer Action(Button button, string name)
    {
        var peer = FrameworkElementAutomationPeer.CreatePeerForElement(button);
        Check(peer != null, "A presented action creates its native automation peer: " + button.Name);
        Check(peer.GetAutomationControlType() == AutomationControlType.Button &&
            peer.GetPattern(PatternInterface.Invoke) is IInvokeProvider && peer.GetName() == name &&
            !string.IsNullOrWhiteSpace(name), "Accessible button exposes its localized action: " + button.Name +
            "; actual=" + peer.GetName() + "; expected=" + name);
        return peer;
    }

    internal static async Task CheckPageAsync(FeatureTestHost host, StringBuilder log)
    {
        var page = host.Page;
        var resources = ResourceLoader.GetForCurrentView();
        Action(Named<Button>(page, "MainMenuButton"), resources.GetString("MainMenu_AutomationName"));
        Action(Named<Button>(page, "NewSetButton"), Named<MenuFlyoutItem>(page, "MenuCreateNewButton").Text);

        var originalTab = host.Core.GetSelectedTab();
        var newTab = await NewTabAsync(page, toolbar: true);
        var close = Descendants(page.TestContainer(newTab)).OfType<Button>().Single(button => button.Name == "CloseButton");
        Action(close, resources.GetString("Tab_ContextFlyout_CloseButtonDisplayText"));
        Invoke(close);
        await UntilAsync(() => !host.Core.ContainsTab(newTab) && ReferenceEquals(host.Core.GetSelectedTab(), originalTab),
            "The named tab-close action closes the new clean tab and restores the original document.");
        var sets = Named<Notepads.Controls.SetsView>(page, "Sets");
        foreach (var (id, key) in new[] { ("SetsScrollBackButton", "Tabs_ScrollBack_AutomationName"),
            ("SetsScrollForwardButton", "Tabs_ScrollForward_AutomationName") })
        {
            var button = Descendants(sets).OfType<RepeatButton>().Single(control => control.Name == id);
            // Overflow buttons are collapsed until the strip needs scrolling.
            Check(AutomationProperties.GetName(button) == resources.GetString(key),
                "Tab-strip scroll names are localized even before overflow: " + id);
        }

        // Both the initial x:Load and a later recreation must expose real actions.
        for (var recreation = 0; recreation < 2; recreation++)
        {
            if (recreation != 0)
            {
                page.ShowHideStatusBar(false);
                page.ShowHideStatusBar(true);
            }
            await UntilAsync(() => page.TestFeatureStatusVisible, "Status actions finish loading.");
            foreach (var name in new[] { "PathIndicator", "LineColumnIndicator", "FontZoomIndicator", "LineEndingIndicator", "EncodingIndicator" })
            {
                var text = Named<TextBlock>(page, name);
                var button = Named<Button>(page, name + "Button");
                await UntilAsync(() => button.IsLoaded && button.ActualHeight > 0 && AutomationProperties.GetName(button) == text.Text,
                    "Status action finishes layout and its name follows the current value.");
                var peer = Action(button, text.Text);
                Check(button.IsEnabled && button.IsTabStop && peer.IsKeyboardFocusable() &&
                    AutomationProperties.GetAccessibilityView(text) == AccessibilityView.Raw && button.ActualHeight == 25,
                    "Status action is keyboard accessible, has one semantic owner and retains its height: " + name);
            }
            var language = Named<TextBlock>(page, "LanguageIndicator");
            await UntilAsync(() => AutomationProperties.GetName(Named<Button>(page, "LanguageIndicatorButton")) ==
                AutomationProperties.GetName(language), "Language action retains its detailed localized name.");
            Action(Named<Button>(page, "LanguageIndicatorButton"), AutomationProperties.GetName(language));
        }

        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        var originalText = editor.GetText();
        editor.TypeText("temporary edit");
        Check(editor.IsModified, "The reload fixture has a real unsaved change.");
        await editor.ReloadFromEditingFileAsync();
        Check(editor.GetText() == originalText && !editor.IsModified && editor.CanChangeLanguage &&
            Named<Button>(page, "LanguageIndicatorButton").IsEnabled,
            "Modified-file reload restores language action availability after transient editor disablement.");
        Invoke(Named<Button>(page, "LineColumnIndicatorButton"));
        await UntilAsync(() => editor.TestGoToVisible, "UIA invocation of line/column opens the actual Go To control.");
        editor.TestEscape();
        await UntilAsync(() => !editor.TestGoToVisible, "Go To dismisses normally after accessible invocation.");

        foreach (var (buttonName, flyoutName) in new[] { ("FontZoomIndicatorButton", "FontZoomIndicatorFlyout"),
            ("LineEndingIndicatorButton", "LineEndingSelectionFlyout"), ("EncodingIndicatorButton", "EncodingSelectionFlyout"),
            ("LanguageIndicatorButton", "LanguageSelectionFlyout") })
        {
            var flyout = Named<FlyoutBase>(page, flyoutName);
            var opened = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            var closed = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            void Opened(object sender, object args) => opened.TrySetResult(true);
            void Closed(object sender, object args) => closed.TrySetResult(true);
            flyout.Opened += Opened;
            flyout.Closed += Closed;
            try
            {
                Invoke(Named<Button>(page, buttonName));
                await opened.Task.WaitAsync(TimeSpan.FromSeconds(10));
                flyout.Hide();
                await closed.Task.WaitAsync(TimeSpan.FromSeconds(10));
                await UntilAsync(() => editor.TestNativeCore.TestFeatureNative.Focus,
                    "Closing the status flyout restores editor focus: " + buttonName);
            }
            finally { flyout.Opened -= Opened; flyout.Closed -= Closed; }
        }
        log.AppendLine("PASS: accessibility/names and real status-button invocation survive recreation; menus and Go To open, close and restore focus.");
    }

    internal static void CheckFindIcons(Notepads.Presentation.Controls.FindAndReplace.FindAndReplaceControl find)
    {
        foreach (var name in new[] { "ToggleReplaceModeButton", "OptionButton", "SearchBackwardButton", "SearchForwardButton",
            "DismissButton", "ReplaceButton", "ReplaceAllButton" })
        {
            var button = Named<Button>(find, name);
            Action(button, ToolTipService.GetToolTip(button) as string);
        }
    }

    internal static void CheckExitOverlay(Button button) =>
        Action(button, ResourceLoader.GetForCurrentView().GetString("App_ExitCompactOverlayMode_Text"));

    internal static void CheckGoToIcons(Notepads.Presentation.Controls.GoTo.GoToControl goTo)
    {
        foreach (var name in new[] { "SearchButton", "DismissButton" })
        {
            var button = Named<Button>(goTo, name);
            Action(button, ToolTipService.GetToolTip(button) as string);
        }
    }
}
