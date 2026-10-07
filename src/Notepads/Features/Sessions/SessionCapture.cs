// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using Notepads.Features.Documents;
using Notepads.Features.Sessions.Contracts;
using Windows.Storage;

namespace Notepads.Features.Sessions;

/// <summary>Detached state owned by the caller throughout manifest publication.</summary>
internal sealed class SessionCapture : IDisposable
{
    public List<SessionDocumentCapture> Documents { get; } = new List<SessionDocumentCapture>();
    public Guid? SelectedEditorId { get; set; }
    public double TabScrollOffset { get; set; }
    public RecoveryStamp ExpectedStamp { get; set; }

    public void Dispose()
    {
        foreach (var document in Documents) document.Recovery.Dispose();
    }
}

internal sealed class SessionDocumentCapture
{
    public Guid Id { get; set; }
    public DocumentRecoveryState Recovery { get; set; }
    public StorageFile EditingFile { get; set; }
    public string EditingFileName { get; set; }
    public string EditingFilePath { get; set; }
}
