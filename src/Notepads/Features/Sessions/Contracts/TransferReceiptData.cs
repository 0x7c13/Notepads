// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;

namespace Notepads.Features.Sessions.Contracts;

/// <summary>A target-owned recovery generation that acknowledges one exact source transfer.</summary>
internal sealed class TransferReceiptData
{
    public TransferToken Source { get; set; }

    public Guid TargetRootId { get; set; }

    public Guid TargetInstanceId { get; set; }
    public Guid TargetOwnerId { get; set; }
    public Guid TargetEpochId { get; set; }

    public string TargetDescriptorSha256 { get; set; }

    public void Validate()
    {
        if (Source == null) throw new InvalidDataException("A transfer receipt has no source token.");
        Source.Validate();
        if (TargetRootId == Guid.Empty || TargetInstanceId == Guid.Empty || TargetOwnerId == Guid.Empty || TargetEpochId == Guid.Empty ||
            !Sha256Hex.IsValid(TargetDescriptorSha256))
        {
            throw new InvalidDataException("Invalid durable document transfer receipt.");
        }
    }
}
