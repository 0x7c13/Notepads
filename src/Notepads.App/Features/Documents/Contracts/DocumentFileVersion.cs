// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;

namespace Notepads.Features.Documents.Contracts;

/// <summary>A disk revision, including the original encoded bytes, BOM and line endings.</summary>
public readonly record struct DocumentFileVersion(long ModifiedTime, string Sha256, string TargetPath = null)
{
    public static DocumentFileVersion Unknown => new(-1, null);
    public bool IsVerified => ModifiedTime >= 0 && Sha256?.Length == 64 &&
        !Sha256.AsSpan().ContainsAnyExcept("0123456789abcdefABCDEF");
}
