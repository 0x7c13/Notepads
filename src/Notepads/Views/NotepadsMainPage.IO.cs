namespace Notepads.Views
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.Graphics.Printing;
    using Windows.Storage;
    using Notepads.Controls.Dialog;
    using Notepads.Controls.FilePicker;
    using Notepads.Controls.Print;
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
                var fileOpenPicker = FilePickerFactory.GetFileOpenPicker();
                foreach (var type in FileTypeService.AllSupportedFileExtensions)
                {
                    fileOpenPicker.FileTypeFilter.Add(type);
                }

                files = await fileOpenPicker.PickMultipleFilesAsync();
            }
            catch (Exception ex)
            {
                var fileOpenErrorDialog = NotepadsDialogFactory.GetFileOpenErrorDialog(filePath: null, ex.Message);
                await DialogManager.OpenDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
                if (!fileOpenErrorDialog.IsAborted)
                {
                    NotepadsCore.FocusOnSelectedTextEditor();
                }
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

        public async Task<bool> OpenFile(StorageFile file, bool rebuildOpenRecentItems = true)
        {
            try
            {
                if (file == null) return false;
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
                var success = MRUService.TryAdd(file); // Remember recently used files
                if (success && rebuildOpenRecentItems)
                {
                    await BuildOpenRecentButtonSubItems();
                }
                return true;
            }
            catch (Exception ex)
            {
                var fileOpenErrorDialog = NotepadsDialogFactory.GetFileOpenErrorDialog(file.Path, ex.Message);
                await DialogManager.OpenDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
                if (!fileOpenErrorDialog.IsAborted)
                {
                    NotepadsCore.FocusOnSelectedTextEditor();
                }
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
                    if (await OpenFile(file, rebuildOpenRecentItems: false))
                    {
                        successCount++;
                    }
                }
            }
            if (successCount > 0)
            {
                await BuildOpenRecentButtonSubItems();
            }
            return successCount;
        }

        private async Task<StorageFile> OpenSaveAs(ITextEditor textEditor)
        {
            NotepadsCore.SwitchTo(textEditor);
            StorageFile file = await FilePickerFactory.GetFileSavePicker(textEditor, true).PickSaveFileAsync();
            NotepadsCore.FocusOnTextEditor(textEditor);
            return file;
        }

        private async Task TrySave(ITextEditor textEditor, StorageFile file, bool rebuildOpenRecentItems)
        {
            try
            {
                await NotepadsCore.SaveContentToFileAndUpdateEditorState(textEditor, file);
                var success = MRUService.TryAdd(file); // Remember recently used files
                if (success && rebuildOpenRecentItems)
                {
                    await BuildOpenRecentButtonSubItems();
                }
            } catch (Exception)
            {
                throw new UnauthorizedAccessException("Access denied");
            }
        }

        private async Task<bool> Save(ITextEditor textEditor, bool saveAs, bool ignoreUnmodifiedDocument = false, bool rebuildOpenRecentItems = true)
        {
            if (textEditor == null) return false;

            if (ignoreUnmodifiedDocument && !textEditor.IsModified)
            {
                return true;
            }

            StorageFile file = null;
            try
            {
                if (textEditor.EditingFile == null || saveAs)
                {
                    file = await OpenSaveAs(textEditor);
                    if (file == null)
                    {
                        return false; // User cancelled
                    }
                }
                else
                {
                    file = textEditor.EditingFile;
                }

                await TrySave(textEditor, file, rebuildOpenRecentItems);
                return true;
            }
            catch(UnauthorizedAccessException ex)
            {
                file = await OpenSaveAs(textEditor);
                if (file == null)
                {
                    return false; // User cancelled
                }

                await TrySave(textEditor, file, rebuildOpenRecentItems);
                return true;
            }
            catch (Exception ex)
            {
                var fileSaveErrorDialog = NotepadsDialogFactory.GetFileSaveErrorDialog((file == null) ? string.Empty : file.Path, ex.Message);
                await DialogManager.OpenDialogAsync(fileSaveErrorDialog, awaitPreviousDialog: false);
                if (!fileSaveErrorDialog.IsAborted)
                {
                    NotepadsCore.FocusOnSelectedTextEditor();
                }
                return false;
            }
        }

        private async Task<bool> SaveAll(ITextEditor[] textEditors)
        {
            var success = false;

            foreach (var textEditor in textEditors)
            {
                if (await Save(textEditor, saveAs: false, ignoreUnmodifiedDocument: true, rebuildOpenRecentItems: false)) success = true;
            }

            if (success)
            {
                await BuildOpenRecentButtonSubItems();
            }

            return success;
        }

        public async Task Print(ITextEditor textEditor)
        {
            if (App.IsGameBarWidget) return;
            if (textEditor == null) return;
            await PrintAll(new[] { textEditor });
        }

        public async Task PrintAll(ITextEditor[] textEditors)
        {
            if (App.IsGameBarWidget) return;
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
                if (string.IsNullOrEmpty(textEditor.GetText())) continue;
                return true;
            }
            return false;
        }
    }
}