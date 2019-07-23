﻿
namespace Notepads.Core
{
    using System;
    using System.IO;
    using Notepads.Controls.TextEditor;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.ApplicationModel.Resources;
    using Windows.System;
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

        private readonly INotepadsCore _notepadsCore;
        private readonly TextEditor _textEditor;

        private readonly ResourceLoader _resourceLoader = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView();

        public TabContextFlyout(INotepadsCore notepadsCore, TextEditor textEditor)
        {
            _notepadsCore = notepadsCore;
            _textEditor = textEditor;

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
            if (_textEditor.EditingFile != null)
            {
                _filePath = _textEditor.EditingFile.Path;
                _containingFolderPath = Path.GetDirectoryName(_filePath);
            }

            CloseOthers.IsEnabled = CloseRight.IsEnabled = _notepadsCore.GetNumberOfOpenedTextEditors() > 1;
            CopyFullPath.IsEnabled = !string.IsNullOrEmpty(_filePath);
            OpenContainingFolder.IsEnabled = !string.IsNullOrEmpty(_containingFolderPath);
        }

        private void TabContextFlyout_Closed(object sender, object e)
        {
            _notepadsCore.FocusOnSelectedTextEditor();
        }

        private MenuFlyoutItem Close
        {
            get
            {
                if (_close == null)
                {
                    _close = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_CloseButtonDisplayText") };
                    _close.Click += (sender, args) => { _notepadsCore.CloseTextEditor(_textEditor); };
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
                        ExecuteOnAllTextEditors(
                            (textEditor) =>
                            {
                                if (textEditor != _textEditor)
                                {
                                    _notepadsCore.CloseTextEditor(textEditor);
                                }
                            });
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

                        ExecuteOnAllTextEditors(
                            (textEditor) =>
                            {
                                if (textEditor == _textEditor)
                                {
                                    close = true;
                                }
                                else if (close)
                                {
                                    _notepadsCore.CloseTextEditor(textEditor);
                                }
                            });
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
                        ExecuteOnAllTextEditors(
                            (textEditor) =>
                            {
                                if (textEditor.Saved)
                                {
                                    _notepadsCore.CloseTextEditor(textEditor);
                                }
                            });
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

        private void ExecuteOnAllTextEditors(Action<TextEditor> action)
        {
            foreach (TextEditor textEditor in _notepadsCore.GetAllTextEditors())
            {
                action(textEditor);
            }
        }
    }
}
