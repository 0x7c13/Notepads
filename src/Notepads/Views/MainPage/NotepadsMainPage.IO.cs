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
    using Microsoft.AppCenter.Analytics;
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
                var fileOpenErrorDialog = new FileOpenErrorDialog(filePath: null, ex.Message);
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
                        return await FilePickerFactory.GetFileOpenPicker().PickMultipleFilesAsync();
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

                TrackFileExtensionIfNotSupported(file);

                return true;
            }
            catch (Exception ex)
            {
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
                    Analytics.TrackEvent("UnsupportedFileExtension", new Dictionary<string, string>()
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
                    file = await PickFileForSavingUsingFileSavePicker(textEditor);
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
                    file = await PickFileForSavingUsingFileSavePicker(textEditor);
                    if (file == null) return false; // User cancelled

                    await SaveInternal(textEditor, file, rebuildOpenRecentItems);
                    return true;
                }

                return true;
            }
            catch (Exception ex)
            {
                var fileSaveErrorDialog = new FileSaveErrorDialog((file == null) ? string.Empty : file.Path, ex.Message);
                await DialogManager.OpenDialogAsync(fileSaveErrorDialog, awaitPreviousDialog: false);
                if (!fileSaveErrorDialog.IsAborted)
                {
                    NotepadsCore.FocusOnSelectedTextEditor();
                }
                return false;
            }
        }

        private async Task<StorageFile> PickFileForSavingUsingFileSavePicker(ITextEditor textEditor)
        {
            NotepadsCore.SwitchTo(textEditor);

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
                        await FilePickerFactory.GetFileSavePicker(textEditor).PickSaveFileAsync());
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

            NotepadsCore.FocusOnTextEditor(textEditor);

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

        private async Task RenameFileAsync(ITextEditor textEditor)
        {
            if (textEditor == null) return;

            if (textEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted) return;

            if (textEditor.EditingFile != null && FileSystemUtility.IsFileReadOnly(textEditor.EditingFile)) return;

            var fileRenameDialog = new FileRenameDialog(textEditor.EditingFileName ?? textEditor.FileNamePlaceholder,
                fileExists: textEditor.EditingFile != null,
                confirmedAction: async (newFilename) =>
                {
                    try
                    {
                        await textEditor.RenameAsync(newFilename);
                        NotepadsCore.FocusOnSelectedTextEditor();
                        NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileRenamed"), 1500);
                    }
                    catch (Exception ex)
                    {
                        var errorMessage = ex.Message?.TrimEnd('\r', '\n');
                        NotificationCenter.Instance.PostNotification(errorMessage, 3500); // TODO: Use Content Dialog to display error message
                    }
                });

            await DialogManager.OpenDialogAsync(fileRenameDialog, awaitPreviousDialog: false);
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