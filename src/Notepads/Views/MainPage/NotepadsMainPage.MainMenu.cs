namespace Notepads.Views.MainPage
{
    using System.Collections.Generic;
    using System.Linq;
    using System.Threading.Tasks;
    using Windows.Storage;
    using Windows.Graphics.Printing;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Media;
    using Notepads.Services;

    public sealed partial class NotepadsMainPage
    {
        private void InitializeMainMenu()
        {
            MainMenuButton.Click += (sender, args) => FlyoutBase.ShowAttachedFlyout((FrameworkElement)sender);

            MenuCreateNewButton.Click += (sender, args) => NotepadsCore.OpenNewTextEditor(_defaultNewFileName);
            MenuCreateNewWindowButton.Click += async (sender, args) => await OpenNewAppInstanceAsync();
            MenuOpenFileButton.Click += async (sender, args) => await OpenNewFilesAsync();
            MenuSaveButton.Click += async (sender, args) => await SaveAsync(NotepadsCore.GetSelectedTextEditor(), saveAs: false);
            MenuSaveAsButton.Click += async (sender, args) => await SaveAsync(NotepadsCore.GetSelectedTextEditor(), saveAs: true);
            MenuSaveAllButton.Click += async (sender, args) => await SaveAllAsync(NotepadsCore.GetAllTextEditors());
            MenuFindButton.Click += (sender, args) => NotepadsCore.GetSelectedTextEditor()?.ShowFindAndReplaceControl(showReplaceBar: false);
            MenuReplaceButton.Click += (sender, args) => NotepadsCore.GetSelectedTextEditor()?.ShowFindAndReplaceControl(showReplaceBar: true);
            MenuFullScreenButton.Click += (sender, args) => EnterExitFullScreenMode();
            MenuCompactOverlayButton.Click += (sender, args) => EnterExitCompactOverlayMode();
            MenuPrintButton.Click += async (sender, args) => await PrintAsync(NotepadsCore.GetSelectedTextEditor());
            MenuPrintAllButton.Click += async (sender, args) => await PrintAllAsync(NotepadsCore.GetAllTextEditors());
            MenuSettingsButton.Click += (sender, args) => RootSplitView.IsPaneOpen = true;

            if (!App.IsPrimaryInstance)
            {
                MainMenuButton.Foreground = new SolidColorBrush(ThemeSettingsService.AppAccentColor);
                MenuSettingsButton.IsEnabled = false;
            }

            if (App.IsGameBarWidget)
            {
                MenuFullScreenSeparator.Visibility = Visibility.Collapsed;
                MenuPrintSeparator.Visibility = Visibility.Collapsed;
                MenuSettingsSeparator.Visibility = Visibility.Collapsed;

                MenuCompactOverlayButton.Visibility = Visibility.Collapsed;
                MenuFullScreenButton.Visibility = Visibility.Collapsed;
                MenuPrintButton.Visibility = Visibility.Collapsed;
                MenuPrintAllButton.Visibility = Visibility.Collapsed;
                MenuSettingsButton.Visibility = Visibility.Collapsed;
            }

            if (!PrintManager.IsSupported())
            {
                MenuPrintButton.Visibility = Visibility.Collapsed;
                MenuPrintAllButton.Visibility = Visibility.Collapsed;
                MenuPrintSeparator.Visibility = Visibility.Collapsed;
            }

            MainMenuButtonFlyout.Opening += MainMenuButtonFlyout_Opening;
        }

        private void MainMenuButtonFlyout_Opening(object sender, object e)
        {
            var selectedTextEditor = NotepadsCore.GetSelectedTextEditor();

            if (selectedTextEditor == null)
            {
                MenuSaveButton.IsEnabled = false;
                MenuSaveAsButton.IsEnabled = false;
                MenuFindButton.IsEnabled = false;
                MenuReplaceButton.IsEnabled = false;
                MenuPrintButton.IsEnabled = false;
                MenuPrintAllButton.IsEnabled = false;
            }
            else if (selectedTextEditor.IsEditorEnabled() == false)
            {
                MenuSaveButton.IsEnabled = selectedTextEditor.IsModified;
                MenuSaveAsButton.IsEnabled = true;
                MenuFindButton.IsEnabled = false;
                MenuReplaceButton.IsEnabled = false;
            }
            else
            {
                MenuSaveButton.IsEnabled = selectedTextEditor.IsModified;
                MenuSaveAsButton.IsEnabled = true;
                MenuFindButton.IsEnabled = true;
                MenuReplaceButton.IsEnabled = true;

                if (PrintManager.IsSupported())
                {
                    MenuPrintButton.IsEnabled = !string.IsNullOrEmpty(selectedTextEditor.GetText());
                    MenuPrintAllButton.IsEnabled = NotepadsCore.HaveNonemptyTextEditor();
                }
            }

            MenuFullScreenButton.Text = _resourceLoader.GetString(
                ApplicationView.GetForCurrentView().IsFullScreenMode
                    ? "App_ExitFullScreenMode_Text"
                    : "App_EnterFullScreenMode_Text");
            MenuCompactOverlayButton.Text = _resourceLoader.GetString(
                ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.CompactOverlay
                    ? "App_ExitCompactOverlayMode_Text"
                    : "App_EnterCompactOverlayMode_Text");
            MenuSaveAllButton.IsEnabled = NotepadsCore.HaveUnsavedTextEditor();
        }

        private async Task BuildOpenRecentButtonSubItemsAsync()
        {
            var openRecentSubItem = new MenuFlyoutSubItem
            {
                Text = _resourceLoader.GetString("MainMenu_Button_Open_Recent/Text"),
                Icon = new FontIcon { Glyph = "\xE81C" },
                Name = "MenuOpenRecentlyUsedFileButton",
            };

            var MRUFileList = new HashSet<string>();

            foreach (var item in await MRUService.GetMostRecentlyUsedListAsync(top: 10))
            {
                if (item is StorageFile file)
                {
                    if (MRUFileList.Contains(file.Path))
                    {
                        // MRU might contains files with same path (User opens a recently used file after renaming it)
                        // So we need to do the decouple here
                        continue;
                    }
                    var newItem = new MenuFlyoutItem()
                    {
                        Text = file.Path
                    };
                    ToolTipService.SetToolTip(newItem, file.Path);
                    newItem.Click += async (sender, args) => { await OpenFileAsync(file); };
                    openRecentSubItem.Items?.Add(newItem);
                    MRUFileList.Add(file.Path);
                }
            }

            var oldOpenRecentSubItem = MainMenuButtonFlyout.Items?.FirstOrDefault(i => i.Name == openRecentSubItem.Name);
            if (oldOpenRecentSubItem != null)
            {
                MainMenuButtonFlyout.Items.Remove(oldOpenRecentSubItem);
            }

            openRecentSubItem.IsEnabled = false;
            if (openRecentSubItem.Items?.Count > 0)
            {
                openRecentSubItem.Items?.Add(new MenuFlyoutSeparator());

                var clearRecentlyOpenedSubItem = new MenuFlyoutItem()
                {
                    Text = _resourceLoader.GetString("MainMenu_Button_Open_Recent_ClearRecentlyOpenedSubItem_Text")
                };

                clearRecentlyOpenedSubItem.Click += async (sender, args) =>
                {
                    MRUService.ClearAll();
                    await BuildOpenRecentButtonSubItemsAsync();
                };
                openRecentSubItem.Items?.Add(clearRecentlyOpenedSubItem);
                openRecentSubItem.IsEnabled = true;
            }

            if (MainMenuButtonFlyout.Items != null)
            {
                var indexToInsert = MainMenuButtonFlyout.Items.IndexOf(MenuOpenFileButton) + 1;
                MainMenuButtonFlyout.Items.Insert(indexToInsert, openRecentSubItem);
            }
        }
    }
}