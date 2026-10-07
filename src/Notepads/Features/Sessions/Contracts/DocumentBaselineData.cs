// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using Notepads.Features.Documents.Contracts;

namespace Notepads.Features.Sessions.Contracts;

internal sealed class DocumentBaselineData
{
    public Guid OwnerId { get; set; }

    public Guid GenerationId { get; set; }

    public string FileName { get; set; }

    public long ByteLength { get; set; }

    public string Sha256 { get; set; }

    public bool Matches(DocumentBaselineData other) => other != null && OwnerId == other.OwnerId &&
        GenerationId == other.GenerationId && ByteLength == other.ByteLength &&
        string.Equals(FileName, other.FileName, StringComparison.OrdinalIgnoreCase) &&
        string.Equals(Sha256, other.Sha256, StringComparison.OrdinalIgnoreCase);

    public void Validate()
    {
        if (GenerationId == Guid.Empty || ByteLength < 0 || ByteLength > DocumentLimits.MaximumCanonicalByteLength ||
            !Sha256Hex.IsValid(Sha256))
        {
            throw new InvalidDataException("Invalid immutable document baseline identity.");
        }

        if (FileName == null)
        {
            if (ByteLength != 0 || !string.Equals(Sha256,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidDataException("A document baseline without a file must represent empty text.");
            }
        }
        else if (!string.Equals(FileName, OwnerId.ToString("N") + "-" + GenerationId.ToString("N") + ".utf8",
            StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("A document baseline references an invalid owned file name.");
        }
    }
}
