// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using Notepads.Features.Documents;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Windows.Storage;

namespace Notepads.Features.Sessions;

/// <summary>Detached state owned by the caller throughout manifest publication.</summary>
internal sealed partial class SessionCapture : IDisposable
{
    public List<SessionDocumentCapture> Documents { get; } = new List<SessionDocumentCapture>();
    public List<RecoveryDocumentSource> PendingRescues { get; } = [];
    public Guid? SelectedEditorId { get; set; }
    public double TabScrollOffset { get; set; }
    public RecoveryStamp ExpectedStamp { get; set; }

    public void Dispose()
    {
        foreach (var document in Documents) document.Recovery?.Dispose();
    }
}

internal sealed class SessionDocumentCapture
{
    public Guid Id { get; set; }
    public DocumentRecoveryState Recovery { get; set; }
    public RecoveryDocumentSource Source { get; set; }
    public RecoveryFileAccessBinding FileAccessBinding { get; set; }
    public bool IsRetained => Recovery == null && Source != null;
    public StorageFile EditingFile { get; set; }
    public string EditingFileName { get; set; }
    public string EditingFilePath { get; set; }

    public static SessionDocumentCapture Retain(RecoveryDocumentSource source) => new()
    {
        Id = source.Id,
        Source = source
    };
}
