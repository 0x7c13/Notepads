// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Features.Documents.Contracts;

namespace Notepads.Features.Sessions.Contracts.Legacy;

internal sealed class TextEditorSessionDataV1
{
    public Guid Id { get; set; }

    public string LastSavedBackupFilePath { get; set; }

    public string PendingBackupFilePath { get; set; }

    // Recovery storage is independent of the encoding selected for saving.
    // Older V1 records omit this value and use LastSavedEncoding instead.
    public string BackupEncoding { get; set; }

    public string EditingFileFutureAccessToken { get; set; }

    public string EditingFileName { get; set; }

    public string EditingFilePath { get; set; }

    public DocumentMetadata StateMetaData { get; set; }
}
