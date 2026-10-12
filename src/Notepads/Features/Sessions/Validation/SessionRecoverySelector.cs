// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Recovery;

namespace Notepads.Features.Sessions.Validation;

/// <summary>Pure content selection over one admission's verified roots and lifecycle evidence.</summary>
internal static class SessionRecoverySelector
{
    public static RecoverySelection Select(RecoveryScopeSnapshot snapshot)
    {
        ArgumentNullException.ThrowIfNull(snapshot);
        var stamp = snapshot.Stamp;
        if (snapshot.ControlBlocked)
            return Blocked(stamp, snapshot.ControlAttentionReason);
        if (snapshot.Checkpoints.Concat(snapshot.Pending).Concat(snapshot.Rescues)
            .Any(read => read.State == RecoveryReadState.Conflict && !snapshot.IsRetiredContent(read)))
            return Blocked(stamp, "Complete recovery mirrors disagree.");

        var own = snapshot.Decisions.Where(read => read.State == RecoveryReadState.Valid)
            .Select(read => read.Record).ToArray();
        var all = snapshot.AllDecisions.ToArray();
        bool Eligible(RecoveryStamp candidateStamp, TextEditorSessionDataV2 editor) =>
            SessionRecoveryAuthority.IsEligible(candidateStamp, editor, stamp, own, all);

        // Checkpoint ordinals share a directory and therefore establish publication order.
        var checkpoints = snapshot.Checkpoints.Where(read => read.State == RecoveryReadState.Valid &&
            SessionRecoveryAuthority.SameEpoch(read.Record.Stamp, stamp))
            .OrderByDescending(read => read.Address.Ordinal)
            .ThenBy(read => read.Address.OperationId).ThenBy(read => read.PayloadSha256, StringComparer.Ordinal).ToArray();
        var checkpoint = checkpoints.FirstOrDefault();
        var checkpointCandidates = checkpoints.SelectMany(read => read.Record.Session.TextEditors
            .Concat(read.Record.Session.PendingRescues).Where(editor => Eligible(read.Record.Stamp, editor))
            .Select(editor => new Candidate(editor, read))).ToArray();
        var visible = checkpoint == null ? Array.Empty<Candidate>() : checkpoint.Record.Session.TextEditors
            .Where(editor => Eligible(checkpoint.Record.Stamp, editor)).Select(editor => new Candidate(editor, checkpoint)).ToArray();
        var standalone = snapshot.Rescues.Where(read => read.State == RecoveryReadState.Valid &&
            Eligible(read.Record.Stamp, read.Record.Editor)).Select(read => new Candidate(read.Record.Editor, read)).ToArray();
        var seeded = checkpoint == null ? standalone : checkpoint.Record.Session.PendingRescues
            .Where(editor => Eligible(checkpoint.Record.Stamp, editor)).Select(editor => new Candidate(editor, checkpoint)).ToArray();
        var durable = snapshot.Decisions.Concat(snapshot.GlobalRecords)
            .Where(read => read.State == RecoveryReadState.Valid && read.Record.Editor != null &&
                read.Record.Kind is RecoveryRecordKind.AdoptLegacy or RecoveryRecordKind.AdoptInactive or RecoveryRecordKind.TransferReceipt &&
                Eligible(read.Record.Kind == RecoveryRecordKind.TransferReceipt ? read.Record.TargetStamp : read.Record.Stamp,
                    read.Record.Editor))
            .Select(read => new Candidate(read.Record.Editor, read)).ToArray();
        var offers = snapshot.GlobalRecords.Where(read => read.State == RecoveryReadState.Valid &&
            read.Record.Kind == RecoveryRecordKind.TransferOffer && Eligible(read.Record.SourceStamp, read.Record.Editor))
            .Select(read => new Candidate(read.Record.Editor, read)).ToArray();
        var unresolvedRescues = UnresolvedRescues(snapshot, own).ToArray();
        var unknownCheckpoints = snapshot.Checkpoints.Any(read => read.State != RecoveryReadState.Valid &&
            !snapshot.IsRetiredContent(read));

        var visibleById = visible.ToDictionary(candidate => candidate.Editor.Id);
        var seededById = seeded.ToLookup(candidate => candidate.Editor.Id);
        var durableById = durable.ToLookup(candidate => candidate.Editor.Id);
        var offersById = offers.ToLookup(candidate => candidate.Editor.Id);
        var standaloneById = standalone.ToLookup(candidate => candidate.Editor.Id);
        var checkpointById = checkpointCandidates.ToLookup(candidate => candidate.Editor.Id);
        var ids = visible.Concat(seeded).Concat(durable).Concat(offers).Select(candidate => candidate.Editor.Id).Distinct();
        var documents = new List<RecoveryDocumentDecision>();
        var adoptedRescueSelected = false;
        var uncertainInactiveHistory = false;

        foreach (var id in ids)
        {
            var membership = RecoveryMembership.Owned;
            visibleById.TryGetValue(id, out var primary);
            var rescue = Newest(seededById[id]);
            if (primary == null)
            {
                var adoption = Newest(durableById[id].Where(candidate => candidate.Root.Record.Kind is
                    RecoveryRecordKind.AdoptLegacy or RecoveryRecordKind.AdoptInactive));
                if (adoption != null)
                {
                    // Adoption owns this incarnation even when its newer rescue supplies
                    // the content and current file binding. The adoption wins equal captures.
                    primary = adoption;
                    if (rescue != null && rescue.Editor.CaptureRevision > adoption.Editor.CaptureRevision)
                    {
                        primary = rescue;
                        adoptedRescueSelected = true;
                    }
                }
                else
                {
                    var receipt = Newest(durableById[id].Where(candidate =>
                        candidate.Root.Record.Kind == RecoveryRecordKind.TransferReceipt));
                    if (receipt != null)
                    {
                        // A receipt can reveal an independent first receiver backup even
                        // when the selected checkpoint omitted that interrupted capture.
                        var independent = Newest(standaloneById[id].Where(candidate =>
                            candidate.Editor.CaptureRevision >= receipt.Editor.CaptureRevision));
                        rescue = Newest(new[] { rescue, independent }.Where(candidate => candidate != null));
                    }
                    if (rescue != null && (receipt == null || rescue.Editor.CaptureRevision >= receipt.Editor.CaptureRevision))
                    {
                        primary = rescue;
                        membership = RecoveryMembership.Rescue;
                    }
                    else primary = receipt;
                }
            }

            var offer = Newest(offersById[id]);
            // Every candidate, including a rescue, keeps equal-revision precedence over
            // an outgoing offer. A strictly newer offer becomes ordinary source membership.
            if (offer != null && (primary == null || offer.Editor.CaptureRevision > primary.Editor.CaptureRevision))
            {
                primary = offer;
                membership = RecoveryMembership.Owned;
            }
            if (primary == null) continue;

            var frozenPrimary = new RecoveryDescriptor(primary.Editor);
            // These observations precede deduplication: an independently written identical
            // checkpoint/rescue is ownership evidence, unlike a bare untouched receipt.
            var receipts = durableById[id].Where(candidate => candidate.Root.Record.Kind == RecoveryRecordKind.TransferReceipt &&
                candidate.Hash == frozenPrimary.DescriptorSha256).ToArray();
            var import = true;
            if (receipts.Length != 0 && !receipts.Any(receipt =>
                checkpointById[id].Concat(standaloneById[id]).Any(candidate =>
                    candidate.Editor.CaptureRevision >= receipt.Editor.CaptureRevision) ||
                snapshot.GlobalRecords.Any(read => read.State == RecoveryReadState.Valid &&
                    read.Record.Kind == RecoveryRecordKind.SourceMoved &&
                    read.Record.TargetReceiptOperationId == receipt.Root.Address.OperationId)))
            {
                if (unknownCheckpoints || unresolvedRescues.Any(read => read.Address.Area.DocumentId == id))
                    uncertainInactiveHistory = true;
                else import = false;
            }

            // Preserve the selected checkpoint as fallback when a newer offer won. Only
            // eligible incarnations enter this sequence; fallback cannot undo lifecycle.
            var fallback = checkpointById[id]
                .Concat(standaloneById[id].OrderByDescending(candidate => candidate.Root.Address.Ordinal)
                    .ThenBy(candidate => candidate, Comparer<Candidate>.Create(CompareProvenance)))
                .Concat(OrderNewest(durableById[id]))
                .DistinctBy(candidate => candidate.Hash)
                .Where(candidate => candidate.Hash != frozenPrimary.DescriptorSha256)
                .Select(candidate => new RecoveryDescriptor(candidate.Editor)).ToArray();
            var observed = snapshot.CurrentDescriptorEvidence(id)
                .Select(evidence => new Candidate(evidence.Editor, evidence.Root)).ToArray();
            var newer = OrderNewest(observed.Where(candidate => candidate.Editor.CaptureRevision > frozenPrimary.CaptureRevision))
                .DistinctBy(candidate => candidate.Hash).Select(candidate => new RecoveryDescriptor(candidate.Editor)).ToArray();
            var cutoff = ConsumptionCutoff(snapshot, id, observed);
            documents.Add(new RecoveryDocumentDecision(membership, frozenPrimary, fallback, newer, cutoff,
                membership == RecoveryMembership.Rescue ? primary.Root.Address : null,
                membership == RecoveryMembership.Rescue ? primary.Root.PayloadSha256 : null, import));
        }

        // Preserve visible checkpoint order. Additional owned members and then rescues use
        // original GUID order; neither directory enumeration nor overlay traversal sets tabs.
        var visibleOrder = visible.Select((candidate, index) => (candidate.Editor.Id, index))
            .ToDictionary(pair => pair.Id, pair => pair.index);
        var ordered = documents.OrderBy(document => visibleOrder.ContainsKey(document.Id) ? 0 :
                document.Membership == RecoveryMembership.Owned ? 1 : 2)
            .ThenBy(document => visibleOrder.TryGetValue(document.Id, out var index) ? index : int.MaxValue)
            .ThenBy(document => document.Id).ToArray();
        var rescued = adoptedRescueSelected || documents.Any(document => document.Membership == RecoveryMembership.Rescue);
        var outcome = snapshot.ControlOutcome;
        if (checkpoint != null)
        {
            var newest = snapshot.Checkpoints.OrderByDescending(read => read.Address.Ordinal)
                .ThenBy(read => read.Address.OperationId).FirstOrDefault();
            outcome = SessionRecoveryAuthority.CombineOutcomes(outcome, newest.Address == checkpoint.Address ?
                checkpoint.Redundancy is RecoveryRedundancy.Single or RecoveryRedundancy.Repaired ?
                    SessionRecoveryOutcome.MirrorRepaired : SessionRecoveryOutcome.Exact : SessionRecoveryOutcome.OlderCheckpoint);
        }
        else
        {
            if (documents.Count != 0) outcome = SessionRecoveryAuthority.CombineOutcomes(outcome, SessionRecoveryOutcome.OlderCheckpoint);
            if (unresolvedRescues.Length != 0 || unknownCheckpoints)
            {
                if (documents.Count == 0) return Blocked(stamp, "No valid checkpoint or rescue descriptor remains.");
                if (unresolvedRescues.Length != 0) outcome = SessionRecoveryOutcome.Partial;
            }
        }
        if (rescued) outcome = SessionRecoveryAuthority.CombineOutcomes(outcome, SessionRecoveryOutcome.Rescue);
        var inactiveOutcome = uncertainInactiveHistory ?
            SessionRecoveryAuthority.CombineOutcomes(outcome, SessionRecoveryOutcome.Partial) : outcome;
        return new RecoverySelection(stamp, ordered, checkpoint?.Record.Session.SelectedTextEditor ?? Guid.Empty,
            checkpoint?.Record.Session.TabScrollViewerHorizontalOffset ?? 0, checkpoint?.Address, checkpoint?.PayloadSha256,
            outcome, false, snapshot.ControlAttentionReason, inactiveOutcome,
            uncertainInactiveHistory ? "An inactive receiver's independent backup history cannot be verified." : snapshot.ControlAttentionReason);
    }

    private static RecoverySelection Blocked(RecoveryStamp stamp, string reason) => new(stamp,
        Array.Empty<RecoveryDocumentDecision>(), Guid.Empty, 0, null, null, SessionRecoveryOutcome.Blocked, true,
        reason, SessionRecoveryOutcome.Partial, reason);

    private static IEnumerable<RecoveryReadResult> UnresolvedRescues(RecoveryScopeSnapshot snapshot,
        RecoveryRecord[] decisions) => snapshot.Rescues.GroupBy(read => read.Address.Area).SelectMany(area =>
        {
            var latestVerified = area.Where(read => read.State == RecoveryReadState.Valid)
                .Select(read => read.Address.Ordinal).DefaultIfEmpty(0UL).Max();
            return area.Where(read => read.State != RecoveryReadState.Valid && read.Address.Ordinal > latestVerified &&
                !snapshot.IsRetiredContent(read) && !SessionRecoveryAuthority.IsClosed(snapshot.Stamp,
                    read.Address.Area.DocumentId, decisions));
        });

    private static long ConsumptionCutoff(RecoveryScopeSnapshot snapshot, Guid id, Candidate[] observed)
    {
        var floor = observed.Select(candidate => candidate.Editor.CaptureRevision).DefaultIfEmpty(0).Max();
        foreach (var record in snapshot.VerifiedRecords)
        {
            foreach (var key in record.Consumptions)
                if (key.Kind == RecoveryConsumptionKind.Inactive && key.SourceScopeId == snapshot.Stamp.ScopeId &&
                    key.SourceEpochId == snapshot.Stamp.EpochId && key.SourceEditorId == id)
                    floor = Math.Max(floor, key.SourceCaptureRevision);
            if (record.Kind == RecoveryRecordKind.SourceMoved && SessionRecoveryAuthority.SameEpoch(record.SourceStamp, snapshot.Stamp) &&
                record.SourceEditor.Id == id)
                floor = Math.Max(floor, record.Session.SourceRemovalRevision);
        }
        return floor;
    }

    private static Candidate Newest(IEnumerable<Candidate> candidates) => OrderNewest(candidates).FirstOrDefault();

    private static IOrderedEnumerable<Candidate> OrderNewest(IEnumerable<Candidate> candidates) => candidates
        .OrderByDescending(candidate => candidate.Editor.CaptureRevision)
        .ThenBy(candidate => candidate, Comparer<Candidate>.Create(CompareProvenance));

    // Ordinals are comparable only inside one directory. Across directories, immutable
    // area identity breaks equal-capture ties; operation and payload hashes finish ties.
    private static int CompareProvenance(Candidate left, Candidate right)
    {
        var a = left.Root.Address;
        var b = right.Root.Address;
        var compare = a.Area.Kind.CompareTo(b.Area.Kind);
        if (compare == 0) compare = a.Area.ScopeId.CompareTo(b.Area.ScopeId);
        if (compare == 0) compare = a.Area.DocumentId.CompareTo(b.Area.DocumentId);
        if (compare == 0) compare = a.Area.RootId.CompareTo(b.Area.RootId);
        if (compare == 0) compare = b.Ordinal.CompareTo(a.Ordinal);
        if (compare == 0) compare = a.OperationId.CompareTo(b.OperationId);
        if (compare == 0) compare = string.Compare(left.Root.PayloadSha256, right.Root.PayloadSha256, StringComparison.Ordinal);
        if (compare == 0) compare = string.Compare(left.Hash, right.Hash, StringComparison.Ordinal);
        return compare;
    }

    private sealed class Candidate(TextEditorSessionDataV2 editor, RecoveryReadResult root)
    {
        public TextEditorSessionDataV2 Editor { get; } = editor;
        public RecoveryReadResult Root { get; } = root;
        public string Hash { get; } = editor.ComputeSha256();
    }
}
