// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Windows.Storage;
using WinUIEditor;

namespace Notepads.Features.Documents.Storage;

/// <summary>
/// Owns a local append-only native journal file. Keep the writer lease until
/// StopJournalAsync/CloseAsync finishes and retain it for every prefix reader.
/// Native framing, capture, replay, and checksums stay in WinUIEdit.
/// </summary>
public sealed class DocumentJournal : IDisposable
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

    public static async Task<DocumentJournal> CreateAsync(Guid? ownerId = null, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var owner = ownerId ?? DocumentAssetLease.DefaultOwnerId;
        var generation = Guid.NewGuid();
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            "DocumentJournals", CreationCollisionOption.OpenIfExists);
        var asset = await DocumentAssetLease.CreateNewAsync(folder,
            owner.ToString("N") + "-" + generation.ToString("N") + ".npj", cancellationToken);
        return new DocumentJournal(asset, owner, generation);
    }

    /// <summary>Retain before awaiting native checkpoint validation of this committed file.</summary>
    public static DocumentJournal OpenExisting(StorageFile file, Guid ownerId, Guid generationId)
    {
        if (file == null) throw new ArgumentNullException(nameof(file));
        if (generationId == Guid.Empty || !string.Equals(file.Name,
            ownerId.ToString("N") + "-" + generationId.ToString("N") + ".npj", StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("Invalid existing document journal identity.");
        }

        return new DocumentJournal(DocumentAssetLease.Acquire(file, persistent: true), ownerId, generationId);
    }

    public DocumentJournal Retain() => new(_asset.Retain(), OwnerId, GenerationId);

    public async Task<EditorJournalCheckpoint> OpenCheckpointAsync(ulong baselineSequence,
        ulong committedSequence, long committedByteLength, string prefixSha256, long documentByteLength)
    {
        var expectedDocumentLength = checked((ulong)documentByteLength);
        using (var retained = Retain())
        {
            var checkpoint = await EditorJournalCheckpoint.OpenAsync(retained.File.Path, baselineSequence,
                committedSequence, checked((ulong)committedByteLength), prefixSha256);
            if (expectedDocumentLength != checkpoint.CommittedDocumentByteLength)
            {
                checkpoint.Dispose();
                throw new InvalidDataException("The recovered document length does not match its committed descriptor.");
            }
            return checkpoint;
        }
    }

    /// <summary>Expose only the committed prefix, even while the native writer appends future operations.</summary>
    public Task<Stream> OpenReadPrefixAsync(long committedByteLength, CancellationToken cancellationToken = default) =>
        _asset.OpenReadStreamAsync(committedByteLength, cancellationToken, allowConcurrentWriters: true);

    public void PreserveForRecovery() => _asset.PreserveForRecovery();

    public void Dispose() => _asset.Dispose();

    public Task DisposeAsync() => _asset.DisposeAsync();
}
