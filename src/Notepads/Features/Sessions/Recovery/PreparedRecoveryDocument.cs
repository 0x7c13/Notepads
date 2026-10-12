// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Documents.Text;
using Notepads.Features.Sessions.Contracts;
using Windows.Storage;
using WinUIEditor;

namespace Notepads.Features.Sessions.Recovery;

/// <summary>Owns prepared assets until a native document retains them.</summary>
internal sealed class PreparedRecoveryDocument : IDisposable
{
    public Guid Id { get; set; }
    public RecoveryDocumentSource Source { get; internal set; }
    public RecoveryFileAccessBinding FileAccessBinding { get; internal set; }
    public DocumentMetadata Metadata { get; set; }
    public StorageFile EditingFile { get; set; }
    public string FileNamePlaceholder { get; set; }
    public DocumentSnapshot SavedSnapshot { get; set; }
    public DocumentBaseline RecoveryBaseline { get; set; }
    public DocumentJournal Journal { get; set; }
    public EditorJournalCheckpoint Checkpoint { get; set; }
    public bool TextDirty { get; set; }
    public bool InitializeFromFile { get; set; }

    /// <summary>Reopen a clean tab from its file, as V1 did: the file may have changed since capture.</summary>
    public static async Task<PreparedRecoveryDocument> FromFileAsync(Guid id, DocumentMetadata metadata,
        StorageFile file, Guid ownerId, DocumentLoadOptions defaults, CancellationToken cancellation)
    {
        var options = new DocumentLoadOptions(EncodingCatalog.GetEncodingByName(metadata.LastSavedEncoding),
            defaults.GetInitialEncoding());
        return new PreparedRecoveryDocument
        {
            Id = id,
            Metadata = metadata,
            EditingFile = file,
            FileNamePlaceholder = file.Name,
            SavedSnapshot = await DocumentTextPipeline.DecodeFileAsync(file, options, ownerId, cancellationToken: cancellation),
            InitializeFromFile = true
        };
    }

    public void Dispose()
    {
        try { Checkpoint?.Dispose(); }
        finally
        {
            try { Journal?.Dispose(); }
            finally
            {
                try { RecoveryBaseline?.Dispose(); }
                finally { SavedSnapshot?.Dispose(); }
            }
        }
    }
}

/// <summary>The exact broker resolution retained from preparation; it does not grant token ownership.</summary>
internal sealed class RecoveryFileAccessBinding
{
    internal RecoveryFileAccessBinding(StorageFile file, string token) { File = file; Token = token; }
    public StorageFile File { get; }
    public string Token { get; }
}