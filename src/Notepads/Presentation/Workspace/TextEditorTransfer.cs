// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Transfer;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Controls.TextEditor;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;

namespace Notepads.Presentation.Workspace;

/// <summary>Owns a captured recovery prefix until its receiving window acknowledges the move.</summary>
internal enum TextEditorTransferResult
{
    Cancelled,
    UnverifiedMove,
    Received,
}

[SuppressMessage("Design", "CA1001:Types that own disposable fields should be disposable",
    Justification = "CompleteAsync retires captured assets. Gate/CTS use no wait handles or timers and remain valid for late OS data-provider callbacks to observe completion.")]
internal sealed class TextEditorTransfer
{
    public const string DataFormat = "Notepads.DocumentTransfer.v5";
    public const string InstanceProperty = "NotepadsInstanceId";
    public const string TransferProperty = "NotepadsTransferId";

    private readonly WindowContext _context;
    private readonly RecoveryStamp _expectedStamp;
    private readonly DocumentRecoveryState _recovery;
    private readonly StorageFile _editingFile;
    private readonly string _fileName;
    private readonly string _filePath;
    private readonly Guid _nonce = Guid.NewGuid();
    private readonly SemaphoreSlim _gate = new(1, 1);
    private readonly CancellationTokenSource _cancellation = new();
    private string _serializedData;
    private TransferToken _token;
    private DataPackage _package;
    private Func<bool, Task> _onCompleted;
    private long _sourceRemovalRevision;
    private bool _completed;

    public TextEditorTransfer(ITextEditor editor, WindowContext context)
    {
        _context = context ?? throw new ArgumentNullException(nameof(context));
        Editor = editor ?? throw new ArgumentNullException(nameof(editor));
        Id = Guid.NewGuid();
        _expectedStamp = context.Sessions.CurrentStamp;
        _recovery = editor.CaptureRecoveryState();
        _editingFile = editor.EditingFile;
        _fileName = _editingFile?.Name;
        _filePath = editor.EditingFilePath;
    }

    public Guid Id { get; }

    public ITextEditor Editor { get; }

    public bool HasPublishedData => _serializedData != null;

    public void Populate(DataPackage package, Func<bool, Task> onCompleted)
    {
        _package = package ?? throw new ArgumentNullException(nameof(package));
        _onCompleted = onCompleted ?? throw new ArgumentNullException(nameof(onCompleted));
        package.OperationCompleted += OnOperationCompleted;
        package.RequestedOperation = DataPackageOperation.Move;
        package.Properties.ApplicationName = _context.ApplicationName;
        package.Properties.Add(InstanceProperty, _context.InstanceId.ToString());
        package.Properties.Add(TransferProperty, Id.ToString());
        if (_editingFile != null) package.SetStorageItems(new[] { _editingFile }, readOnly: false);
        package.SetDataProvider(DataFormat, ProvideData);
    }

    private async void OnOperationCompleted(DataPackage sender, OperationCompletedEventArgs args)
    {
        var callback = _onCompleted;
        if (callback == null) return;
        try { await callback(args.Operation == DataPackageOperation.Move); }
        catch (Exception ex) { LoggingService.LogException(ex); }
    }

    private async void ProvideData(DataProviderRequest request)
    {
        var deferral = request.GetDeferral();
        await _gate.WaitAsync();
        try
        {
            if (_completed) return;
            if (_serializedData == null)
            {
                await _recovery.Checkpoint.FlushAsync().AsCompletionTask(_cancellation.Token);
                var descriptor = SessionDocumentStore.CaptureEditor(Editor.Id, _recovery, _fileName, _filePath);
                _recovery.PreserveForRecovery();
                var token = await TransferRecoveryStore.CreateSourceTransferAsync(Id, _context.InstanceId,
                    _nonce, descriptor, _editingFile, _expectedStamp, _cancellation.Token);
                // The receiver may outlive this process. Promote captured assets
                // before publication; deliver the descriptor only after commit.
                _token = token;
                _serializedData = JsonSerializer.Serialize(token, SessionJsonContext.Default.TransferToken);
            }
            request.SetData(_serializedData);
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(TextEditorTransfer)}] Failed to prepare transfer [{Id}]: {ex}");
        }
        finally
        {
            _gate.Release();
            deferral.Complete();
        }
    }

    /// <summary>Do not discard edits or save-setting changes made after the captured revision.</summary>
    public bool IsSourceUnchanged()
    {
        using (var current = Editor.CaptureRecoveryState())
        {
            var metadata = current.MetaData;
            var captured = _recovery.MetaData;
            var unchanged = current.Checkpoint.CommittedSequence == _recovery.Checkpoint.CommittedSequence &&
                current.SavedSnapshot.Baseline.OwnerId == _recovery.SavedSnapshot.Baseline.OwnerId &&
                current.SavedSnapshot.Baseline.GenerationId == _recovery.SavedSnapshot.Baseline.GenerationId &&
                metadata.RequestedEncoding == captured.RequestedEncoding &&
                metadata.RequestedLineEnding == captured.RequestedLineEnding &&
                metadata.LanguageOverride == captured.LanguageOverride &&
                metadata.FileNamePlaceholder == captured.FileNamePlaceholder &&
                metadata.RequiresSaveAs == captured.RequiresSaveAs &&
                metadata.HasEditingFile == captured.HasEditingFile &&
                (Editor.EditingFile == null ? _editingFile == null :
                    _editingFile != null && Editor.EditingFile.IsEqual(_editingFile)) &&
                Editor.EditingFile?.Name == _fileName && Editor.EditingFilePath == _filePath;
            _sourceRemovalRevision = unchanged ? current.CaptureRevision : 0;
            return unchanged;
        }
    }

    public Task<bool> HasReceiptAsync() => _token == null ? Task.FromResult(false) :
        TransferRecoveryStore.HasReceiptAsync(_token);

    public Task MarkSourceAcknowledgedAsync() => _token == null ?
        Task.FromException(new InvalidOperationException("The source transfer has not been published.")) :
        TransferRecoveryStore.MarkSourceAcknowledgedAsync(_token, _sourceRemovalRevision);

    public Task<bool> TryCommitSourceMoveAsync(Func<bool> canRemove, Action removeLogicalSource) =>
        Editor.TryCommitTransferAsync(() => canRemove() && IsSourceUnchanged(),
            MarkSourceAcknowledgedAsync, removeLogicalSource);

    public async Task CompleteAsync(TextEditorTransferResult result, bool sourceIsAlive)
    {
        if (_package != null) _package.OperationCompleted -= OnOperationCompleted;
        _package = null;
        _onCompleted = null;
        if (result != TextEditorTransferResult.Received) _cancellation.Cancel();
        await _gate.WaitAsync();
        try
        {
            if (_completed) return;
            _completed = true;
            // A failed move from an already closed source leaves a recovery
            // root available instead of destroying the only remaining copy.
            if (_token != null && (result == TextEditorTransferResult.Received ||
                result == TextEditorTransferResult.Cancelled && sourceIsAlive))
            {
                await TransferRecoveryStore.DeleteSourceTransferAsync(_token);
            }
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(TextEditorTransfer)}] Failed to retire transfer [{Id}]: {ex}");
        }
        finally
        {
            try { if (_completed) await _recovery.DisposeAsync(); }
            catch (Exception ex) { LoggingService.LogException(ex); }
            finally { _gate.Release(); }
        }
    }
}
