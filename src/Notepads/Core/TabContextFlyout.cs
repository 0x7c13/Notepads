
namespace Notepads.Core
{
    using Notepads.Controls.TextEditor;
    using System;
    using System.Collections.Generic;
    using System.IO;
    using System.Linq;
    using SetsView;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.ApplicationModel.Resources;
    using Windows.System;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;

    public class TabContextFlyout : MenuFlyout
    {
        private MenuFlyoutItem _close;
        private MenuFlyoutItem _closeOthers;
        private MenuFlyoutItem _closeRight;
        private MenuFlyoutItem _closeSaved;
        private MenuFlyoutItem _copyFullPath;
        private MenuFlyoutItem _openContainingFolder;

        private string _filePath;
        private string _containingFolderPath;

        private readonly SetsView _tabs;
        private readonly SetsViewItem _tab;

        private readonly ResourceLoader _resourceLoader = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView();

        public TabContextFlyout(SetsView tabs, SetsViewItem tab)
        {
            _tabs = tabs;
            _tab = tab;

            Items.Add(Close);
            Items.Add(CloseOthers);
            Items.Add(CloseRight);
            Items.Add(CloseSaved);
            Items.Add(new MenuFlyoutSeparator());
            Items.Add(CopyFullPath);
            Items.Add(OpenContainingFolder);

            Opening += TabContextFlyout_Opening;
            Closed += TabContextFlyout_Closed;
        }

        private void TabContextFlyout_Opening(object sender, object e)
        {
            if (_tab.Content is TextEditor textEditor && textEditor.EditingFile != null)
            {
                _filePath = textEditor.EditingFile.Path;
                _containingFolderPath = Path.GetDirectoryName(_filePath);
            }

            CloseOthers.IsEnabled = CloseRight.IsEnabled = _tabs.Items?.Count > 1;
            CopyFullPath.IsEnabled = !string.IsNullOrEmpty(_filePath);
            OpenContainingFolder.IsEnabled = !string.IsNullOrEmpty(_containingFolderPath);
        }

        private void TabContextFlyout_Closed(object sender, object e)
        {
            if (((SetsViewItem) _tabs.SelectedItem)?.Content is TextEditor textEditor)
            {
                textEditor.Focus(FocusState.Programmatic);
            }
        }

        private IList<SetsViewItem> TabList
        {
            get
            {
                if (_tabs.Items == null)
                {
                    return Array.Empty<SetsViewItem>();
                }
                return _tabs.Items.Cast<SetsViewItem>().ToList();
            }
        }

        private MenuFlyoutItem Close
        {
            get
            {
                if (_close == null)
                {
                    _close = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_CloseButtonDisplayText") };
                    _close.Click += (sender, args) => { _tab.Close(); };
                    _close.KeyboardAccelerators.Add(new KeyboardAccelerator()
                    {
                        Modifiers = VirtualKeyModifiers.Control,
                        Key = VirtualKey.W,
                        IsEnabled = false,
                    });
                }

                return _close;
            }
        }

        private MenuFlyoutItem CloseOthers
        {
            get
            {
                if (_closeOthers == null)
                {
                    _closeOthers = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_CloseOthersButtonDisplayText") };
                    _closeOthers.Click += (sender, args) =>
                    {
                        foreach (SetsViewItem tab in TabList)
                        {
                            if (tab != _tab)
                            {
                                tab.Close();
                            }
                        }
                    };
                }
                return _closeOthers;
            }
        }

        private MenuFlyoutItem CloseRight
        {
            get
            {
                if (_closeRight == null)
                {
                    _closeRight = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_CloseRightButtonDisplayText") };
                    _closeRight.Click += (sender, args) =>
                    {
                        bool close = false;

                        foreach (SetsViewItem tab in TabList)
                        {
                            if (tab == _tab)
                            {
                                close = true;
                            }
                            else if (close)
                            {
                                tab.Close();
                            }
                        }
                    };
                }
                return _closeRight;
            }
        }

        private MenuFlyoutItem CloseSaved
        {
            get
            {
                if (_closeSaved == null)
                {
                    _closeSaved = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_CloseSavedButtonDisplayText") };
                    _closeSaved.Click += (sender, args) =>
                    {
                        foreach (SetsViewItem tab in TabList)
                        {
                            if (tab.Content is TextEditor textEditor && textEditor.Saved)
                            {
                                tab.Close();
                            }
                        }
                    };
                }
                return _closeSaved;
            }
        }

        private MenuFlyoutItem CopyFullPath
        {
            get
            {
                if (_copyFullPath == null)
                {
                    _copyFullPath = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_CopyFullPathButtonDisplayText") };
                    _copyFullPath.Click += (sender, args) =>
                    {
                        try
                        {
                            DataPackage dataPackage = new DataPackage { RequestedOperation = DataPackageOperation.Copy };
                            dataPackage.SetText(_filePath);
                            Clipboard.SetContent(dataPackage);
                        }
                        catch (Exception)
                        {
                            // Ignore
                        }
                    };
                }
                return _copyFullPath;
            }
        }

        private MenuFlyoutItem OpenContainingFolder
        {
            get
            {
                if (_openContainingFolder == null)
                {
                    _openContainingFolder = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_OpenContainingFolderButtonDisplayText") };
                    _openContainingFolder.Click += async (sender, args) =>
                    {
                        await Launcher.LaunchFolderPathAsync(_containingFolderPath);
                    };
                }
                return _openContainingFolder;
            }
        }
    }
}
