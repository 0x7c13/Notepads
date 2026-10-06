// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;

namespace Notepads.Features.Documents.Contracts;

/// <summary>Own the decoding choices admitted for one load, independently of later preference changes.</summary>
public sealed class DocumentLoadOptions
{
    private readonly Encoding _initialEncoding;

    public DocumentLoadOptions(Encoding explicitEncoding = null, Encoding configuredDefaultEncoding = null,
        DocumentDecodingMode decodingMode = DocumentDecodingMode.ConfiguredDefault)
    {
        if (decodingMode != DocumentDecodingMode.ConfiguredDefault && decodingMode != DocumentDecodingMode.AutoDetect)
            throw new ArgumentOutOfRangeException(nameof(decodingMode));
        var selected = explicitEncoding ?? (decodingMode == DocumentDecodingMode.ConfiguredDefault ?
            configuredDefaultEncoding : null);
        _initialEncoding = selected == null ? null : (Encoding)selected.Clone();
    }

    internal Encoding GetInitialEncoding() => _initialEncoding == null ? null : (Encoding)_initialEncoding.Clone();
}
