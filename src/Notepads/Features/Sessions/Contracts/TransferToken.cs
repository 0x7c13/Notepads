// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;

namespace Notepads.Features.Sessions.Contracts;

/// <summary>Bind a delivered package to one immutable source descriptor and its durable receipt.</summary>
internal sealed class TransferToken
{
    public Guid TransferId { get; set; }

    public Guid SourceInstanceId { get; set; }

    public Guid SourceEditorId { get; set; }

    public Guid SourceOwnerId { get; set; }
    public Guid SourceEpochId { get; set; }

    public Guid Nonce { get; set; }

    public string DescriptorSha256 { get; set; }

    public void Validate()
    {
        if (TransferId == Guid.Empty || SourceInstanceId == Guid.Empty || SourceEditorId == Guid.Empty || SourceOwnerId == Guid.Empty || SourceEpochId == Guid.Empty || Nonce == Guid.Empty ||
            DescriptorSha256 == null || DescriptorSha256.Length != 64 || DescriptorSha256.Any(character =>
                !(character >= '0' && character <= '9' || character >= 'a' && character <= 'f' || character >= 'A' && character <= 'F')))
        {
            throw new InvalidDataException("Invalid document transfer token.");
        }
    }

    public bool Matches(TransferToken other) => other != null && TransferId == other.TransferId &&
        SourceInstanceId == other.SourceInstanceId && SourceEditorId == other.SourceEditorId && SourceOwnerId == other.SourceOwnerId && SourceEpochId == other.SourceEpochId && Nonce == other.Nonce &&
        string.Equals(DescriptorSha256, other.DescriptorSha256, StringComparison.OrdinalIgnoreCase);

    public TransferToken Clone() => new()
    {
        TransferId = TransferId,
        SourceInstanceId = SourceInstanceId,
        SourceEditorId = SourceEditorId,
        SourceOwnerId = SourceOwnerId,
        SourceEpochId = SourceEpochId,
        Nonce = Nonce,
        DescriptorSha256 = DescriptorSha256
    };
}
