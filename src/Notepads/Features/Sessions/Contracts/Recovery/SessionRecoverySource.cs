// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;

namespace Notepads.Features.Sessions.Contracts.Recovery;

/// <summary>An immutable recovery root retained until the importing session durably adopts its records.</summary>
internal sealed partial class SessionRecoverySource : IDisposable
{
    public SessionRecoverySource(RecoverySelection selection, long captureRevisionFloor,
        IDisposable writerLease, IDisposable readerPin)
    {
        Selection = selection;
        Documents = Array.AsReadOnly(selection.Documents.Where(document => document.ImportFromInactive).ToArray());
        CaptureRevisionFloor = captureRevisionFloor;
        WriterLease = writerLease;
        ReaderPin = readerPin;
    }

    public RecoverySelection Selection { get; }
    public IReadOnlyList<RecoveryDocumentDecision> Documents { get; }
    public RecoveryStamp ExpectedStamp => Selection.Stamp;
    public RecoveryAddress RootAddress => Selection.SelectedCheckpointAddress;
    public string RootSha256 => Selection.SelectedCheckpointSha256;
    public long CaptureRevisionFloor { get; }
    public SessionRecoveryOutcome Outcome => Selection.InactiveOutcome;
    private IDisposable WriterLease { get; set; }
    private IDisposable ReaderPin { get; set; }
    public void Dispose()
    {
        try { ReaderPin?.Dispose(); }
        finally { WriterLease?.Dispose(); ReaderPin = null; WriterLease = null; }
    }
}
