// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using Notepads.Features.Documents.Contracts;

namespace Notepads.Features.Sessions.Contracts;

internal sealed class DocumentJournalData
{
    public int FormatVersion { get; set; } = 1;

    public Guid OwnerId { get; set; }

    public Guid GenerationId { get; set; }

    public string FileName { get; set; }

    public ulong BaselineSequence { get; set; }

    public ulong CommittedSequence { get; set; }

    public long CommittedByteLength { get; set; }

    public long DocumentByteLength { get; set; }

    public string PrefixSha256 { get; set; }

    public void Validate()
    {
        if (FormatVersion != 1 || GenerationId == Guid.Empty || CommittedSequence < BaselineSequence ||
            CommittedByteLength < 0 || DocumentByteLength < 0 || DocumentByteLength > DocumentLimits.MaximumCanonicalByteLength ||
            PrefixSha256 == null || PrefixSha256.Length != 64 || PrefixSha256.Any(character =>
                !(character >= '0' && character <= '9' || character >= 'a' && character <= 'f' || character >= 'A' && character <= 'F')) ||
            !string.Equals(FileName, OwnerId.ToString("N") + "-" + GenerationId.ToString("N") + ".npj",
                StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("Invalid committed document journal prefix.");
        }
    }
}
