// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;

namespace Notepads.Features.Sessions.Contracts;

internal enum RecoveryRecordKind
{
    Checkpoint,
    Rescue,
    Pending,
    Reset,
    Closed,
    AdoptLegacy,
    AdoptInactive,
    TransferOffer,
    TransferReceipt,
    SourceMoved,
    Archive
}

/// <summary>Identity carried inside the checksummed payload, independently of file timestamps.</summary>
internal sealed class RecoveryStamp
{
    public Guid ScopeId { get; set; }
    public Guid EpochId { get; set; }
    public Guid OperationId { get; set; }
    public ulong Ordinal { get; set; }
}

internal enum RecoveryConsumptionKind
{
    Legacy,
    Inactive
}

/// <summary>A source identity consumed by adoption; this is not a foreign asset reference.</summary>
internal sealed class RecoveryConsumption
{
    public RecoveryConsumptionKind Kind { get; set; }
    public string LegacyFingerprint { get; set; }
    public Guid SourceScopeId { get; set; }
    public Guid SourceEpochId { get; set; }
    public Guid SourceEditorId { get; set; }
    public long SourceCaptureRevision { get; set; }
}

/// <summary>Shared metadata payload. The catalog, rather than storage, interprets lifecycle authority.</summary>
internal sealed class RecoveryRecord
{
    public RecoveryRecordKind Kind { get; set; }
    public RecoveryStamp Stamp { get; set; }
    public NotepadsSessionDataV2 Session { get; set; }
    public TextEditorSessionDataV2 Editor { get; set; }
    public TextEditorSessionDataV2 SourceEditor { get; set; }
    public RecoveryStamp SourceStamp { get; set; }
    public RecoveryStamp TargetStamp { get; set; }
    public List<RecoveryConsumption> Consumptions { get; set; } = [];
    public List<string> ResetAcceptedLegacySources { get; set; } = [];
    public Guid PreviousEpochId { get; set; }
    public Guid ClosedEditorId { get; set; }
    public Guid TargetReceiptOperationId { get; set; }
    public string TargetReceiptSha256 { get; set; }
}
