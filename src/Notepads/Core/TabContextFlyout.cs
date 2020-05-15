namespace Notepads.Core
{
    using System;
    using System.IO;
    using System.Threading.Tasks;
    using Notepads.Controls.Dialog;
    using Notepads.Controls.TextEditor;
    using Notepads.Services;
    using Notepads.Utilities;
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
        private MenuFlyoutItem _rename;

        private string _filePath;
        private string _containingFolderPath;

        private readonly INotepadsCore _notepadsCore;
        private readonly ITextEditor _textEditor;

        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        public TabContextFlyout(INotepadsCore notepadsCore, ITextEditor textEditor)
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
            Items.Add(new MenuFlyoutSeparator());
            Items.Add(Rename);

            var style = new Style(typeof(MenuFlyoutPresenter));
            style.Setters.Add(new Setter(Control.BorderThicknessProperty, 0));
            MenuFlyoutPresenterStyle = style;

            Opening += TabContextFlyout_Opening;
            Closed += TabContextFlyout_Closed;
        }

        public void Dispose()
        {
            Opening -= TabContextFlyout_Opening;
            Closed -= TabContextFlyout_Closed;
        }

        private void TabContextFlyout_Opening(object sender, object e)
        {
            var isFileReadonly = false;
            if (_textEditor.EditingFile != null)
            {
                _filePath = _textEditor.EditingFile.Path;
                _containingFolderPath = Path.GetDirectoryName(_filePath);
                isFileReadonly = FileSystemUtility.IsFileReadOnly(_textEditor.EditingFile);
            }

            CloseOthers.IsEnabled = CloseRight.IsEnabled = _notepadsCore.GetNumberOfOpenedTextEditors() > 1;
            CopyFullPath.IsEnabled = !string.IsNullOrEmpty(_filePath);
            OpenContainingFolder.IsEnabled = !string.IsNullOrEmpty(_containingFolderPath);
            Rename.IsEnabled = _textEditor.FileModificationState != FileModificationState.RenamedMovedOrDeleted && !isFileReadonly;

            if (App.IsGameBarWidget)
            {
                OpenContainingFolder.Visibility = Visibility.Collapsed;
            }
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
                                if (!textEditor.IsModified)
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
                            Clipboard.SetContentWithOptions(dataPackage, new ClipboardContentOptions() { IsAllowedInHistory = true, IsRoamable = true });
                            Clipboard.Flush(); // This method allows the content to remain available after the application shuts down.
                            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileNameOrPathCopied"), 1500);
                        }
                        catch (Exception ex)
                        {
                            LoggingService.LogError($"[{nameof(TabContextFlyout)}] Failed to copy full path: {ex.Message}");
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
                        try
                        {
                            await Launcher.LaunchFolderPathAsync(_containingFolderPath);
                        }
                        catch (Exception ex)
                        {
                            LoggingService.LogError($"[{nameof(TabContextFlyout)}] Failed to open containing folder: {ex.Message}");
                        }
                    };
                }
                return _openContainingFolder;
            }
        }

        private MenuFlyoutItem Rename
        {
            get
            {
                if (_rename == null)
                {
                    _rename = new MenuFlyoutItem() { Text = _resourceLoader.GetString("Tab_ContextFlyout_RenameButtonDisplayText") };
                    _rename.KeyboardAccelerators.Add(new KeyboardAccelerator()
                    {
                        Key = VirtualKey.F2,
                        IsEnabled = false,
                    });
                    _rename.Click += async (sender, args) =>
                    {
                        _notepadsCore.SwitchTo(_textEditor);

                        await Task.Delay(10); // Give notepads core enough time to switch to the selected editor

                        var fileRenameDialog = new FileRenameDialog(_textEditor.EditingFileName ?? _textEditor.FileNamePlaceholder,
                            fileExists: _textEditor.EditingFile != null,
                            confirmedAction: async (newFilename) =>
                            {
                                try
                                {
                                    await _textEditor.RenameAsync(newFilename);
                                    _notepadsCore.FocusOnSelectedTextEditor();
                                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileRenamed"), 1500);
                                }
                                catch (Exception ex)
                                {
                                    var errorMessage = ex.Message?.TrimEnd('\r', '\n');
                                    NotificationCenter.Instance.PostNotification(errorMessage, 3500); // TODO: Use Content Dialog to display error message
                                }
                            });
                        await DialogManager.OpenDialogAsync(fileRenameDialog, awaitPreviousDialog: false);
                    };
                }
                return _rename;
            }
        }

        private void ExecuteOnAllTextEditors(Action<ITextEditor> action)
        {
            foreach (ITextEditor textEditor in _notepadsCore.GetAllTextEditors())
            {
                action(textEditor);
            }
        }
    }
}