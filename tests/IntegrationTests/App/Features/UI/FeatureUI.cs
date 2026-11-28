// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Views.Settings;
using Notepads.Presentation.Workspace;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class FeatureUI
{
    internal static T Named<T>(FrameworkElement owner, string name) where T : class
    {
        var element = owner.FindName(name) as T;
        Check(element != null, "Production control is present: " + name);
        return element;
    }

    // The peer invokes the production Click handler, including async command admission.
    internal static void Invoke(FrameworkElement element)
    {
        Check(element != null, "The command control exists.");
        var peer = FrameworkElementAutomationPeer.CreatePeerForElement(element);
        if (peer == null && element is MenuFlyoutItem menu) peer = new MenuFlyoutItemAutomationPeer(menu);
        if (peer == null && element is Button button) peer = new ButtonAutomationPeer(button);
        Check(peer?.GetPattern(PatternInterface.Invoke) is IInvokeProvider,
            "The production command exposes Invoke: " + element.Name);
        ((IInvokeProvider)peer.GetPattern(PatternInterface.Invoke)).Invoke();
    }

    internal static async Task<WorkspaceTab> NewTabAsync(NotepadsMainPage page, bool toolbar = false)
    {
        var before = page.TestCore.GetTabs().Select(tab => tab.Id).ToArray();
        Invoke(page.TestFeatureControl(toolbar ? "NewSetButton" : "MenuCreateNewButton"));
        await UntilAsync(() => page.TestCore.GetTabs().Length == before.Length + 1 &&
            page.TestCore.GetSelectedReadyEditor() != null,
            "The actual New command creates and selects one ready tab.");
        var tab = page.TestCore.GetSelectedTab();
        Check(!before.Contains(tab.Id), "New selects the newly created document.");
        await UntilAsync(() => ((FrameworkElement)tab.ReadyEditor).ActualWidth > 0,
            "The new editor is presented.");
        return tab;
    }

    internal static async Task<T> SettingsAsync<T>(NotepadsMainPage page, string tag) where T : Page
    {
        if (!page.TestFeatureSettingsOpen)
        {
            var previous = page.TestFeatureSettingsPage;
            var previousPageUnloaded = previous == null || !previous.IsLoaded;
            var opened = false;
            void OnOpened(SplitView sender, object args) => opened = true;
            void OnUnloaded(object sender, RoutedEventArgs args) => previousPageUnloaded = true;
            var pane = page.TestFeatureSettingsPane;
            pane.PaneOpened += OnOpened;
            if (previous != null) previous.Unloaded += OnUnloaded;
            try
            {
                Invoke(page.TestFeatureControl("MenuSettingsButton"));
                // PaneOpening performs Frame.Navigate. Before it fires, the old hidden
                // SettingsPage can still report IsLoaded and must not satisfy readiness.
                await UntilAsync(() => opened && previousPageUnloaded &&
                    !ReferenceEquals(page.TestFeatureSettingsPage, previous) &&
                    page.TestFeatureSettingsPage?.IsLoaded == true,
                    "The Settings pane opens a fresh page and unloads its previous frame content.");
            }
            finally
            {
                pane.PaneOpened -= OnOpened;
                if (previous != null) previous.Unloaded -= OnUnloaded;
            }
        }
        var settings = page.TestFeatureSettingsPage;
        var navigation = Named<NavigationView>(settings, "SettingsNavigationView");
        var panel = Named<SettingsPanel>(settings, "SettingsPanel");
        var frame = Named<Frame>(panel, "ContentFrame");
        var target = navigation.MenuItems.OfType<NavigationViewItem>().Single(item => (string)item.Tag == tag);
        var previousContent = frame.Content as Page;
        var previousUnloaded = ReferenceEquals(navigation.SelectedItem, target) || previousContent == null || !previousContent.IsLoaded;
        void OnPreviousUnloaded(object sender, RoutedEventArgs args) => previousUnloaded = true;
        if (previousContent != null) previousContent.Unloaded += OnPreviousUnloaded;
        try
        {
            navigation.SelectedItem = target;
            await UntilAsync(() => previousUnloaded && frame.Content is T content && SettingsPageReady(content),
                "Settings navigation loads and binds the requested production page: " + tag);
        }
        finally { if (previousContent != null) previousContent.Unloaded -= OnPreviousUnloaded; }
        return (T)frame.Content;
    }

    internal static async Task CloseSettingsAsync(NotepadsMainPage page)
    {
        var pane = page.TestFeatureSettingsPane;
        var paneRoot = Descendants(pane).OfType<FrameworkElement>().FirstOrDefault(element => element.Name == "PaneRoot");
        Check(paneRoot != null, "The production settings pane template exposes its visual root.");
        // Light dismissal may already have started or completed. The production
        // close storyboard keeps PaneRoot visible until its transition finishes.
        pane.IsPaneOpen = false;
        await UntilAsync(() => !pane.IsPaneOpen && paneRoot.Visibility == Visibility.Collapsed,
            "The settings pane completes its close transition before document interaction.");
    }

    private static bool SettingsPageReady(Page page) => page.IsLoaded && page.ActualWidth > 0 && (page switch
    {
        TextAndEditorSettingsPage text => text.TestFeatureEventsBound,
        AdvancedSettingsPage advanced => advanced.TestFeatureEventsBound,
        PersonalizationSettingsPage personalization => personalization.TestFeatureEventsBound,
        _ => true
    });

    internal static TextEditor Editor(WorkspaceTab tab) => (TextEditor)tab.ReadyEditor;
}
