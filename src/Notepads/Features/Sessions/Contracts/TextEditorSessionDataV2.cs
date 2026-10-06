// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Security.Cryptography;
using System.Text.Json;
using Notepads.Features.Documents.Contracts;

namespace Notepads.Features.Sessions.Contracts;

internal sealed class TextEditorSessionDataV2
{
    public Guid Id { get; set; }

    public long CaptureRevision { get; set; }

    public DocumentBaselineData SavedBaseline { get; set; }

    public DocumentBaselineData RecoveryBaseline { get; set; }

    public DocumentJournalData Journal { get; set; }

    // Native text dirtiness is independent of the journal sequence and of
    // pending encoding/line-ending requests in StateMetaData.
    public bool TextDirty { get; set; }

    public string EditingFileFutureAccessToken { get; set; }

    public string EditingFileName { get; set; }

    public string EditingFilePath { get; set; }

    public DocumentMetadata StateMetaData { get; set; }

    internal string ComputeSha256()
    {
        return Convert.ToHexStringLower(SHA256.HashData(
            JsonSerializer.SerializeToUtf8Bytes(this, SessionJsonContext.Default.TextEditorSessionDataV2)));
    }
}
