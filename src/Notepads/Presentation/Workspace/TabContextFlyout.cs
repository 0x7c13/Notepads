// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;
using Notepads.Presentation.Controls.TextEditor;
using Windows.ApplicationModel.DataTransfer;
using Windows.ApplicationModel.Resources;
using Windows.System;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;

namespace Notepads.Presentation.Workspace;

public sealed partial class TabContextFlyout : MenuFlyout
{
    private MenuFlyoutItem _close;
    private MenuFlyoutItem _closeOthers;
    private MenuFlyoutItem _closeRight;
    private MenuFlyoutItem _closeSaved;
    private MenuFlyoutItem _copyFullPath;
    private MenuFlyoutItem _openContainingFolder;
    private MenuFlyoutItem _rename;
    private MenuFlyoutItem _moveToNewWindow;

    private string _filePath;
    private string _containingFolderPath;

    private readonly INotepadsCore _notepadsCore;
    private readonly WorkspaceTab _tab;
    private readonly Func<ITextEditor, Task> _renameAsync;

    private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

    public TabContextFlyout(INotepadsCore notepadsCore, WorkspaceTab tab, Func<ITextEditor, Task> renameAsync)
    {
        _notepadsCore = notepadsCore;
        _tab = tab;
        _renameAsync = renameAsync;

        Items.Add(Close);
        Items.Add(CloseOthers);
        Items.Add(CloseRight);
        Items.Add(CloseSaved);
        Items.Add(new MenuFlyoutSeparator());
        Items.Add(CopyFullPath);
        Items.Add(OpenContainingFolder);
        Items.Add(new MenuFlyoutSeparator());
        Items.Add(Rename);
        Items.Add(MoveToNewWindow);

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
        var textEditor = _tab.ReadyEditor;
        _filePath = _tab.EditingFilePath;
        _containingFolderPath = string.IsNullOrEmpty(_filePath) ? null : Path.GetDirectoryName(_filePath);
        var isFileReadonly = false;
        if (textEditor?.EditingFile != null)
        {
            _filePath = textEditor.EditingFile.Path;
            _containingFolderPath = Path.GetDirectoryName(_filePath);
            isFileReadonly = FileStorage.IsFileReadOnly(textEditor.EditingFile);
        }

        CloseOthers.IsEnabled = CloseRight.IsEnabled = _notepadsCore.GetTabCount() > 1;
        CopyFullPath.IsEnabled = !string.IsNullOrEmpty(_filePath);
        OpenContainingFolder.IsEnabled = !string.IsNullOrEmpty(_containingFolderPath);
        Rename.IsEnabled = textEditor != null && textEditor.FileModificationState != FileModificationState.RenamedMovedOrDeleted && !isFileReadonly;
        MoveToNewWindow.IsEnabled = _notepadsCore.CanMoveTabToNewWindow(_tab);
    }

    private MenuFlyoutItem MoveToNewWindow
    {
        get
        {
            if (_moveToNewWindow != null) return _moveToNewWindow;
            _moveToNewWindow = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_MoveToNewWindowDisplayText") };
            _moveToNewWindow.Click += async (_, _) => await _notepadsCore.MoveTabToNewWindowAsync(_tab);
            return _moveToNewWindow;
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
            if (_close != null) return _close;

            _close = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_CloseButtonDisplayText") };
            _close.Click += (sender, args) => { _notepadsCore.CloseTab(_tab); };
            _close.KeyboardAccelerators.Add(new KeyboardAccelerator()
            {
                Modifiers = VirtualKeyModifiers.Control,
                Key = VirtualKey.W,
                IsEnabled = false,
            });
            return _close;
        }
    }

    private MenuFlyoutItem CloseOthers
    {
        get
        {
            if (_closeOthers != null) return _closeOthers;

            _closeOthers = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_CloseOthersButtonDisplayText") };
            _closeOthers.Click += (sender, args) =>
            {
                ExecuteOnAllTabs(
                    (tab) =>
                    {
                        if (tab != _tab)
                        {
                            _notepadsCore.CloseTab(tab);
                        }
                    });
            };
            return _closeOthers;
        }
    }

    private MenuFlyoutItem CloseRight
    {
        get
        {
            if (_closeRight != null) return _closeRight;

            _closeRight = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_CloseRightButtonDisplayText") };
            _closeRight.Click += (sender, args) =>
            {
                bool close = false;

                ExecuteOnAllTabs(
                    (tab) =>
                    {
                        if (tab == _tab)
                        {
                            close = true;
                        }
                        else if (close)
                        {
                            _notepadsCore.CloseTab(tab);
                        }
                    });
            };
            return _closeRight;
        }
    }

    private MenuFlyoutItem CloseSaved
    {
        get
        {
            if (_closeSaved != null) return _closeSaved;

            _closeSaved = new MenuFlyoutItem { Text = _resourceLoader.GetString("Tab_ContextFlyout_CloseSavedButtonDisplayText") };
            _closeSaved.Click += (sender, args) =>
            {
                ExecuteOnAllTabs(
                    (tab) =>
                    {
                        if (tab.ReadyEditor is ITextEditor editor && !editor.IsModified)
                        {
                            _notepadsCore.CloseTab(tab);
                        }
                    });
            };
            return _closeSaved;
        }
    }

    private MenuFlyoutItem CopyFullPath
    {
        get
        {
            if (_copyFullPath != null) return _copyFullPath;

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
            return _copyFullPath;
        }
    }

    private MenuFlyoutItem OpenContainingFolder
    {
        get
        {
            if (_openContainingFolder != null) return _openContainingFolder;

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
            return _openContainingFolder;
        }
    }

    private MenuFlyoutItem Rename
    {
        get
        {
            if (_rename != null) return _rename;

            _rename = new MenuFlyoutItem() { Text = _resourceLoader.GetString("Tab_ContextFlyout_RenameButtonDisplayText") };
            _rename.KeyboardAccelerators.Add(new KeyboardAccelerator()
            {
                Key = VirtualKey.F2,
                IsEnabled = false,
            });
            _rename.Click += async (sender, args) =>
            {
                _notepadsCore.SwitchTo(_tab);
                if (_tab.ReadyEditor is ITextEditor editor) await _renameAsync(editor);
            };
            return _rename;
        }
    }

    private void ExecuteOnAllTabs(Action<WorkspaceTab> action)
    {
        foreach (var tab in _notepadsCore.GetTabs())
        {
            action(tab);
        }
    }
}
