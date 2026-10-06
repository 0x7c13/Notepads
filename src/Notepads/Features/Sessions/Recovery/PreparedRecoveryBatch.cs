// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Sessions.Contracts;
using Windows.Storage;
using WinUIEditor;

namespace Notepads.Features.Sessions.Recovery;

/// <summary>Owns prepared assets until editors retain them or preparation is abandoned.</summary>
internal sealed class PreparedRecoveryBatch : IDisposable
{
    public List<PreparedRecoveryDocument> Documents { get; } = new List<PreparedRecoveryDocument>();
    public Guid? SelectedEditorId { get; set; }
    public double TabScrollOffset { get; set; }
    public RecoveryStamp ExpectedStamp { get; set; }
    public List<IDisposable> Pins { get; } = [];

    public void Dispose()
    {
        try
        {
            foreach (var document in Documents) document.Dispose();
        }
        finally
        {
            foreach (var pin in Pins) pin.Dispose();
            Pins.Clear();
        }
    }
}

internal sealed class PreparedRecoveryDocument : IDisposable
{
    public Guid Id { get; set; }
    public DocumentMetadata Metadata { get; set; }
    public StorageFile EditingFile { get; set; }
    public string FileNamePlaceholder { get; set; }
    public DocumentSnapshot SavedSnapshot { get; set; }
    public DocumentBaseline RecoveryBaseline { get; set; }
    public DocumentJournal Journal { get; set; }
    public EditorJournalCheckpoint Checkpoint { get; set; }
    public bool TextDirty { get; set; }
    public bool InitializeFromFile { get; set; }

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
