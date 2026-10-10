// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;

namespace Notepads.Features.Sessions.Contracts.Recovery;

internal enum RecoveryMembership { Owned, Rescue }

/// <summary>A frozen descriptor. Each DTO access returns an independent, deep copy.</summary>
internal sealed class RecoveryDescriptor
{
    private readonly TextEditorSessionDataV2 _descriptor;

    public RecoveryDescriptor(TextEditorSessionDataV2 descriptor)
    {
        ArgumentNullException.ThrowIfNull(descriptor);
        _descriptor = Copy(descriptor);
        DescriptorSha256 = _descriptor.ComputeSha256();
    }

    public Guid Id => _descriptor.Id;
    public long CaptureRevision => _descriptor.CaptureRevision;
    public string DescriptorSha256 { get; }
    public TextEditorSessionDataV2 Descriptor => Copy(_descriptor);

    private static TextEditorSessionDataV2 Copy(TextEditorSessionDataV2 descriptor) => JsonSerializer.Deserialize(
        JsonSerializer.SerializeToUtf8Bytes(descriptor, SessionJsonContext.Default.TextEditorSessionDataV2),
        SessionJsonContext.Default.TextEditorSessionDataV2);
}

/// <summary>Membership, replay and consumption evidence for one source incarnation.</summary>
internal sealed class RecoveryDocumentDecision
{
    public RecoveryDocumentDecision(RecoveryMembership membership, RecoveryDescriptor primary,
        IEnumerable<RecoveryDescriptor> fallbacks, IEnumerable<RecoveryDescriptor> protectedNewer,
        long consumptionCutoff, RecoveryAddress rescueRootAddress, string rescueRootSha256,
        bool importFromInactive)
    {
        ArgumentNullException.ThrowIfNull(primary);
        Membership = membership;
        Primary = primary;
        Fallbacks = Array.AsReadOnly(fallbacks.ToArray());
        ProtectedNewer = Array.AsReadOnly(protectedNewer.ToArray());
        ConsumptionCutoff = consumptionCutoff;
        RescueRootAddress = rescueRootAddress;
        RescueRootSha256 = rescueRootSha256;
        ImportFromInactive = importFromInactive;
    }

    public Guid Id => Primary.Id;
    public RecoveryMembership Membership { get; }
    public RecoveryDescriptor Primary { get; }
    public TextEditorSessionDataV2 Descriptor => Primary.Descriptor;
    public string DescriptorSha256 => Primary.DescriptorSha256;
    public IReadOnlyList<RecoveryDescriptor> Fallbacks { get; }
    public IReadOnlyList<RecoveryDescriptor> ProtectedNewer { get; }
    public long ConsumptionCutoff { get; }
    public RecoveryAddress RescueRootAddress { get; }
    public string RescueRootSha256 { get; }
    public bool ImportFromInactive { get; }
}

/// <summary>Immutable interpretation of one admitted, fully read evidence snapshot.</summary>
internal sealed class RecoverySelection
{
    private readonly RecoveryStamp _stamp;

    public RecoverySelection(RecoveryStamp stamp, IEnumerable<RecoveryDocumentDecision> documents,
        Guid selectedEditorId, double tabScrollViewerHorizontalOffset,
        RecoveryAddress selectedCheckpointAddress, string selectedCheckpointSha256,
        SessionRecoveryOutcome outcome, bool blocked, string attentionReason,
        SessionRecoveryOutcome inactiveOutcome, string inactiveAttentionReason)
    {
        ArgumentNullException.ThrowIfNull(stamp);
        _stamp = CopyStamp(stamp);
        Documents = Array.AsReadOnly(documents.ToArray());
        SelectedEditorId = selectedEditorId;
        TabScrollViewerHorizontalOffset = tabScrollViewerHorizontalOffset;
        SelectedCheckpointAddress = selectedCheckpointAddress;
        SelectedCheckpointSha256 = selectedCheckpointSha256;
        Outcome = outcome;
        Blocked = blocked;
        AttentionReason = attentionReason;
        InactiveOutcome = inactiveOutcome;
        InactiveAttentionReason = inactiveAttentionReason;
    }

    public RecoveryStamp Stamp => CopyStamp(_stamp);
    public IReadOnlyList<RecoveryDocumentDecision> Documents { get; }
    public Guid SelectedEditorId { get; }
    public double TabScrollViewerHorizontalOffset { get; }
    public RecoveryAddress SelectedCheckpointAddress { get; }
    public string SelectedCheckpointSha256 { get; }
    public SessionRecoveryOutcome Outcome { get; }
    public bool Blocked { get; }
    public string AttentionReason { get; }
    public SessionRecoveryOutcome InactiveOutcome { get; }
    public string InactiveAttentionReason { get; }

    private static RecoveryStamp CopyStamp(RecoveryStamp stamp) => new()
    {
        ScopeId = stamp.ScopeId, EpochId = stamp.EpochId,
        OperationId = stamp.OperationId, Ordinal = stamp.Ordinal
    };
}
