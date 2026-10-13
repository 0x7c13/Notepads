// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Threading;
using Windows.Foundation;
using Windows.Storage;
using WinUIEditor;

namespace Notepads.Features.Documents.Storage;

/// <summary>
/// Owns a local append-only native journal file. Keep the writer lease until
/// StopJournalAsync/CloseAsync finishes and retain it for every prefix reader.
/// Native framing, capture, replay, and checksums stay in WinUIEdit.
/// </summary>
public sealed partial class DocumentJournal : IDisposable
{
    private readonly DocumentAssetLease _asset;

    private DocumentJournal(DocumentAssetLease asset, Guid ownerId, Guid generationId)
    {
        _asset = asset;
        OwnerId = ownerId;
        GenerationId = generationId;
    }

    public Guid OwnerId { get; }

    public Guid GenerationId { get; }

    public StorageFile File => _asset.File;

    public static async Task<DocumentJournal> CreateAsync(Guid ownerId, CancellationToken cancellationToken = default)
    {
        if (ownerId == Guid.Empty) throw new ArgumentException("A document asset requires its owning scope.", nameof(ownerId));
        cancellationToken.ThrowIfCancellationRequested();
        var generation = Guid.NewGuid();
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            "DocumentJournals", CreationCollisionOption.OpenIfExists);
        var asset = await DocumentAssetLease.CreateNewAsync(folder,
            ownerId.ToString("N") + "-" + generation.ToString("N") + ".npj", cancellationToken);
        return new DocumentJournal(asset, ownerId, generation);
    }

    /// <summary>Retain before awaiting native checkpoint validation of this committed file.</summary>
    public static DocumentJournal OpenExisting(StorageFile file, Guid ownerId, Guid generationId)
    {
        ArgumentNullException.ThrowIfNull(file);
        if (generationId == Guid.Empty || !string.Equals(file.Name,
            ownerId.ToString("N") + "-" + generationId.ToString("N") + ".npj", StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("Invalid existing document journal identity.");
        }

        return new DocumentJournal(DocumentAssetLease.Acquire(file, persistent: true), ownerId, generationId);
    }

    public DocumentJournal Retain() => new(_asset.Retain(), OwnerId, GenerationId);

    /// <summary>
    /// Compact once the committed prefix reaches 64 MiB, or 4 MiB and four times the document,
    /// so journal size and restore time follow the document rather than its edit history.
    /// </summary>
    public static bool ShouldCompact(ulong committedByteLength, ulong documentByteLength) =>
        committedByteLength >= 64UL * 1024 * 1024 ||
        (committedByteLength >= 4UL * 1024 * 1024 && committedByteLength / 4 >= documentByteLength);

    public async Task<EditorJournalCheckpoint> OpenCheckpointAsync(ulong baselineSequence,
        ulong committedSequence, long committedByteLength, string prefixSha256, long documentByteLength,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var expectedDocumentLength = checked((ulong)documentByteLength);
        using var retained = Retain();
        var operation = EditorJournalCheckpoint.OpenAsync(retained.File.Path, baselineSequence,
            committedSequence, checked((ulong)committedByteLength), prefixSha256);
        OnCheckpointOpening(operation, cancellationToken);
        // Cancel the native validator, but keep its journal lease until it finishes unwinding.
        var checkpoint = await operation.AsCompletionTask(cancellationToken).ConfigureAwait(false);
        try
        {
            OnCheckpointOpened(checkpoint, cancellationToken);
            cancellationToken.ThrowIfCancellationRequested();
            if (expectedDocumentLength != checkpoint.CommittedDocumentByteLength)
                throw new InvalidDataException("The recovered document length does not match its committed descriptor.");
            return checkpoint;
        }
        catch
        {
            // Native completion can win the cancellation race and return an owned result.
            checkpoint.Dispose();
            throw;
        }
    }

    // Source-linked tests implement these boundaries; production builds erase the calls.
    static partial void OnCheckpointOpening(IAsyncOperation<EditorJournalCheckpoint> operation, CancellationToken cancellationToken);
    static partial void OnCheckpointOpened(EditorJournalCheckpoint checkpoint, CancellationToken cancellationToken);

    public void PreserveForRecovery() => _asset.PreserveForRecovery();

    public void Dispose() => _asset.Dispose();

    public Task DisposeAsync() => _asset.DisposeAsync();
}
