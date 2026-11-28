// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Runtime.InteropServices;

namespace Notepads.Features.Documents.IO;

internal enum DocumentSaveStage { Coordinate, VerifyVersion, Unverified, Conflict, Superseded, OpenTarget, PrepareSnapshot, WriteTarget, CommitTarget, ConfirmCommit }

internal sealed class DocumentSaveException : IOException
{
    public DocumentSaveStage Stage { get; }
    internal Guid? VerificationId { get; init; }
    internal DocumentFileWriteResult CommittedWrite { get; set; }
    internal Windows.Storage.StorageFile RenamedBinding { get; set; }
    internal string ObservedOriginalSha256 { get; init; }

    internal DocumentSaveException(DocumentSaveStage stage, Exception cause = null)
        : base(cause?.Message ?? "The saved file version could not be confirmed.", cause)
    {
        Stage = stage;
        if (cause != null) HResult = cause.HResult;
    }

    internal static bool IsStorageError(Exception error) =>
        error is IOException or UnauthorizedAccessException or COMException;
}
