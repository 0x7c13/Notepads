// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Storage;
using WinUIEditor;

namespace Notepads.Features.Documents;

/// <summary>One captured completed document operation and independently owned recovery/file metadata.</summary>
public sealed class DocumentRecoveryState : IDisposable
{
    private readonly DocumentMetadata _metadata;
    private static long _nextCaptureRevision;
    private int _disposed;

    public DocumentRecoveryState(DocumentSnapshot savedSnapshot, DocumentBaseline recoveryBaseline,
        DocumentJournal documentJournal, EditorJournalCheckpoint checkpoint,
        DocumentMetadata metaData, bool textDirty, long documentByteLength, long captureRevision = 0)
    {
        if (savedSnapshot == null) throw new ArgumentNullException(nameof(savedSnapshot));
        if (recoveryBaseline == null) throw new ArgumentNullException(nameof(recoveryBaseline));
        if (documentJournal == null) throw new ArgumentNullException(nameof(documentJournal));
        if (checkpoint == null) throw new ArgumentNullException(nameof(checkpoint));
        if (metaData == null) throw new ArgumentNullException(nameof(metaData));
        if (documentByteLength < 0 || documentByteLength > DocumentBaseline.MaximumByteLength)
            throw new ArgumentOutOfRangeException(nameof(documentByteLength));
        if (checked((ulong)documentByteLength) != checkpoint.CommittedDocumentByteLength)
            throw new InvalidOperationException("The recovery capture is not at a completed document operation boundary.");
        _metadata = metaData.Clone();
        try
        {
            SavedSnapshot = savedSnapshot.Retain();
            RecoveryBaseline = recoveryBaseline.Retain();
            DocumentJournal = documentJournal.Retain();
            Checkpoint = checkpoint.Retain();
            TextDirty = textDirty;
            DocumentByteLength = documentByteLength;
            CaptureRevision = captureRevision > 0 ? captureRevision : NextCaptureRevision();
        }
        catch
        {
            Checkpoint?.Dispose();
            DocumentJournal?.Dispose();
            RecoveryBaseline?.Dispose();
            SavedSnapshot?.Dispose();
            throw;
        }
    }

    public DocumentSnapshot SavedSnapshot { get; }

    public DocumentBaseline RecoveryBaseline { get; }

    public DocumentJournal DocumentJournal { get; }

    public EditorJournalCheckpoint Checkpoint { get; }

    public DocumentMetadata MetaData => _metadata.Clone();

    public bool TextDirty { get; }

    public long DocumentByteLength { get; }

    // Orders captures within one app instance, including metadata-only changes.
    public long CaptureRevision { get; }

    /// <summary>Resume capture ordering above durable records loaded by a new process.</summary>
    public static void AdvanceCaptureRevisionFloor(long revision)
    {
        if (revision < 0 || revision == long.MaxValue) throw new ArgumentOutOfRangeException(nameof(revision));
        long observed;
        do
        {
            observed = Volatile.Read(ref _nextCaptureRevision);
            if (observed >= revision) return;
        }
        while (Interlocked.CompareExchange(ref _nextCaptureRevision, revision, observed) != observed);
    }

    private static long NextCaptureRevision()
    {
        long observed;
        do
        {
            observed = Volatile.Read(ref _nextCaptureRevision);
            if (observed == long.MaxValue) throw new InvalidOperationException("Document capture ordering is exhausted.");
        }
        while (Interlocked.CompareExchange(ref _nextCaptureRevision, observed + 1, observed) != observed);
        return observed + 1;
    }

    public DocumentRecoveryState Retain()
    {
        if (Volatile.Read(ref _disposed) != 0) throw new ObjectDisposedException(nameof(DocumentRecoveryState));
        return new DocumentRecoveryState(SavedSnapshot, RecoveryBaseline, DocumentJournal,
            Checkpoint, _metadata, TextDirty, DocumentByteLength, CaptureRevision);
    }

    /// <summary>Retain recovery assets before any publication can reach its first final rename.</summary>
    public void PreserveForRecovery()
    {
        if (Volatile.Read(ref _disposed) != 0) throw new ObjectDisposedException(nameof(DocumentRecoveryState));
        SavedSnapshot.Baseline.PreserveForRecovery();
        RecoveryBaseline.PreserveForRecovery();
        DocumentJournal.PreserveForRecovery();
        Checkpoint.PreserveForRecovery();
    }

    public void Dispose()
    {
        if (Interlocked.Exchange(ref _disposed, 1) != 0) return;
        try { Checkpoint.Dispose(); }
        finally
        {
            try { DocumentJournal.Dispose(); }
            finally
            {
                try { RecoveryBaseline.Dispose(); }
                finally { SavedSnapshot.Dispose(); }
            }
        }
    }

    public async Task DisposeAsync()
    {
        Dispose();
        await DocumentJournal.DisposeAsync().ConfigureAwait(false);
        await RecoveryBaseline.DisposeAsync().ConfigureAwait(false);
        await SavedSnapshot.DisposeAsync().ConfigureAwait(false);
    }
}
