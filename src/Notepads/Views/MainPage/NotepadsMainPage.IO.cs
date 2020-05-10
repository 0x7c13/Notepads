namespace Notepads.Views.MainPage
{
    using System;
    using System.Collections.Generic;
    using System.IO;
    using System.Threading.Tasks;
    using Windows.Graphics.Printing;
    using Windows.Storage;
    using Notepads.Controls.Dialog;
    using Notepads.Controls.FilePicker;
    using Notepads.Controls.Print;
    using Notepads.Controls.TextEditor;
    using Notepads.Extensions;
    using Notepads.Services;
    using Notepads.Utilities;
    using Microsoft.Gaming.XboxGameBar;

    public sealed partial class NotepadsMainPage
    {
        private async Task OpenNewFiles()
        {
            IReadOnlyList<StorageFile> files;

            try
            {
                files = await OpenFilesUsingFileOpenPicker();
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

        private async Task<IReadOnlyList<StorageFile>> OpenFilesUsingFileOpenPicker()
        {
            IReadOnlyList<StorageFile> files = null;

            ForegroundWorkHandler foregroundWork = (() =>
            {
                return Task.Run(async () =>
                {
                    files = await Dispatcher.RunTaskAsync(async () =>
                    {
                        var fileOpenPicker = FilePickerFactory.GetFileOpenPicker();
                        foreach (var type in FileTypeService.AllSupportedFileExtensions)
                        {
                            fileOpenPicker.FileTypeFilter.Add(type);
                        }
                        return await fileOpenPicker.PickMultipleFilesAsync();
                    });

                    return true;
                }).AsAsyncOperation<bool>();
            });

            if (_widget != null)
            {
                var foregroundWorker = new XboxGameBarForegroundWorker(_widget, foregroundWork);
                await foregroundWorker.ExecuteAsync();
            }
            else
            {
                await foregroundWork.Invoke();
            }
            
            return files;
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

        private async Task<StorageFile> OpenFileUsingFileSavePicker(ITextEditor textEditor)
        {
            NotepadsCore.SwitchTo(textEditor);
            StorageFile file = await FilePickerFactory.GetFileSavePicker(textEditor, true).PickSaveFileAsync();
            NotepadsCore.FocusOnTextEditor(textEditor);
            return file;
        }

        private async Task SaveInternal(ITextEditor textEditor, StorageFile file, bool rebuildOpenRecentItems)
        {
            await NotepadsCore.SaveContentToFileAndUpdateEditorState(textEditor, file);
            var success = MRUService.TryAdd(file); // Remember recently used files
            if (success && rebuildOpenRecentItems)
            {
                await BuildOpenRecentButtonSubItems();
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
                    file = await OpenFileUsingFileSavePicker(textEditor);
                    if (file == null) return false; // User cancelled
                }
                else
                {
                    file = textEditor.EditingFile;
                }

                bool promptSaveAs = false;
                try
                {
                    await SaveInternal(textEditor, file, rebuildOpenRecentItems);
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
                    file = await OpenFileUsingFileSavePicker(textEditor);
                    if (file == null) return false; // User cancelled

                    await SaveInternal(textEditor, file, rebuildOpenRecentItems);
                    return true;
                }

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

        private async Task<StorageFile> SaveFileUsingFileSavePicker(ITextEditor textEditor, bool saveAs)
        {
            StorageFile file = null;

            // Create a lambda for the UI work and re-use if not running as a Game Bar widget
            // If you are doing async work on the UI thread inside this lambda, it must be awaited before the lambda returns to ensure Game Bar is
            // in the right state for the entirety of the foreground operation.
            // We recommend using the Dispatcher RunTaskAsync task extension to make this easier
            // Look at Extensions/DispatcherTaskExtensions.cs
            // For more information you can read this blog post: https://devblogs.microsoft.com/oldnewthing/20190327-00/?p=102364
            // For another approach more akin to how C++/WinRT handles awaitable thread switching, read this blog post: https://devblogs.microsoft.com/oldnewthing/20190328-00/?p=102368
            ForegroundWorkHandler filePickerWork = (() =>
            {
                return Task.Run(async () =>
                {
                    file = await Dispatcher.RunTaskAsync<StorageFile>(async () =>
                        await FilePickerFactory.GetFileSavePicker(textEditor, saveAs).PickSaveFileAsync());
                    return true;
                }).AsAsyncOperation<bool>();
            });

            if (App.IsGameBarWidget && _widget != null)
            {
                await new XboxGameBarForegroundWorker(_widget, filePickerWork).ExecuteAsync();
            }
            else
            {
                await filePickerWork.Invoke();
            }

            return file;
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