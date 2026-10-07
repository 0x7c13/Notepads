// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Operations;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Documents.Text;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Workspace;
using Windows.Storage;
using Windows.UI.Xaml;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    public DocumentSnapshot LastSavedSnapshot { get; private set; }

    public Guid DocumentOwnerId { get; set; }

    public LineEnding? RequestedLineEnding { get; private set; }

    private Encoding _requestedEncoding;
    private bool _requiresSaveAs;

    public Encoding RequestedEncoding => _requestedEncoding == null ? null : (Encoding)_requestedEncoding.Clone();

    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "Dispose calls BeginShutdown with the coordinator's Dispose delegate, then DisposalCompletion waits for every operation to unwind.")]
    private readonly DocumentOperationCoordinator _documentOperations = new();

    private DocumentBaseline _recoveryBaseline;
    private DocumentJournal _documentJournal;
    private Task _journalShutdown = Task.CompletedTask;
    private Exception _journalShutdownFailure;
    private readonly TaskCompletionSource<object> _disposalCompletion = new(
        TaskCreationOptions.RunContinuationsAsynchronously);
    private bool _recoveryTransition;
    private bool _transferCommitInProgress;

    public ulong DocumentSequence => TextEditorCore.DocumentSequence;

    public Task DisposalCompletion => _disposalCompletion.Task;

    public Task WaitForDocumentOperationsAsync() =>
        _documentOperations.WaitForIdleAsync();

    public async Task<bool> TryCommitTransferAsync(Func<bool> sourceUnchanged,
        Func<Task> commitRemovalIntent, Action removeLogicalSource)
    {
        if (sourceUnchanged == null) throw new ArgumentNullException(nameof(sourceUnchanged));
        if (commitRemovalIntent == null) throw new ArgumentNullException(nameof(commitRemovalIntent));
        if (removeLogicalSource == null) throw new ArgumentNullException(nameof(removeLogicalSource));

        var committed = false;
        await RunDocumentOperationAsync(async cancellation =>
        {
            var wasEnabled = TextEditorCore.IsEnabled;
            TextEditorCore.IsEnabled = false;
            try
            {
                using (var reader = TextEditorCore.AcquireDocumentReader())
                {
                    cancellation.ThrowIfCancellationRequested();
                    if (!sourceUnchanged()) return;
                    _transferCommitInProgress = true;
                    await commitRemovalIntent();
                    // The durable intent and logical removal describe one move.
                    // Cancellation after this commit cannot retain the old tab.
                    removeLogicalSource();
                    committed = true;
                }
            }
            finally
            {
                _transferCommitInProgress = false;
                if (!_disposed) TextEditorCore.IsEnabled = wasEnabled;
            }
        });
        return committed;
    }

    public Task RenameAsync(string newFileName, StorageFile expectedFile)
    {
        return RunDocumentOperationAsync(async cancellation =>
        {
            var file = EditingFile;
            if (file == null ? expectedFile != null : expectedFile == null || !file.IsEqual(expectedFile))
                throw new InvalidOperationException("The document's file changed before it could be renamed.");
            cancellation.ThrowIfCancellationRequested();
            if (file == null) FileNamePlaceholder = newFileName;
            else await file.RenameAsync(newFileName);
            // Once storage commits, closing cannot undo a successful rename.
            // Publication and its UI events belong only to a live editor.
            if (_disposed) return;
            UpdateDocumentInfo();
            FileRenamed?.Invoke(this, EventArgs.Empty);
        });
    }

    public void InitEmpty(Encoding encoding, LineEnding lineEnding, StorageFile file = null)
    {
        if (!TextEditorCore.IsDocumentEmpty || LastSavedSnapshot != null)
            throw new InvalidOperationException("Only a new editor can be initialized synchronously.");
        using (var baseline = DocumentBaseline.CreateEmpty(DocumentOwnerId))
        {
            AdoptSnapshot(new DocumentSnapshot(baseline, encoding, lineEnding), file, isModified: false);
        }
    }

    private void AdoptSnapshot(DocumentSnapshot snapshot, StorageFile file, bool isModified)
    {
        var previous = LastSavedSnapshot;
        LastSavedSnapshot = snapshot;
        previous?.Dispose();
        EditingFile = file;
        // Reverting saved bytes does not recover access to a missing file.
        _requiresSaveAs = file == null && _requiresSaveAs;
        _requestedEncoding = null;
        RequestedLineEnding = null;
        if (isModified) TextEditorCore.MarkModified();
        else TextEditorCore.MarkSaved();
        _loaded = true;
        IsModified = !NoChangesSinceLastSaved();
    }

    public Task InitAsync(DocumentSnapshot snapshot, StorageFile file, bool isModified = false)
    {
        if (_documentJournal != null) throw new InvalidOperationException("Use reload to replace an initialized document.");
        return RunDocumentOperationAsync(cancellation =>
            LoadAndInitializeAsync(snapshot, file, isModified, cancellation));
    }

    public Task InitializeNewAsync() => RunDocumentOperationAsync(cancellation =>
        InitializeRecoveryJournalAsync(LastSavedSnapshot.Baseline, cancellation));

    private async Task InitializeRecoveryJournalAsync(DocumentBaseline baseline, CancellationToken cancellationToken)
    {
        if (_documentJournal != null)
            throw new InvalidOperationException("The document recovery journal is already initialized.");
        var retained = baseline.Retain();
        DocumentJournal journal = null;
        var adopted = false;
        try
        {
            journal = await DocumentJournal.CreateAsync(DocumentOwnerId, cancellationToken);
            cancellationToken.ThrowIfCancellationRequested();
            TextEditorCore.StartJournal(journal);
            _recoveryBaseline?.Dispose();
            _recoveryBaseline = retained;
            _documentJournal = journal;
            adopted = true;
        }
        finally
        {
            if (!adopted)
            {
                retained.Dispose();
                journal?.Dispose();
            }
        }
    }

    private async Task CompleteDisposalAsync(Exception startupFailure)
    {
        try
        {
            await Task.WhenAll(_documentOperations.Completion, _fileStatusCheckerCompletion);
            // An operation may have attached or stopped a candidate while cancellation
            // unwound. Drain every native writer, even when no managed journal is attached.
            _journalShutdown = TextEditorCore.StopJournalAsync();
            await _journalShutdown;
            if (_journalShutdownFailure != null)
                throw new InvalidOperationException("A native recovery journal failed to shut down.", _journalShutdownFailure);
            if (startupFailure != null)
                throw new InvalidOperationException("Editor disposal could not complete.", startupFailure);

            TextEditorCore.Dispose();
            if (_documentJournal != null)
            {
                await _documentJournal.DisposeAsync();
                _documentJournal = null;
            }
            _recoveryBaseline?.Dispose();
            _recoveryBaseline = null;
            LastSavedSnapshot?.Dispose();
            LastSavedSnapshot = null;
            _fileStatusSemaphoreSlim.Dispose();
            _disposalCompletion.TrySetResult(null);
        }
        catch (Exception failure)
        {
            // Keep the leases and fault the public completion. The workspace writer
            // cannot be released while native shutdown or owned cleanup is uncertain.
            _disposalCompletion.TrySetException(failure);
        }
    }

    private async Task ReleaseJournalAsync(TextEditorCore core, DocumentJournal journal)
    {
        try { await core.StopJournalAsync().ConfigureAwait(false); }
        catch (Exception ex)
        {
            // A failed shutdown cannot prove that the writer released its
            // file. Keep it for conservative recovery cleanup after restart.
            journal.PreserveForRecovery();
            _journalShutdownFailure ??= ex;
            throw;
        }
        await journal.DisposeAsync().ConfigureAwait(false);
    }

    public DocumentRecoveryState CaptureRecoveryState()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(TextEditor));
        if (!_loaded || _recoveryTransition || _transferCommitInProgress || _documentJournal == null)
            throw new InvalidOperationException("The document is still preparing its recovery state.");
        using (var checkpoint = TextEditorCore.AcquireJournalCheckpoint())
        {
            return new DocumentRecoveryState(LastSavedSnapshot, _recoveryBaseline, _documentJournal,
                checkpoint, GetTextEditorStateMetaData(), TextEditorCore.IsDocumentModified,
                checked((long)checkpoint.CommittedDocumentByteLength));
        }
    }

    private async Task LoadAndInitializeAsync(DocumentSnapshot snapshot, StorageFile file, bool isModified,
        CancellationToken cancellationToken, DocumentMetadata metadata = null,
        DocumentBaseline pendingBaseline = null, bool preserveUndo = false)
    {
        var prepared = snapshot.Retain();
        var adopted = false;
        try
        {
            using (var content = (pendingBaseline ?? prepared.Baseline).Retain())
            {
                var wasLoaded = _loaded;
                var wasEnabled = TextEditorCore.IsEnabled;
                _loaded = false;
                TextEditorCore.IsEnabled = false;
                try
                {
                    var sequence = TextEditorCore.DocumentSequence;
                    await TextEditorCore.LoadBaselineAsync(content, preserveUndo, cancellationToken);
                    if (_disposed) return;
                    // An undoable load that inserts text commits exactly one journal operation. If no
                    // other operation committed meanwhile, the document is this baseline at that sequence.
                    var loadedSequence = TextEditorCore.DocumentSequence;
                    var rebaseRecovery = preserveUndo && content.ByteLength > 0 && loadedSequence == sequence + 1;
                    if (!preserveUndo)
                        await InitializeRecoveryJournalAsync(content, cancellationToken);
                    if (_disposed) return;
                    var contentModified = metadata == null ? isModified : metadata.IsModified &&
                        (content.ByteLength != prepared.Baseline.ByteLength || content.Sha256 != prepared.Baseline.Sha256);
                    adopted = true;
                    AdoptSnapshot(prepared, file, contentModified);
                    if (metadata != null) ResetEditorState(metadata);
                    // Drop the whole-document replacement from the journal. Files over the
                    // compaction budget would otherwise keep it until the next explicit save.
                    if (rebaseRecovery) await TryRotateRecoveryJournalAsync(content, loadedSequence);
                }
                catch
                {
                    if (!_disposed) _loaded = wasLoaded;
                    throw;
                }
                finally
                {
                    if (!_disposed) TextEditorCore.IsEnabled = wasEnabled;
                }
            }
        }
        finally { if (!adopted) prepared.Dispose(); }
    }

    public Task ReloadFromEditingFileAsync(Encoding encoding = null,
        DocumentDecodingMode decodingMode = DocumentDecodingMode.ConfiguredDefault)
    {
        return RunDocumentOperationAsync(cancellation => ReloadFromEditingFileCoreAsync(encoding, decodingMode, cancellation));
    }

    private async Task ReloadFromEditingFileCoreAsync(Encoding encoding, DocumentDecodingMode decodingMode,
        CancellationToken cancellation)
    {
        var file = EditingFile;
        if (file == null) return;
        var wasEnabled = TextEditorCore.IsEnabled;
        var version = TextEditorCore.ContentVersion;
        TextEditorCore.IsEnabled = false;
        try
        {
            var options = new DocumentLoadOptions(encoding, ApplicationPreferences.EditorDefaultDecoding, decodingMode);
            using (var snapshot = await DocumentTextPipeline.DecodeFileAsync(file, options,
                DocumentOwnerId, cancellationToken: cancellation))
            {
                cancellation.ThrowIfCancellationRequested();
                if (version != TextEditorCore.ContentVersion)
                    throw new InvalidOperationException("The document changed while its file was being reloaded.");
                await LoadAndInitializeAsync(snapshot, file, false, cancellation, preserveUndo: true);
            }
            if (_disposed) return;
            LineEndingChanged?.Invoke(this, EventArgs.Empty);
            EncodingChanged?.Invoke(this, EventArgs.Empty);
            StartCheckingFileStatusPeriodically();
            CloseSideBySideDiffViewer();
            // Closing diff restores editing; preserve that enabled state.
            wasEnabled |= TextEditorCore.IsEnabled;
            HideGoToControl();
            FileReloaded?.Invoke(this, EventArgs.Empty);
            AnalyticsService.TrackEvent(encoding == null ? "OnFileReloaded" : "OnFileReopenedWithEncoding");
        }
        finally
        {
            if (!_disposed) TextEditorCore.IsEnabled = wasEnabled;
        }
    }

    public void ResetEditorState(DocumentMetadata metadata)
    {
        RestoreLanguageOverride(metadata.LanguageOverride);
        RefreshDocumentLanguage();
        _requiresSaveAs = EditingFile == null && (metadata.RequiresSaveAs || metadata.HasEditingFile);
        if (!string.IsNullOrEmpty(metadata.RequestedEncoding))
        {
            TryChangeEncoding(EncodingCatalog.GetEncodingByName(metadata.RequestedEncoding));
        }

        if (!string.IsNullOrEmpty(metadata.RequestedLineEnding))
        {
            TryChangeLineEnding(LineEndingUtility.GetLineEndingByName(metadata.RequestedLineEnding));
        }

        TextEditorCore.TextWrapping = metadata.WrapWord ? TextWrapping.Wrap : TextWrapping.NoWrap;
        TextEditorCore.FontSize = metadata.FontZoomFactor * ApplicationPreferences.EditorFontSize;
        TextEditorCore.SetTextSelectionPosition(metadata.SelectionStartPosition, metadata.SelectionEndPosition);
        TextEditorCore.SetScrollViewerInitPosition(metadata.ScrollViewerHorizontalOffset, metadata.ScrollViewerVerticalOffset);
        TextEditorCore.ClearUndoQueue();
        IsModified = !NoChangesSinceLastSaved();
    }

    public Task RestoreAsync(DocumentSnapshot snapshot, StorageFile file, DocumentMetadata metadata,
        DocumentBaseline pendingBaseline = null)
    {
        return RunDocumentOperationAsync(cancellation =>
            LoadAndInitializeAsync(snapshot, file, metadata.IsModified, cancellation, metadata, pendingBaseline));
    }

    public Task RestoreV2Async(DocumentSnapshot savedSnapshot, DocumentBaseline recoveryBaseline,
        DocumentJournal journal, EditorJournalCheckpoint checkpoint, StorageFile file,
        DocumentMetadata metadata, bool textDirty)
    {
        return RunDocumentOperationAsync(async cancellation =>
        {
            if (_documentJournal != null)
                throw new InvalidOperationException("Recovery requires a new editor.");
            using (var capture = new DocumentRecoveryState(savedSnapshot, recoveryBaseline, journal,
                checkpoint, metadata, textDirty, checked((long)checkpoint.CommittedDocumentByteLength)))
            {
                var saved = capture.SavedSnapshot.Retain();
                var recovery = capture.RecoveryBaseline.Retain();
                DocumentJournal candidateJournal = null;
                var adopted = false;
                var wasEnabled = TextEditorCore.IsEnabled;
                _loaded = false;
                TextEditorCore.IsEnabled = false;
                try
                {
                    candidateJournal = await DocumentJournal.CreateAsync(DocumentOwnerId, cancellation);
                    await TextEditorCore.RestoreBaselineAsync(recovery, capture.Checkpoint, cancellation);
                    if (_disposed) return;
                    await TextEditorCore.StartJournalFromCheckpointAsync(candidateJournal, capture.Checkpoint, cancellation);
                    if (_disposed)
                    {
                        await ReleaseJournalAsync(TextEditorCore, candidateJournal);
                        candidateJournal = null;
                        return;
                    }
                    _recoveryBaseline = recovery;
                    _documentJournal = candidateJournal;
                    adopted = true;
                    AdoptSnapshot(saved, file, textDirty);
                    ResetEditorState(capture.MetaData);
                }
                finally
                {
                    if (!adopted)
                    {
                        saved.Dispose();
                        recovery.Dispose();
                        candidateJournal?.Dispose();
                    }
                    if (!_disposed) TextEditorCore.IsEnabled = wasEnabled;
                }
            }
        });
    }

    private async Task TryRotateRecoveryJournalAsync(DocumentBaseline savedBaseline, ulong sequence)
    {
        var baseline = savedBaseline.Retain();
        DocumentJournal candidate = null;
        var adopted = false;
        _recoveryTransition = true;
        try
        {
            candidate = await DocumentJournal.CreateAsync(DocumentOwnerId);
            if (_disposed) return;
            await TextEditorCore.RotateJournalAsync(candidate, sequence, CancellationToken.None);
            if (_disposed)
            {
                await ReleaseJournalAsync(TextEditorCore, candidate);
                candidate = null;
                return;
            }
            var previousJournal = _documentJournal;
            var previousBaseline = _recoveryBaseline;
            _documentJournal = candidate;
            _recoveryBaseline = baseline;
            adopted = true;
            previousJournal.Dispose();
            previousBaseline.Dispose();
        }
        catch (Exception ex)
        {
            // The existing journal still protects the current text. Checkpoint
            // maintenance cannot turn a committed save or load into failure.
            LoggingService.LogError($"[{nameof(TextEditor)}] Recovery checkpoint rotation deferred: {ex.Message}");
        }
        finally
        {
            _recoveryTransition = false;
            if (!adopted)
            {
                candidate?.Dispose();
                baseline.Dispose();
            }
        }
    }

    /// <summary>
    /// Rebase a journal whose writer failed (for example, the disk filled) on the current text.
    /// If the disk is still full this fails again, and the next session capture reports it.
    /// </summary>
    public Task RepairRecoveryJournalAsync(CancellationToken cancellationToken = default)
    {
        if (_disposed || !TextEditorCore.IsJournalFaulted) return Task.CompletedTask;
        return _documentOperations.RunAsync(async cancellation =>
        {
            if (_disposed || !_loaded || _documentJournal == null || _recoveryTransition || !TextEditorCore.IsJournalFaulted) return;
            using (var reader = TextEditorCore.AcquireDocumentReader())
            {
                using (var source = new EditorDocumentStream(reader, ownsReader: false))
                using (var baseline = await DocumentBaseline.CreateAsync(output => source.CopyToAsync(output, 65536, cancellation),
                    DocumentOwnerId, cancellation))
                {
                    if (!_disposed) await TryRotateRecoveryJournalAsync(baseline, reader.Sequence);
                }
            }
        }, cancellationToken);
    }

    public Task MaintainRecoveryAsync(CancellationToken cancellationToken = default)
    {
        return _documentOperations.RunAsync(async cancellation =>
        {
            const ulong candidateBudget = 128UL * 1024 * 1024;
            if (_disposed || !_loaded || _documentJournal == null || _recoveryTransition) return;
            using (var recovery = CaptureRecoveryState())
            {
                // Recovery remains valid when the candidate would exceed our
                // native memory budget. Explicit saves rotate every size.
                if (!DocumentJournal.ShouldCompact(recovery.Checkpoint.CommittedByteLength,
                        recovery.Checkpoint.CommittedDocumentByteLength) ||
                    recovery.Checkpoint.CommittedDocumentByteLength > candidateBudget ||
                    checked((ulong)recovery.RecoveryBaseline.ByteLength) > candidateBudget)
                {
                    return;
                }

                await recovery.Checkpoint.FlushAsync().AsCompletionTask(cancellation);
                using (var input = await recovery.RecoveryBaseline.OpenReadStreamAsync(cancellation))
                using (var baseline = await DocumentBaseline.CreateAsync(output =>
                    recovery.Checkpoint.WriteBaselineAsync(input.AsInputStream(),
                        checked((ulong)recovery.RecoveryBaseline.ByteLength), output.AsOutputStream(), candidateBudget)
                        .AsCompletionTask(cancellation), DocumentOwnerId, cancellation))
                {
                    cancellation.ThrowIfCancellationRequested();
                    if (_disposed || TextEditorCore.DocumentSequence != recovery.Checkpoint.CommittedSequence) return;
                    // Reconstruction runs off the UI thread while editing
                    // continues. Rotation checks this sequence again at its
                    // publication boundary and keeps the old journal on a race.
                    await TryRotateRecoveryJournalAsync(baseline, recovery.Checkpoint.CommittedSequence);
                }
            }
        }, cancellationToken);
    }

    public Task RevertAllChangesAsync()
    {
        return RunDocumentOperationAsync(async cancellation =>
        {
            CloseSideBySideDiffViewer();
            await LoadAndInitializeAsync(LastSavedSnapshot, EditingFile, false, cancellation, preserveUndo: true);
            if (_disposed) return;
            LineEndingChanged?.Invoke(this, EventArgs.Empty);
            EncodingChanged?.Invoke(this, EventArgs.Empty);
            ChangeReverted?.Invoke(this, EventArgs.Empty);
        });
    }

    public bool TryChangeEncoding(Encoding encoding)
    {
        if (_transferCommitInProgress || encoding == null) return false;

        if (!EncodingCatalog.Equals(LastSavedSnapshot.Encoding, encoding))
        {
            _requestedEncoding = (Encoding)encoding.Clone();
            IsModified = true;
            EncodingChanged?.Invoke(this, EventArgs.Empty);
            return true;
        }

        if (_requestedEncoding != null && EncodingCatalog.Equals(LastSavedSnapshot.Encoding, encoding))
        {
            _requestedEncoding = null;
            IsModified = !NoChangesSinceLastSaved();
            EncodingChanged?.Invoke(this, EventArgs.Empty);
            return true;
        }
        return false;
    }

    public bool TryChangeLineEnding(LineEnding lineEnding)
    {
        if (_transferCommitInProgress) return false;

        if (LastSavedSnapshot.LineEnding != lineEnding)
        {
            RequestedLineEnding = lineEnding;
            IsModified = true;
            LineEndingChanged?.Invoke(this, EventArgs.Empty);
            return true;
        }

        if (RequestedLineEnding != null && LastSavedSnapshot.LineEnding == lineEnding)
        {
            RequestedLineEnding = null;
            IsModified = !NoChangesSinceLastSaved();
            LineEndingChanged?.Invoke(this, EventArgs.Empty);
            return true;
        }
        return false;
    }

    public LineEnding GetLineEnding()
    {
        return RequestedLineEnding ?? LastSavedSnapshot.LineEnding;
    }

    public Encoding GetEncoding()
    {
        return _requestedEncoding == null ? LastSavedSnapshot.Encoding : (Encoding)_requestedEncoding.Clone();
    }

    public Task SaveContentToFileAndUpdateEditorStateAsync(StorageFile file)
    {
        return RunDocumentOperationAsync(cancellation => SaveAndUpdateEditorStateAsync(file, cancellation));
    }

    private async Task SaveAndUpdateEditorStateAsync(StorageFile file, CancellationToken cancellationToken)
    {
        if (Mode == TextEditorMode.DiffPreview) CloseSideBySideDiffViewer();
        var version = TextEditorCore.ContentVersion;
        DocumentBaseline savedBaseline = null;
        var encoding = GetEncoding();
        var lineEnding = GetLineEnding();
        try
        {
            DocumentFileWriteResult result;
            using (var reader = TextEditorCore.AcquireDocumentReader())
            {
                result = await DocumentFileWriter.WriteAsync(file, async destination =>
                {
                    // A retriable transaction starts again over the same frozen
                    // native revision, with a fresh destination and baseline.
                    savedBaseline?.Dispose();
                    savedBaseline = null;
                    using (var source = new EditorDocumentStream(reader, ownsReader: false))
                    {
                        savedBaseline = await DocumentBaseline.CreateAsync(canonical =>
                            DocumentTextCodec.CopyFromNativeAsync(source, canonical, destination,
                                encoding, lineEnding, cancellationToken: cancellationToken),
                            DocumentOwnerId, cancellationToken);
                    }
                }, cancellationToken);
                if (!_disposed)
                    await TryRotateRecoveryJournalAsync(savedBaseline, reader.Sequence);
            }

            long modifiedTime = -1;
            try { modifiedTime = await FileStorage.GetDateModifiedAsync(result.File); }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(TextEditor)}] Saved file time could not be queried: {ex.Message}");
            }
            if (_disposed) return;

            // Encoding/EOL controls remain available during a save. Preserve
            // their latest effective choice, including a return to the old default.
            var currentEncoding = GetEncoding();
            var currentLineEnding = GetLineEnding();
            var previous = LastSavedSnapshot;
            // The saved snapshot exists only once its file has committed.
            LastSavedSnapshot = new DocumentSnapshot(savedBaseline, encoding, lineEnding, modifiedTime);
            previous.Dispose();
            FileModificationState = FileModificationState.Untouched;
            EditingFile = result.File;
            _requiresSaveAs = false;
            _requestedEncoding = EncodingCatalog.Equals(currentEncoding, encoding) ? null : currentEncoding;
            RequestedLineEnding = currentLineEnding == lineEnding ? (LineEnding?)null : currentLineEnding;
            if (version == TextEditorCore.ContentVersion) TextEditorCore.MarkSaved();
            else TextEditorCore.MarkModified();
            IsModified = !NoChangesSinceLastSaved();
            FileSaved?.Invoke(this, EventArgs.Empty);
            StartCheckingFileStatusPeriodically();
            if (result.ProviderError != null)
            {
                NotificationCenter.Instance.PostNotification(
                    _resourceLoader.GetString("TextEditor_NotificationMsg_CloudSyncPending"), 5000);
            }
        }
        finally
        {
            // The adopted snapshot holds its own lease; this releases the creation lease.
            savedBaseline?.Dispose();
        }
    }
}
