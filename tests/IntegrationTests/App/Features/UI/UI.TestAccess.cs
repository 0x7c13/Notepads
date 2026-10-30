// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

// Compiled only into the isolated integration app. Seams expose real controls/handlers.
using System;
using Notepads.Presentation.Views.Settings;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using WinUIEditor;

namespace Notepads.Presentation.Views.MainPage
{
    public sealed partial class NotepadsMainPage
    {
        internal FrameworkElement TestFeatureControl(string name) => (FrameworkElement)FindName(name);
        internal SettingsPage TestFeatureSettingsPage => SettingsFrame.Content as SettingsPage;
        internal SplitView TestFeatureSettingsPane => RootSplitView;
        internal bool TestFeatureSettingsOpen => RootSplitView.IsPaneOpen;
        internal bool TestFeatureStatusVisible => StatusBar?.IsLoaded == true;
        internal bool TestFeatureStatusInteractive => StatusBar?.IsHitTestVisible == true;
        internal ElementTheme TestFeatureTheme => RootGrid.ActualTheme;
        internal MenuFlyout TestFeatureLanguageMenu => LanguageSelectionFlyout;
        internal MenuFlyout TestFeatureEncodingMenu => EncodingSelectionFlyout;
        internal void TestFeatureOpenEncodingMenu() => EncodingIndicatorClicked();
        internal void TestFeatureOpenLanguageMenu() => LanguageSelectionFlyout_OnOpening(LanguageSelectionFlyout, null);
        internal void TestFeatureSelectLanguage(MenuFlyoutItem item) => LanguageSelection_OnClick(item, new RoutedEventArgs());
        internal void TestFeatureOpenZoom() => FontZoomIndicatorClicked(NotepadsCore.GetSelectedReadyEditor());
        internal Flyout TestFeatureZoomFlyout => FontZoomIndicatorFlyout;
        internal void TestFeatureFullScreen() => EnterExitFullScreenMode();
        internal void TestFeatureCompactOverlay() => EnterExitCompactOverlayMode();
    }
}

namespace Notepads.Presentation.Views.Settings
{
    public sealed partial class TextAndEditorSettingsPage
    {
        internal bool TestFeatureEventsBound => _eventsBound;
    }

    public sealed partial class AdvancedSettingsPage
    {
        internal bool TestFeatureEventsBound => _eventsBound;
    }

    public sealed partial class PersonalizationSettingsPage
    {
        internal bool TestFeatureEventsBound => _eventsBound;
    }
}

namespace Notepads.Presentation.Controls.TextEditor
{
    public sealed partial class TextEditor
    {
        internal bool TestFindAllVisible => FindAllPanel?.Visibility == Visibility.Visible;
        internal ListView TestFindAllList => FindAllList;
        internal bool TestDiffPreparing => _diffGeneration?.Preparing == true;
        internal bool TestSearchPending => _searchCancellation != null;
    }

    public sealed partial class TextEditorCore
    {
        internal Editor TestFeatureNative => Native;
        internal (long, long) TestHighlightedBracePair => _highlightedBracePair;
    }

    public sealed partial class TextEditorContextFlyout
    {
        internal void TestFeatureRefresh() => TextEditorContextFlyout_Opening(this, null);
    }
}

namespace Notepads.Presentation.Workspace
{
    public sealed partial class TabContextFlyout
    {
        internal void TestFeatureRefresh() => TabContextFlyout_Opening(this, null);
        internal MenuFlyoutItem TestFeatureCommand(string name) => name switch
        {
            "Close" => Close,
            "CloseOthers" => CloseOthers,
            "CloseRight" => CloseRight,
            "CloseSaved" => CloseSaved,
            "Rename" => Rename,
            "MoveToNewWindow" => MoveToNewWindow,
            "CopyFullPath" => CopyFullPath,
            "OpenContainingFolder" => OpenContainingFolder,
            _ => throw new ArgumentOutOfRangeException(nameof(name))
        };
    }
}
