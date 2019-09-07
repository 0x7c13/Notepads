﻿namespace Notepads
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.Storage;
    using Notepads.Controls.TextEditor;
    using Notepads.Services;
    using Notepads.Utilities;

    public sealed partial class NotepadsMainPage
    {
        private async Task OpenNewFiles()
        {
            IReadOnlyList<StorageFile> files;

            try
            {
                files = await FilePickerFactory.GetFileOpenPicker().PickMultipleFilesAsync();
            }
            catch (Exception ex)
            {
                var fileOpenErrorDialog = ContentDialogFactory.GetFileOpenErrorDialog(filePath: null, ex.Message);
                await ContentDialogMaker.CreateContentDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
                NotepadsCore.FocusOnSelectedTextEditor();
                return;
            }

            if (files == null || files.Count == 0)
            {
                NotepadsCore.FocusOnSelectedTextEditor();
                return;
            }

            foreach (var file in files)
            {
                await OpenFile(file);
            }
        }

        public async Task<bool> OpenFile(StorageFile file)
        {
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

                var editor = await NotepadsCore.CreateTextEditor(Guid.NewGuid(), file);
                NotepadsCore.OpenTextEditor(editor);
                NotepadsCore.FocusOnSelectedTextEditor();
                return true;
            }
            catch (Exception ex)
            {
                var fileOpenErrorDialog = ContentDialogFactory.GetFileOpenErrorDialog(file.Path, ex.Message);
                await ContentDialogMaker.CreateContentDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
                NotepadsCore.FocusOnSelectedTextEditor();
                return false;
            }
        }

        public async Task<int> OpenFiles(IReadOnlyList<IStorageItem> storageItems)
        {
            if (storageItems == null || storageItems.Count == 0) return 0;
            int successCount = 0;
            foreach (var storageItem in storageItems)
            {
                if (storageItem is StorageFile file)
                {
                    if (await OpenFile(file))
                    {
                        successCount++;
                    }
                }
            }

            return successCount;
        }

        private async Task<bool> Save(ITextEditor textEditor, bool saveAs, bool ignoreUnmodifiedDocument = false)
        {
            if (textEditor == null) return false;

            if (ignoreUnmodifiedDocument && !textEditor.IsModified)
            {
                return true;
            }

            StorageFile file = null;
            try
            {
                if (textEditor.EditingFile == null || saveAs ||
                    FileSystemUtility.IsFileReadOnly(textEditor.EditingFile) ||
                    !await FileSystemUtility.FileIsWritable(textEditor.EditingFile))
                {
                    NotepadsCore.SwitchTo(textEditor);
                    file = await FilePickerFactory.GetFileSavePicker(textEditor, _defaultNewFileName, saveAs)
                        .PickSaveFileAsync();
                    NotepadsCore.FocusOnTextEditor(textEditor);
                    if (file == null)
                    {
                        return false; // User cancelled
                    }
                }
                else
                {
                    file = textEditor.EditingFile;
                }

                await NotepadsCore.SaveContentToFileAndUpdateEditorState(textEditor, file);
                return true;
            }
            catch (Exception ex)
            {
                var fileSaveErrorDialog =
                    ContentDialogFactory.GetFileSaveErrorDialog((file == null) ? string.Empty : file.Path, ex.Message);
                await ContentDialogMaker.CreateContentDialogAsync(fileSaveErrorDialog, awaitPreviousDialog: false);
                NotepadsCore.FocusOnSelectedTextEditor();
                return false;
            }
        }
    }
}