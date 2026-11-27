// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.IO;
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
using Windows.UI.Xaml.Controls;

namespace Notepads.Presentation.Views.MainPage;

public sealed partial class NotepadsMainPage
{
    private ulong _markdownLinkRequest;

    private async Task CreateNewTextEditorAsync(bool onlyIfEmpty = false)
    {
        if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing) return;
        ITextEditor editor = null;
        try
        {
            editor = await NotepadsCore.CreateNewTextEditorAsync(_defaultNewFileName);
            if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing) { await NotepadsCore.DiscardUnattachedEditorAsync(editor); return; }
            // Creating the journal yields. An external activation or a user
            // command may have already supplied the startup document meanwhile.
            if (onlyIfEmpty && NotepadsCore.GetTabCount() != 0) { await NotepadsCore.DiscardUnattachedEditorAsync(editor); return; }
            NotepadsCore.OpenTextEditor(editor);
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to create a document: {ex}");
            if (editor != null && NotepadsCore.GetTab(editor) == null) await NotepadsCore.DiscardUnattachedEditorAsync(editor);
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

    public Task<bool> OpenFileAsync(StorageFile file, bool rebuildOpenRecentItems = true) =>
        OpenFileAsync(file, rebuildOpenRecentItems, mayPublish: null);

    private Task<bool> OpenFileAsync(StorageFile file, bool rebuildOpenRecentItems, Func<bool> mayPublish, Action<WorkspaceTab> opened = null)
    {
        if (_viewClosed || _isAppClosing || file == null || NotepadsCore.IsClosing) return Task.FromResult(false);
        return NotepadsCore.OpenFileAsync(file, async result =>
        {
            if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing) return;
            if (result.Tab != null) opened?.Invoke(result.Tab);
            switch (result.Kind)
            {
                case WorkspaceFileOpenKind.Existing when mayPublish == null:
                    NotificationCenter.Instance.PostNotification(
                        _resourceLoader.GetString("TextEditor_NotificationMsg_FileAlreadyOpened"), 2500);
                    break;
                case WorkspaceFileOpenKind.Created:
                    if (MRUService.TryAdd(file) && rebuildOpenRecentItems)
                        await BuildOpenRecentButtonSubItemsAsync();
                    TrackFileExtensionIfNotSupported(file);
                    break;
                case WorkspaceFileOpenKind.Failed:
                    LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to open [{file.Path}]: {result.Error}");
                    var message = result.Error is SessionFileRecoveryException ?
                        _resourceLoader.GetString("SessionBackup_NotificationMsg_RecoveryIncomplete") : result.Error.Message;
                    var dialog = new FileOpenErrorDialog(file.Path, message);
                    await DialogManager.OpenDialogAsync(dialog, awaitPreviousDialog: false);
                    if (!dialog.IsAborted) NotepadsCore.FocusOnSelectedTextEditor();
                    break;
            }
        }, mayPublish);
    }

    internal async Task<bool> OpenMarkdownLinkAsync(StorageFile file, string fragment, bool wikiHeading, Func<bool> isCurrent = null)
    {
        if (isCurrent?.Invoke() == false) return false;
        var request = ++_markdownLinkRequest;
        bool MayPublish() => request == _markdownLinkRequest && isCurrent?.Invoke() != false;
        WorkspaceTab opened = null;
        await OpenFileAsync(file, rebuildOpenRecentItems: true, MayPublish, tab => opened = tab);
        if (_viewClosed || _isAppClosing || NotepadsCore.IsClosing || request != _markdownLinkRequest) return true;

        // Opening the target can hide the originating preview. After publication
        // only the page request and selected destination govern the fragment.
        var selected = NotepadsCore.GetSelectedReadyEditor();
        if (selected == null || !ReferenceEquals(selected, opened?.ReadyEditor)) return false;
        return fragment == null || (selected is TextEditor editor && editor.QueueMarkdownFragment(fragment, wikiHeading));
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
        if (textEditor == null || NotepadsCore.GetTab(textEditor) == null) return null;
        if (!_isAppClosing) NotepadsCore.SwitchTo(textEditor);
        StorageFile file = await FilePickerFactory.GetFileSavePicker(textEditor).PickSaveFileAsync();
        if (!_viewClosed && !_isAppClosing && NotepadsCore.GetTab(textEditor) != null)
            NotepadsCore.FocusOnTextEditor(textEditor);
        return file;
    }

    private async Task SaveInternalAsync(ITextEditor textEditor, StorageFile file, bool rebuildOpenRecentItems, DocumentFileVersion? pickedFileVersion, CancellationToken cancellation, Guid? verificationId)
    {
        var committedFile = await NotepadsCore.SaveContentToFileAndUpdateEditorStateAsync(textEditor, file, pickedFileVersion, cancellation, verificationId);
        var success = MRUService.TryAdd(committedFile);
        if (success && rebuildOpenRecentItems)
        {
            await BuildOpenRecentButtonSubItemsAsync();
        }
    }

    partial void OnPickedVersionCaptured(StorageFile file);

    private async Task<bool> SaveAsync(ITextEditor textEditor, bool saveAs, bool ignoreUnmodifiedDocument = false,
        bool rebuildOpenRecentItems = true, bool allowWhileClosing = false, bool allowWhileTabClosing = false)
    {
        if (!CanSaveEditor(textEditor, allowWhileClosing, allowWhileTabClosing)) return false;

        if (ignoreUnmodifiedDocument && !textEditor.IsModified)
        {
            return true;
        }

        StorageFile file = null;
        var cancellation = NotepadsCore.GetTab(textEditor).SaveToken;

        try
        {
            var binding = textEditor.EditingFile;
            file = binding;
            var pickTarget = saveAs || file == null;
            DocumentFileVersion? pickedFileVersion = null;
            bool readPickedVersion = false;
            Guid? verificationId = null;
            async Task<bool> CanContinueAsync()
            {
                while (!cancellation.IsCancellationRequested && CanSaveEditor(textEditor, allowWhileClosing, allowWhileTabClosing))
                {
                    var current = textEditor.EditingFile;
                    var same = await DocumentSaveTarget.IsSameFileAsync(binding, current, cancellation);
                    if (!ReferenceEquals(current, textEditor.EditingFile)) continue;
                    if (cancellation.IsCancellationRequested || !CanSaveEditor(textEditor, allowWhileClosing, allowWhileTabClosing)) return false;
                    if (same) return true;
                    break;
                }
                if (cancellation.IsCancellationRequested || !CanSaveEditor(textEditor, allowWhileClosing, allowWhileTabClosing)) return false;
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("FileSaveErrorDialog_VersionConflict"), 3500);
                return false;
            }

            while (await CanContinueAsync())
            {
                if (pickTarget)
                {
                    file = await OpenFileUsingFileSavePickerAsync(textEditor);
                    if (file == null || !await CanContinueAsync()) return false;
                    // The picker authorizes this destination, including an explicit replace.
                    // Keep that version through queuing and retries.
                    verificationId = null;
                    readPickedVersion = true;
                    pickTarget = false;
                }
                try
                {
                    if (readPickedVersion)
                    {
                        try { pickedFileVersion = await textEditor.ReadPickedFileVersionAsync(file, cancellation); }
                        catch (OperationCanceledException) when (cancellation.IsCancellationRequested ||
                            !CanSaveEditor(textEditor, allowWhileClosing, allowWhileTabClosing)) { return false; }
                        OnPickedVersionCaptured(file);
                        readPickedVersion = false;
                        if (!await CanContinueAsync()) return false;
                    }
                    await SaveInternalAsync(textEditor, file, rebuildOpenRecentItems, pickedFileVersion, cancellation, verificationId);
                    return true;
                }
                catch (DocumentSaveException failure)
                {
                    LoggingService.LogError($"[Save] {failure.Stage} (0x{failure.HResult:X8}): {failure}");
                    if (failure.RenamedBinding is { } renamed && ReferenceEquals(renamed, textEditor.EditingFile))
                    {
                        // A provider may rename the picker grant while the original alias still names a different entry.
                        binding = renamed;
                        file = renamed;
                    }
                    if (!await CanContinueAsync()) return false;
                    var committed = failure.Stage == DocumentSaveStage.ConfirmCommit ||
                        verificationId.HasValue && failure.Stage is DocumentSaveStage.Coordinate or DocumentSaveStage.VerifyVersion;
                    if (committed && failure.CommittedWrite != null) file = failure.CommittedWrite.File;
                    var message = committed ? _resourceLoader.GetString("FileSaveErrorDialog_CommittedUnverified") : failure.Stage switch
                    {
                        DocumentSaveStage.Unverified => _resourceLoader.GetString("FileSaveErrorDialog_Unverified"),
                        DocumentSaveStage.VerifyVersion when failure.InnerException == null => _resourceLoader.GetString("FileSaveErrorDialog_Unverified"),
                        DocumentSaveStage.Coordinate when failure.InnerException is TimeoutException =>
                            _resourceLoader.GetString("FileSaveErrorDialog_SaveBusy"),
                        DocumentSaveStage.Conflict or DocumentSaveStage.Superseded => _resourceLoader.GetString("FileSaveErrorDialog_VersionConflict"),
                        DocumentSaveStage.PrepareSnapshot => _resourceLoader.GetString("FileSaveErrorDialog_SnapshotFailed"),
                        _ => failure.Message
                    };
                    var dialog = new FileSaveErrorDialog(file, message, offerActions: true,
                        allowRetry: failure.Stage == DocumentSaveStage.ConfirmCommit ? failure.VerificationId.HasValue :
                            !readPickedVersion && failure.Stage is not (DocumentSaveStage.Conflict or DocumentSaveStage.Superseded or DocumentSaveStage.Unverified),
                        committed: committed);
                    var action = await OpenSaveErrorDialogAsync(dialog, cancellation);
                    if (dialog.IsAborted || action == null || action == ContentDialogResult.None) return false;
                    pickTarget = action == ContentDialogResult.Secondary;
                    if (failure.Stage == DocumentSaveStage.ConfirmCommit) verificationId = failure.VerificationId;
                }
            }
            return false;
        }
        catch (OperationCanceledException) when (cancellation.IsCancellationRequested || NotepadsCore.GetTab(textEditor) == null)
        {
            return false;
        }
        catch (ObjectDisposedException) when (NotepadsCore.GetTab(textEditor) == null)
        {
            return false;
        }
        catch (Exception ex)
        {
            if (cancellation.IsCancellationRequested || !CanSaveEditor(textEditor, allowWhileClosing, allowWhileTabClosing)) return false;
            var fileSaveErrorDialog = new FileSaveErrorDialog(file, ex.Message);
            var action = await OpenSaveErrorDialogAsync(fileSaveErrorDialog, cancellation);
            if (action != null && !fileSaveErrorDialog.IsAborted && CanSaveEditor(textEditor, allowWhileClosing, allowWhileTabClosing))
            {
                NotepadsCore.FocusOnSelectedTextEditor();
            }
            return false;
        }
    }

    private static async Task<ContentDialogResult?> OpenSaveErrorDialogAsync(FileSaveErrorDialog dialog, CancellationToken cancellation)
    {
        if (cancellation.IsCancellationRequested) return null;
        try { return await DialogManager.OpenDialogAsync(dialog, awaitPreviousDialog: false).WaitAsync(cancellation); }
        catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { return null; }
        finally
        {
            if (cancellation.IsCancellationRequested && ReferenceEquals(DialogManager.ActiveDialog, dialog)) dialog.Hide();
        }
    }

    private bool CanSaveEditor(ITextEditor textEditor, bool allowWhileClosing, bool allowWhileTabClosing) =>
        !_viewClosed && textEditor != null && (!_isAppClosing || allowWhileClosing) &&
        (_closeDecisionCount == 0 || allowWhileClosing || allowWhileTabClosing) &&
        (allowWhileTabClosing || !_closingTextEditors.Contains(textEditor.Id)) && NotepadsCore.GetTab(textEditor) != null;

    private async Task<bool> SaveAllAsync(WorkspaceTab[] tabs)
    {
        var success = false;
        var complete = true;
        bool CanContinueBatch() => !_viewClosed && !_isAppClosing && _closeDecisionCount == 0;

        foreach (var tab in tabs)
        {
            if (!CanContinueBatch()) { complete = false; break; }
            var textEditor = await SessionController.EnsureLoadedAsync(tab);
            if (!CanContinueBatch()) { complete = false; break; }
            if (!NotepadsCore.ContainsTab(tab)) continue;
            if (textEditor == null)
            {
                complete = false;
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("WorkspaceTab_LoadFailed"), 3500);
                continue;
            }
            if (await SaveAsync(textEditor, saveAs: false, ignoreUnmodifiedDocument: true, rebuildOpenRecentItems: false)) success = true;
            else complete = false;
        }

        if (success && !_viewClosed && !_isAppClosing)
        {
            await BuildOpenRecentButtonSubItemsAsync();
        }

        return complete;
    }

    private async Task RenameFileAsync(ITextEditor textEditor)
    {
        if (_viewClosed || _isAppClosing || textEditor == null) return;

        if (textEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted) return;

        if (textEditor.EditingFile != null && FileStorage.IsFileReadOnly(textEditor.EditingFile)) return;

        var expectedFile = textEditor.EditingFile;
        string newFileName = null;
        var fileRenameDialog = new FileRenameDialog(textEditor.DisplayTitle,
            fileExists: textEditor.EditingFile != null,
            confirmedAction: filename => { newFileName = filename; },
            allowUnchangedName: textEditor.UsesContentBasedTitle);

        var result = await DialogManager.OpenDialogAsync(fileRenameDialog, awaitPreviousDialog: false);
        if (result == null || fileRenameDialog.IsAborted || newFileName == null || _isAppClosing ||
            NotepadsCore.GetTab(textEditor) == null)
        {
            return;
        }

        try
        {
            await textEditor.RenameAsync(newFileName, expectedFile);
            if (NotepadsCore.GetTab(textEditor) == null) return;
            NotepadsCore.FocusOnSelectedTextEditor();
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileRenamed"), 1500);
        }
        catch (OperationCanceledException) { }
        catch (ObjectDisposedException) when (NotepadsCore.GetTab(textEditor) == null) { }
        catch (Exception ex)
        {
            if (NotepadsCore.GetTab(textEditor) == null) return;
            var errorMessage = ex.Message?.TrimEnd('\r', '\n');
            NotificationCenter.Instance.PostNotification(errorMessage, 3500);
        }
    }

    // Print layout copies and flows the whole text on the UI thread. V1's 1 MB
    // open limit used to cap it; 1 MiB is known to print acceptably.
    private const long PrintDocumentLengthLimit = 1024 * 1024;

    public async Task PrintAsync(ITextEditor textEditor)
    {
        if (_viewClosed || _isAppClosing || textEditor == null || NotepadsCore.GetTab(textEditor) == null) return;
        await PrintEditorsAsync([textEditor]);
    }

    public async Task PrintAllAsync(WorkspaceTab[] tabs)
    {
        if (tabs == null || tabs.Length == 0 || _viewClosed || _isAppClosing) return;
        var editors = new List<ITextEditor>();
        foreach (var tab in tabs)
        {
            var editor = await SessionController.EnsureLoadedAsync(tab);
            if (_viewClosed || _isAppClosing) return;
            if (!NotepadsCore.ContainsTab(tab)) continue;
            if (editor == null)
            {
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("WorkspaceTab_LoadFailed"), 3500);
                return;
            }
            editors.Add(editor);
        }
        await PrintEditorsAsync(editors.ToArray());
    }

    private async Task PrintEditorsAsync(ITextEditor[] textEditors)
    {
        if (textEditors == null || textEditors.Length == 0) return;

        var printable = Array.FindAll(textEditors, textEditor => textEditor.DocumentLength <= PrintDocumentLengthLimit);
        if (printable.Length != textEditors.Length)
        {
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("Print_NotificationMsg_DocumentTooLarge"), 2500);
            if (printable.Length == 0) return;
        }

        // Initialize print content
        PrintArgs.PreparePrintContent(printable);

        if (PrintManager.IsSupported() && HaveNonemptyTextEditor(printable))
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
