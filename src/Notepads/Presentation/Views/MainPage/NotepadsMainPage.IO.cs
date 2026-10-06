// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Notepads.Features.Documents.FileTypes;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Shell;
using Notepads.Infrastructure.Storage;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.FilePicker;
using Notepads.Presentation.Controls.Print;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace;
using Windows.Graphics.Printing;
using Windows.Storage;

namespace Notepads.Presentation.Views.MainPage;

public sealed partial class NotepadsMainPage
{
    private async Task CreateNewTextEditorAsync()
    {
        if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing) return;
        ITextEditor editor = null;
        try
        {
            editor = await NotepadsCore.CreateNewTextEditorAsync(_defaultNewFileName);
            if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing) { editor.Dispose(); return; }
            NotepadsCore.OpenTextEditor(editor);
        }
        catch (Exception ex)
        {
            editor?.Dispose();
            if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing) return;
            var errorDialog = new FileOpenErrorDialog(filePath: null, ex.Message);
            await DialogManager.OpenDialogAsync(errorDialog, awaitPreviousDialog: false);
        }
    }

    private async Task OpenNewFilesAsync()
    {
        if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing) return;
        IReadOnlyList<StorageFile> files;

        try
        {
            files = await FilePickerFactory.GetFileOpenPicker().PickMultipleFilesAsync();
        }
        catch (Exception ex)
        {
            if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing) return;
            var fileOpenErrorDialog = new FileOpenErrorDialog(filePath: null, ex.Message);
            await DialogManager.OpenDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
            if (!fileOpenErrorDialog.IsAborted)
            {
                NotepadsCore.FocusOnSelectedTextEditor();
            }
            return;
        }

        if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing) return;
        if (files == null || files.Count == 0)
        {
            NotepadsCore.FocusOnSelectedTextEditor();
            return;
        }

        foreach (var file in files)
        {
            await OpenFileAsync(file);
        }
    }

    public async Task<bool> OpenFileAsync(StorageFile file, bool rebuildOpenRecentItems = true)
    {
        if (_viewClosed || _isAppClosing || file == null || NotepadsCore.IsClosing) return false;
        ITextEditor editor = null;
        try
        {
            var openedEditor = NotepadsCore.GetTextEditor(file);
            if (openedEditor != null)
            {
                NotepadsCore.SwitchTo(openedEditor);
                NotificationCenter.Instance.PostNotification(
                    _resourceLoader.GetString("TextEditor_NotificationMsg_FileAlreadyOpened"), 2500);
                return false;
            }

            editor = await NotepadsCore.CreateTextEditorAsync(Guid.NewGuid(), file);
            if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing) { editor.Dispose(); return false; }
            // Another activation or drop can finish opening the same file
            // while this candidate is decoding its retained source stream.
            openedEditor = NotepadsCore.GetTextEditor(file);
            if (openedEditor != null)
            {
                editor.Dispose();
                NotepadsCore.SwitchTo(openedEditor);
                return false;
            }
            NotepadsCore.OpenTextEditor(editor);
            NotepadsCore.FocusOnSelectedTextEditor();
            var success = MRUService.TryAdd(file); // Remember recently used files
            if (success && rebuildOpenRecentItems)
            {
                await BuildOpenRecentButtonSubItemsAsync();
            }

            TrackFileExtensionIfNotSupported(file);

            return true;
        }
        catch (Exception ex)
        {
            if (editor != null && !NotepadsCore.GetAllTextEditors().Contains(editor)) editor.Dispose();
            if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing) return false;
            var fileOpenErrorDialog = new FileOpenErrorDialog(file.Path, ex.Message);
            await DialogManager.OpenDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
            if (!fileOpenErrorDialog.IsAborted)
            {
                NotepadsCore.FocusOnSelectedTextEditor();
            }
            return false;
        }
    }

    // Here we track the file extension opened by user but not supported by Notepads.
    // This information will be used to on-board new extension support for future release.
    // Because UWP does not allow user to associate arbitrary file extension with the app.
    // File name will not and should not be tracked.
    private void TrackFileExtensionIfNotSupported(StorageFile file)
    {
        try
        {
            var extension = FileTypeUtility.GetFileExtension(file.Name).ToLower();
            if (!FileExtensionProvider.AllSupportedFileExtensions.Contains(extension))
            {
                if (string.IsNullOrEmpty(extension))
                {
                    extension = "<NoExtension>";
                }
                AnalyticsService.TrackEvent("UnsupportedFileExtension", new Dictionary<string, string>()
                {
                    { "Extension", extension },
                });
            }
        }
        catch (Exception)
        {
            // ignore
        }
    }

    public async Task<int> OpenFilesAsync(IReadOnlyList<IStorageItem> storageItems)
    {
        if (storageItems == null || storageItems.Count == 0) return 0;
        int successCount = 0;
        foreach (var storageItem in storageItems)
        {
            if (storageItem is StorageFile file)
            {
                if (await OpenFileAsync(file, rebuildOpenRecentItems: false))
                {
                    successCount++;
                }
            }
        }
        if (successCount > 0)
        {
            await BuildOpenRecentButtonSubItemsAsync();
        }
        return successCount;
    }

    private async Task<StorageFile> OpenFileUsingFileSavePickerAsync(ITextEditor textEditor)
    {
        NotepadsCore.SwitchTo(textEditor);
        StorageFile file = await FilePickerFactory.GetFileSavePicker(textEditor).PickSaveFileAsync();
        if (!_viewClosed && !_isAppClosing && NotepadsCore.GetAllTextEditors().Contains(textEditor))
            NotepadsCore.FocusOnTextEditor(textEditor);
        return file;
    }

    private async Task SaveInternalAsync(ITextEditor textEditor, StorageFile file, bool rebuildOpenRecentItems)
    {
        await NotepadsCore.SaveContentToFileAndUpdateEditorStateAsync(textEditor, file);
        var success = MRUService.TryAdd(file); // Remember recently used files
        if (success && rebuildOpenRecentItems)
        {
            await BuildOpenRecentButtonSubItemsAsync();
        }
    }

    private async Task<bool> SaveAsync(ITextEditor textEditor, bool saveAs, bool ignoreUnmodifiedDocument = false,
        bool rebuildOpenRecentItems = true, bool allowWhileClosing = false)
    {
        if (!CanSaveEditor(textEditor, allowWhileClosing)) return false;

        if (ignoreUnmodifiedDocument && !textEditor.IsModified)
        {
            return true;
        }

        StorageFile file = null;

        try
        {
            if (textEditor.EditingFile == null || saveAs)
            {
                file = await OpenFileUsingFileSavePickerAsync(textEditor);
                if (file == null || !CanSaveEditor(textEditor, allowWhileClosing)) return false;
            }
            else
            {
                file = textEditor.EditingFile;
            }

            bool promptSaveAs = false;
            try
            {
                await SaveInternalAsync(textEditor, file, rebuildOpenRecentItems);
            }
            catch (UnauthorizedAccessException) // Happens when the file we are saving is read-only
            {
                promptSaveAs = true;
            }
            catch (FileNotFoundException) // Happens when the file not found or storage media is removed
            {
                promptSaveAs = true;
            }

            if (promptSaveAs)
            {
                if (!CanSaveEditor(textEditor, allowWhileClosing)) return false;
                file = await OpenFileUsingFileSavePickerAsync(textEditor);
                if (file == null || !CanSaveEditor(textEditor, allowWhileClosing)) return false;

                await SaveInternalAsync(textEditor, file, rebuildOpenRecentItems);
                return true;
            }

            return true;
        }
        catch (OperationCanceledException) when (!NotepadsCore.GetAllTextEditors().Contains(textEditor))
        {
            return false;
        }
        catch (ObjectDisposedException) when (!NotepadsCore.GetAllTextEditors().Contains(textEditor))
        {
            return false;
        }
        catch (Exception ex)
        {
            if (_viewClosed) return false;
            var fileSaveErrorDialog = new FileSaveErrorDialog((file == null) ? string.Empty : file.Path, ex.Message);
            await DialogManager.OpenDialogAsync(fileSaveErrorDialog, awaitPreviousDialog: false);
            if (!fileSaveErrorDialog.IsAborted)
            {
                NotepadsCore.FocusOnSelectedTextEditor();
            }
            return false;
        }
    }

    private bool CanSaveEditor(ITextEditor textEditor, bool allowWhileClosing) =>
        !_viewClosed && textEditor != null && (!_isAppClosing || allowWhileClosing) &&
        NotepadsCore.GetAllTextEditors().Contains(textEditor);

    private async Task<bool> SaveAllAsync(ITextEditor[] textEditors)
    {
        var success = false;

        foreach (var textEditor in textEditors)
        {
            if (await SaveAsync(textEditor, saveAs: false, ignoreUnmodifiedDocument: true, rebuildOpenRecentItems: false)) success = true;
        }

        if (success)
        {
            await BuildOpenRecentButtonSubItemsAsync();
        }

        return success;
    }

    private async Task RenameFileAsync(ITextEditor textEditor)
    {
        if (_viewClosed || _isAppClosing || textEditor == null) return;

        if (textEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted) return;

        if (textEditor.EditingFile != null && FileStorage.IsFileReadOnly(textEditor.EditingFile)) return;

        var expectedFile = textEditor.EditingFile;
        string newFileName = null;
        var fileRenameDialog = new FileRenameDialog(textEditor.EditingFileName ?? textEditor.FileNamePlaceholder,
            fileExists: textEditor.EditingFile != null,
            confirmedAction: filename => { newFileName = filename; });

        var result = await DialogManager.OpenDialogAsync(fileRenameDialog, awaitPreviousDialog: false);
        if (result == null || fileRenameDialog.IsAborted || newFileName == null || _isAppClosing ||
            !NotepadsCore.GetAllTextEditors().Contains(textEditor))
        {
            return;
        }

        try
        {
            await textEditor.RenameAsync(newFileName, expectedFile);
            if (!NotepadsCore.GetAllTextEditors().Contains(textEditor)) return;
            NotepadsCore.FocusOnSelectedTextEditor();
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileRenamed"), 1500);
        }
        catch (OperationCanceledException) { }
        catch (ObjectDisposedException) when (!NotepadsCore.GetAllTextEditors().Contains(textEditor)) { }
        catch (Exception ex)
        {
            if (!NotepadsCore.GetAllTextEditors().Contains(textEditor)) return;
            var errorMessage = ex.Message?.TrimEnd('\r', '\n');
            NotificationCenter.Instance.PostNotification(errorMessage, 3500);
        }
    }

    public async Task PrintAsync(ITextEditor textEditor)
    {
        if (textEditor == null) return;
        await PrintAllAsync([textEditor]);
    }

    public async Task PrintAllAsync(ITextEditor[] textEditors)
    {
        if (textEditors == null || textEditors.Length == 0) return;

        // Initialize print content
        PrintArgs.PreparePrintContent(textEditors);

        if (PrintManager.IsSupported() && HaveNonemptyTextEditor(textEditors))
        {
            // Show print UI
            await PrintArgs.ShowPrintUIAsync();
        }
        else if (!PrintManager.IsSupported())
        {
            // Printing is not supported on this device
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("Print_NotificationMsg_PrintNotSupported"), 1500);
        }
    }

    private static bool HaveNonemptyTextEditor(ITextEditor[] textEditors)
    {
        foreach (ITextEditor textEditor in textEditors)
        {
            if (textEditor.IsDocumentEmpty) continue;
            return true;
        }
        return false;
    }
}
