// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Validation;

namespace Notepads.UnitTests;

public sealed class RecoverySelectorTests
{
    [Fact]
    public void IndentationOverrideRejectsUnsupportedValues()
    {
        var fixture = new Evidence();
        var read = fixture.Rescue(fixture.Editor(12, 5));
        foreach (var value in new[] { 0, -1, 2, 4, 8 })
        {
            read.Record.Editor.StateMetaData.IndentationOverride = value;
            _ = RecoveryRecordCodec.Encode(read.Record, read.Address);
        }
        foreach (var value in new[] { int.MinValue, -2, 1, 3, 256, int.MaxValue })
        {
            read.Record.Editor.StateMetaData.IndentationOverride = value;
            Assert.Throws<System.IO.InvalidDataException>(() => RecoveryRecordCodec.Encode(read.Record, read.Address));
        }
    }

    [Fact]
    public void ContentTitleMetadataMustBeBoundedNormalizedAndAutomatic()
    {
        var fixture = new Evidence();
        var read = fixture.Rescue(fixture.Editor(12, 5));
        var metadata = read.Record.Editor.StateMetaData;
        metadata.UseContentBasedTitle = true;
        metadata.ContentTitle = "draft 中文 😀";
        _ = RecoveryRecordCodec.Encode(read.Record, read.Address);
        foreach (var invalid in new[] { "", "title\nsecond line", "\u202etitle", " leading space", new string('a', 161) })
        {
            metadata.ContentTitle = invalid;
            Assert.Throws<System.IO.InvalidDataException>(() => RecoveryRecordCodec.Encode(read.Record, read.Address));
        }
        metadata.ContentTitle = "valid text";
        metadata.UseContentBasedTitle = false;
        Assert.Throws<System.IO.InvalidDataException>(() => RecoveryRecordCodec.Encode(read.Record, read.Address));
    }

    [Fact]
    public void CheckpointAuthorityAndProtection()
    {
        var fixture = new Evidence();
        var first = fixture.Editor(12, 10);
        var second = fixture.Editor(11, 20);
        var committed = fixture.Checkpoint(2, [first, second], selected: second.Id, scroll: 31);
        fixture.Checkpoint(1, [fixture.Editor(12, 5)]);
        fixture.Rescue(fixture.Editor(12, 40), 2);
        fixture.Rescue(fixture.Editor(12, 30), 1);
        fixture.Pending(fixture.Editor(12, 50));
        fixture.Adopt(fixture.Editor(12, 60));
        var selection = fixture.Select();
        var decision = selection.Documents.Single(row => row.Id == first.Id);
        Check(selection.Documents.Select(row => row.Id).SequenceEqual(new[] { first.Id, second.Id }),
            "The selected checkpoint controls visible order, regardless of GUID order.");
        Check(decision.Membership == RecoveryMembership.Owned && decision.DescriptorSha256 == first.ComputeSha256(),
            "A newer rescue or adoption cannot replace a visible checkpoint member.");
        Check(decision.ProtectedNewer.Select(row => row.CaptureRevision).SequenceEqual(new long[] { 60, 50, 40, 30 }) &&
            decision.ConsumptionCutoff == 60, "Newer unselected versions and the consumption cutoff are decided together.");
        Check(selection.SelectedEditorId == second.Id && selection.TabScrollViewerHorizontalOffset == 31 &&
            selection.SelectedCheckpointAddress == committed.Address &&
            selection.SelectedCheckpointSha256 == committed.PayloadSha256,
            "A decision preserves selected-tab, scroll and exact checkpoint provenance.");
        Check(decision.Fallbacks.First().CaptureRevision == 5 &&
            decision.Fallbacks.All(row => row.DescriptorSha256 != decision.DescriptorSha256),
            "An older checkpoint remains first fallback and the exact primary is never replayed twice.");
        var offered = fixture.Editor(12, 70);
        fixture.Offer(offered);
        var afterOffer = fixture.Select().Documents.Single(row => row.Id == first.Id);
        Check(afterOffer.Membership == RecoveryMembership.Owned && afterOffer.DescriptorSha256 == offered.ComputeSha256() &&
            afterOffer.Fallbacks.First().DescriptorSha256 == first.ComputeSha256(),
            "A strictly newer offer can supersede a visible member while retaining the selected checkpoint as fallback.");
        CheckPermutations(fixture, "checkpoint authority with a newer offer");
    }

    [Fact]
    public void ReceiverHistory()
    {
        foreach (var offerRevision in new long[] { 20, 30, 40 })
        {
            var fixture = new Evidence();
            var original = fixture.Editor(11, 10);
            fixture.Receipt(original);
            var receipt = fixture.Select();
            Check(receipt.Documents.Single().DescriptorSha256 == original.ComputeSha256() &&
                !receipt.Documents.Single().ImportFromInactive,
                "A bare unacknowledged receipt is available to its owner but suppressed from inactive import.");
            var edited = fixture.Editor(11, 30);
            edited.EditingFileName = "receiver-edited-日本語.txt";
            fixture.Rescue(edited);
            CheckRescue(fixture.Select(), edited, "A receiver's independent edited rescue wins over its receipt.");
            var checkpoint = fixture.Checkpoint(1, [], [edited]);
            CheckRescue(fixture.Select(), edited, "A pending-only checkpoint preserves rescue membership.");
            fixture.Snapshot.Rescues.Clear();
            var afterPruning = fixture.Select();
            CheckRescue(afterPruning, edited, "Pruning standalone rescue metadata cannot erase a checkpoint-carried rescue.");
            Check(afterPruning.Documents.Single().RescueRootAddress == checkpoint.Address,
                "A pending-only candidate carries the selected checkpoint as its exact rescue root.");
            var offered = fixture.Editor(11, offerRevision);
            fixture.Offer(offered);
            var afterOffer = fixture.Select().Documents.Single();
            Check(afterOffer.DescriptorSha256 == (offerRevision > 30 ? offered : edited).ComputeSha256() &&
                afterOffer.Membership == (offerRevision > 30 ? RecoveryMembership.Owned : RecoveryMembership.Rescue),
                "An older or equal abandoned offer cannot mask edited pending content; a newer offer can replace it.");
            Check(afterOffer.ImportFromInactive && afterOffer.ConsumptionCutoff == Math.Max(30, offerRevision),
                "Receiver edits remain importable and consumption includes every observed transfer capture.");
            if (offerRevision > 30)
                Check(afterOffer.Fallbacks.Any(row => row.DescriptorSha256 == edited.ComputeSha256()),
                    "The checkpoint-carried pending version remains fallback when a newer offer wins.");
            Check(Signature(fixture.Select()) == Signature(fixture.Restart().Select()),
                "Reconstructing the snapshot on restart produces the same complete decision.");
            CheckPermutations(fixture, "receipt, edited rescue, pending checkpoint, pruning, abandoned offer");
        }

        foreach (var checkpointBackup in new[] { false, true })
        {
            var fixture = new Evidence();
            var descriptor = fixture.Editor(11, 10);
            fixture.Receipt(descriptor);
            if (checkpointBackup) fixture.Checkpoint(1, [Copy(descriptor)]);
            else fixture.Rescue(Copy(descriptor));
            var decision = fixture.Select().Documents.Single();
            Check(decision.ImportFromInactive && decision.Fallbacks.Count == 0,
                "An independently written identical backup establishes receiver history before descriptor hash deduplication.");
        }

        var acknowledged = new Evidence();
        var target = acknowledged.Editor(11, 10);
        var receiptRoot = acknowledged.Receipt(target);
        acknowledged.AcknowledgeReceipt(receiptRoot);
        Check(acknowledged.Select().Documents.Single().ImportFromInactive,
            "A source acknowledgement makes an otherwise untouched receipt importable.");

        var uncertain = new Evidence();
        uncertain.Receipt(uncertain.Editor(11, 10));
        uncertain.Damaged(RecoveryAreaKind.Checkpoints, RecoveryReadState.Corrupt, 2);
        var own = uncertain.Select();
        Check(own.Documents.Single().ImportFromInactive && own.Outcome == SessionRecoveryOutcome.OlderCheckpoint &&
            own.InactiveOutcome == SessionRecoveryOutcome.Partial && own.InactiveAttentionReason != null,
            "Unknown independent history keeps receipt content while degrading inactive import separately from own-scope recovery.");
        Check(own.Outcome != own.InactiveOutcome, "Inactive projection never mutates the original authority verdict.");
    }

    [Fact]
    public void DurableMembership()
    {
        foreach (var kind in new[] { RecoveryRecordKind.AdoptLegacy, RecoveryRecordKind.AdoptInactive })
        foreach (var rescueRevision in new long[] { 10, 20 })
        {
            var fixture = new Evidence();
            var adopted = fixture.Editor(11, 10);
            adopted.EditingFileName = "adopted.txt";
            fixture.Adopt(adopted, kind);
            var rescued = fixture.Editor(11, rescueRevision);
            rescued.EditingFileName = "latest-日本語.txt";
            rescued.EditingFilePath = @"C:\selected\latest-日本語.txt";
            rescued.EditingFileFutureAccessToken = "latest-file-binding";
            fixture.Rescue(rescued);
            var decision = fixture.Select().Documents.Single();
            var expected = rescueRevision > 10 ? rescued : adopted;
            Check(decision.Membership == RecoveryMembership.Owned && decision.Id == adopted.Id &&
                decision.DescriptorSha256 == expected.ComputeSha256(),
                "Durable adoption owns the incarnation; only a strictly newer rescue replaces its content.");
            Check(decision.Descriptor.EditingFileName == expected.EditingFileName &&
                decision.Descriptor.EditingFilePath == expected.EditingFilePath &&
                decision.Descriptor.EditingFileFutureAccessToken == expected.EditingFileFutureAccessToken,
                "The chosen content keeps its actual file binding without changing owned identity.");
            CheckPermutations(fixture, "durable adoption with independent rescue content");
        }
    }

    [Fact]
    public void FallbackAndLifecycle()
    {
        var fixture = new Evidence();
        var latest = fixture.Editor(11, 30);
        fixture.Checkpoint(3, [latest]);
        fixture.Checkpoint(2, [fixture.Editor(11, 20)]);
        fixture.Checkpoint(1, [fixture.Editor(11, 10)]);
        fixture.Rescue(Copy(latest), 2);
        fixture.Rescue(fixture.Editor(11, 25), 1);
        fixture.Adopt(fixture.Editor(11, 5));
        fixture.Receipt(fixture.Editor(11, 15));
        var decision = fixture.Select().Documents.Single();
        Check(decision.Fallbacks.Select(row => row.CaptureRevision).SequenceEqual(new long[] { 20, 10, 25, 15, 5 }),
            "Fallback visits checkpoints, rescue history, then durable target roots in their specified order.");
        Check(decision.Fallbacks.Select(row => row.DescriptorSha256).Distinct().Count() == decision.Fallbacks.Count &&
            decision.Fallbacks.All(row => row.DescriptorSha256 != latest.ComputeSha256()),
            "Fallback removes exact duplicates, including a duplicated primary rescue.");
        CheckPermutations(fixture, "checkpoint/rescue/durable fallback ordering");

        var closed = fixture.Restart();
        closed.Close(latest.Id);
        Check(closed.Select().Documents.Count == 0, "Closed suppresses membership and every fallback for that incarnation.");
        var consumed = fixture.Restart();
        consumed.Consume(latest.Id, 30);
        Check(consumed.Select().Documents.Count == 0, "Inactive consumption suppresses content at or below its cutoff.");
        consumed.Snapshot.Checkpoints.Clear();
        consumed.Rescue(consumed.Editor(11, 31), 3);
        var later = consumed.Select().Documents.Single();
        Check(later.Primary.CaptureRevision == 31 && later.Fallbacks.Count == 0 && later.ConsumptionCutoff == 31,
            "A later capture survives consumption without resurrecting earlier fallback.");
        var moved = fixture.Restart();
        moved.Move(latest, 30);
        Check(moved.Select().Documents.Count == 0, "A source move suppresses the exact source incarnation through its removal revision.");
        moved.Snapshot.Checkpoints.Clear();
        moved.Rescue(moved.Editor(11, 35), 3);
        Check(moved.Select().Documents.Single().Primary.CaptureRevision == 35 &&
            moved.Select().Documents.Single().Fallbacks.Count == 0,
            "A later saved capture survives an earlier transfer move boundary.");
        var reset = fixture.Restart();
        reset.Snapshot.Stamp.EpochId = Id(80);
        Check(reset.Select().Documents.Count == 0, "A new epoch cannot select old checkpoint, rescue, offer or target fallback content.");

        var pendingOnly = new Evidence();
        pendingOnly.Pending(pendingOnly.Editor(11, 99));
        Check(pendingOnly.Select().Documents.Count == 0, "Pending protection alone never grants visible membership.");
    }

    [Fact]
    public void UnknownAndResetEvidence()
    {
        foreach (var state in new[] { RecoveryReadState.Corrupt, RecoveryReadState.Unreadable, RecoveryReadState.Conflict })
        foreach (var area in new[] { RecoveryAreaKind.Checkpoints, RecoveryAreaKind.Rescue })
        {
            var fixture = new Evidence();
            var damaged = fixture.Damaged(area, state, 1, Id(11));
            var blocked = fixture.Select();
            Check(blocked.Blocked && blocked.Outcome == SessionRecoveryOutcome.Blocked &&
                blocked.InactiveOutcome == SessionRecoveryOutcome.Partial && blocked.AttentionReason != null,
                "Current unreadable or conflicting content is protected rather than treated as an empty workspace.");
            fixture.Snapshot.RetiredContentOperations.Add(damaged.Address.OperationId);
            Check(!fixture.Select().Blocked && fixture.Select().Outcome == SessionRecoveryOutcome.Absent,
                "A successful reset retires exactly the acknowledged damaged operation.");
            fixture.Damaged(area, state, 2, Id(11));
            Check(fixture.Select().Blocked, "Retiring older damage cannot hide newly observed damage in the same directory.");
        }

        var partial = new Evidence();
        partial.Rescue(partial.Editor(11, 10));
        partial.Damaged(RecoveryAreaKind.Rescue, RecoveryReadState.Unreadable, 2, Id(12));
        Check(partial.Select().Documents.Count == 1 && partial.Select().Outcome == SessionRecoveryOutcome.Partial,
            "A healthy rescue subset remains recoverable while unknown incarnations remain protected.");
        var superseded = new Evidence();
        superseded.Damaged(RecoveryAreaKind.Rescue, RecoveryReadState.Corrupt, 1, Id(11));
        superseded.Rescue(superseded.Editor(11, 20), 2);
        Check(superseded.Select().Outcome == SessionRecoveryOutcome.Rescue,
            "A later verified rescue supersedes earlier damaged history only in its own document directory.");
        superseded.Damaged(RecoveryAreaKind.Rescue, RecoveryReadState.Corrupt, 3, Id(11));
        Check(superseded.Select().Outcome == SessionRecoveryOutcome.Partial,
            "Damage after the newest verified rescue cannot be silently discarded.");
        var rollback = new Evidence();
        rollback.Checkpoint(1, [rollback.Editor(11, 10)]);
        rollback.Damaged(RecoveryAreaKind.Checkpoints, RecoveryReadState.Corrupt, 2);
        Check(rollback.Select().Outcome == SessionRecoveryOutcome.OlderCheckpoint,
            "A damaged latest checkpoint falls back to verified older metadata with an explicit outcome.");
    }

    [Fact]
    public void TransferStampNormalization()
    {
        var receiver = new Evidence();
        var committed = receiver.Editor(11, 10);
        receiver.Checkpoint(1, [committed]);
        var independent = receiver.Editor(11, 25);
        var receipt = receiver.Receipt(independent);
        // Transfer content belongs to TargetStamp even when its enclosing record is
        // associated with the source. Source content must not inflate the receiver cutoff.
        receipt.Record.Stamp.ScopeId = receipt.Record.Stamp.EpochId = Id(2);
        receipt.Record.SourceEditor = receiver.Editor(11, 500);
        receipt.Record.SourceEditor.Journal.OwnerId = Id(2);
        var decision = receiver.Select().Documents.Single();
        Check(decision.DescriptorSha256 == committed.ComputeSha256() && decision.ConsumptionCutoff == 25 &&
            decision.ProtectedNewer.Single().DescriptorSha256 == independent.ComputeSha256(),
            "Transfer receipt content uses TargetStamp and excludes foreign source content from cutoff and protection.");
        Check(decision.Fallbacks.Single().DescriptorSha256 == independent.ComputeSha256(),
            "A receipt with a distinct enclosing stamp remains eligible target fallback.");

        var source = new Evidence();
        var sourceCapture = source.Editor(11, 10);
        source.Rescue(sourceCapture);
        source.Move(sourceCapture, 20);
        source.Snapshot.GlobalRecords.Single().Record.Stamp.ScopeId = Id(2);
        source.Snapshot.GlobalRecords.Single().Record.Stamp.EpochId = Id(2);
        Check(source.Select().Documents.Count == 0,
            "SourceMoved applies SourceStamp and its removal cutoff independently of the enclosing record stamp.");
        source.Rescue(source.Editor(11, 21), 2);
        Check(source.Select().Documents.Single().ConsumptionCutoff == 21 &&
            source.Select().Documents.Single().Fallbacks.Count == 0,
            "A post-move capture survives while the transferred predecessor stays excluded from fallback.");
    }

    [Fact]
    public void DeterministicTies()
    {
        var fixture = new Evidence();
        var visible = fixture.Editor(14, 10);
        fixture.Checkpoint(1, [visible]);
        var offerA = fixture.Editor(12, 20);
        offerA.EditingFileName = "first-root.txt";
        var offerB = fixture.Editor(12, 20);
        offerB.EditingFileName = "second-root.txt";
        fixture.Offer(offerA, root: Id(100), ordinal: 1);
        fixture.Offer(offerB, root: Id(101), ordinal: 99);
        fixture.Adopt(fixture.Editor(11, 10));
        fixture.Rescue(fixture.Editor(11, 25), 1);
        fixture.Offer(fixture.Editor(13, 20));
        var rescueA = fixture.Editor(14, 30);
        var rescueB = fixture.Editor(14, 30);
        rescueB.EditingFileName = "same-revision-independent-copy.txt";
        fixture.Rescue(rescueA, 1);
        fixture.Rescue(rescueB, 2);
        var selected = fixture.Select();
        Check(selected.Documents.Select(row => row.Id).SequenceEqual(new[] { Id(14), Id(11), Id(12), Id(13) }),
            "Visible checkpoint order is followed by stable ordering of additional owned members.");
        Check(selected.Documents.Single(row => row.Id == Id(12)).DescriptorSha256 == offerA.ComputeSha256(),
            "Equal offers from unrelated directories use stable provenance, not incomparable ordinals.");
        Check(selected.Documents.Single(row => row.Id == Id(14)).ProtectedNewer.Count == 2,
            "Distinct descriptors with equal newer captures are both protected.");
        CheckPermutations(fixture, "equal offers, fallback and newer-protection ordering");

        var rescues = new Evidence();
        rescues.Rescue(rescues.Editor(13, 10));
        rescues.Rescue(rescues.Editor(11, 10));
        rescues.Adopt(rescues.Editor(12, 10));
        Check(rescues.Select().Documents.Select(row => row.Id).SequenceEqual(new[] { Id(12), Id(11), Id(13) }),
            "Additional owned members precede stable rescue order without depending on directory enumeration.");
        CheckPermutations(rescues, "standalone rescue and owned member ordering");

        var sameCapture = new Evidence();
        var earlier = sameCapture.Editor(11, 20);
        earlier.EditingFileName = "earlier-binding.txt";
        var later = sameCapture.Editor(11, 20);
        later.EditingFileName = "later-binding.txt";
        sameCapture.Rescue(earlier, 1);
        sameCapture.Rescue(later, 2);
        sameCapture.Offer(sameCapture.Editor(11, 20));
        CheckRescue(sameCapture.Select(), later,
            "Equal capture rescues use their shared directory's publication order and beat an equal outgoing offer.");
        CheckPermutations(sameCapture, "equal rescues and offer in one incarnation");
        sameCapture.Adopt(earlier);
        sameCapture.Adopt(later);
        Check(sameCapture.Select().Documents.Single().Membership == RecoveryMembership.Owned &&
            sameCapture.Select().Documents.Single().DescriptorSha256 == later.ComputeSha256(),
            "Equal durable adoptions use their shared decision directory's order while preserving owned membership.");
        CheckPermutations(sameCapture, "equal durable adoptions, rescues and offer");
    }

    [Fact]
    public void FrozenDecisions()
    {
        var fixture = new Evidence();
        var primary = fixture.Editor(11, 20);
        var fallback = fixture.Editor(11, 10);
        var newer = fixture.Editor(11, 30);
        fixture.Checkpoint(2, [primary], selected: primary.Id, scroll: 23);
        fixture.Checkpoint(1, [fallback]);
        fixture.Rescue(newer);
        var selection = fixture.Select();
        var before = Signature(selection);
        primary.StateMetaData.LanguageOverride = "mutated-input";
        primary.Journal.OwnerId = Id(70);
        primary.SavedBaseline.Sha256 = new string('f', 64);
        fallback.RecoveryBaseline.GenerationId = Id(71);
        newer.EditingFileFutureAccessToken = "mutated-newer-input";
        fixture.Snapshot.Stamp.EpochId = Id(72);
        fixture.Snapshot.Checkpoints.Clear();
        fixture.Snapshot.Rescues.Clear();
        var decision = selection.Documents.Single();
        var returned = decision.Descriptor;
        returned.StateMetaData.RequestedEncoding = "mutated-output";
        returned.Journal.FileName = "mutated.npj";
        returned.RecoveryBaseline.FileName = "mutated.utf8";
        decision.Fallbacks[0].Descriptor.StateMetaData.FontZoomFactor = 99;
        decision.ProtectedNewer[0].Descriptor.CaptureRevision = long.MaxValue;
        selection.Stamp.ScopeId = Id(73);
        Check(Signature(selection) == before, "Input DTOs, nested output copies and returned stamps cannot mutate a frozen selection.");
        Check(decision.Descriptor.ComputeSha256() == decision.DescriptorSha256 &&
            decision.Fallbacks.All(row => row.Descriptor.ComputeSha256() == row.DescriptorSha256) &&
            decision.ProtectedNewer.All(row => row.Descriptor.ComputeSha256() == row.DescriptorSha256),
            "Frozen hashes continue describing the actual nested payload metadata after mutation attempts.");
        Throws<NotSupportedException>(() => ((IList<RecoveryDocumentDecision>)selection.Documents).Clear(),
            "Callers cannot alter the selected document collection.");
        Throws<NotSupportedException>(() => ((IList<RecoveryDescriptor>)decision.Fallbacks).Clear(),
            "Callers cannot alter fallback ordering.");
        Throws<NotSupportedException>(() => ((IList<RecoveryDescriptor>)decision.ProtectedNewer).Clear(),
            "Callers cannot separate newer protection from its frozen cutoff.");

        var fallbackList = new List<RecoveryDescriptor> { new(fallback) };
        var protectedList = new List<RecoveryDescriptor> { new(newer) };
        var frozen = new RecoveryDocumentDecision(RecoveryMembership.Owned, new(primary), fallbackList,
            protectedList, 30, null, null, true);
        fallbackList.Clear(); protectedList.Clear();
        Check(frozen.Fallbacks.Count == 1 && frozen.ProtectedNewer.Count == 1,
            "A decision copies constructor collections instead of retaining mutable caller-owned lists.");
        var documents = new List<RecoveryDocumentDecision> { frozen };
        var copiedCollection = new RecoverySelection(selection.Stamp, documents, frozen.Id, 0, null, null,
            SessionRecoveryOutcome.Exact, false, null, SessionRecoveryOutcome.Exact, null);
        documents.Clear();
        Check(copiedCollection.Documents.Count == 1,
            "A selection copies its constructor document collection instead of exposing caller-owned membership.");
    }

    [Fact]
    public void SelectionAdmission()
    {
        var fixture = new Evidence();
        Throws<InvalidOperationException>(() => _ = fixture.Snapshot.Selection,
            "A lifecycle-only read cannot be mistaken for an empty content selection.");
        var root = fixture.Checkpoint(1, [fixture.Editor(11, 10)]);
        var selection = fixture.Select();
        Throws<InvalidOperationException>(() => _ = fixture.Snapshot.Selection,
            "Pure selection calculation does not mutate the evidence snapshot.");
        fixture.Snapshot.AttachSelection(selection);
        Check(ReferenceEquals(fixture.Snapshot.Selection, selection) &&
            ReferenceEquals(fixture.Snapshot.SelectedCheckpoint, root),
            "Catalog attachment connects immutable selection to its exact raw checkpoint evidence.");
        Throws<InvalidOperationException>(() => fixture.Snapshot.AttachSelection(selection),
            "An admission snapshot cannot have its content decision silently replaced.");
        var foreign = new Evidence();
        foreign.Snapshot.Stamp.EpochId = Id(99);
        Throws<InvalidOperationException>(() => foreign.Snapshot.AttachSelection(selection),
            "A selection cannot be attached to another epoch.");
        var blocked = new Evidence();
        blocked.Damaged(RecoveryAreaKind.Rescue, RecoveryReadState.Corrupt, 1, Id(11));
        blocked.Snapshot.AttachSelection(blocked.Select());
        Check(blocked.Snapshot.Blocked && blocked.Snapshot.Outcome == SessionRecoveryOutcome.Blocked &&
            blocked.Snapshot.AttentionReason != null, "Effective authority exposes content-selection failure to existing publication guards.");
        Throws<System.IO.InvalidDataException>(() => SessionRecoveryAuthority.RequireCurrent(blocked.Snapshot.Stamp, blocked.Snapshot),
            "A current epoch alone cannot authorize publication over unreadable content.");
    }

    private static void CheckRescue(RecoverySelection selection, TextEditorSessionDataV2 expected, string message) =>
        Check(selection.Documents.Count == 1 && selection.Documents[0].Membership == RecoveryMembership.Rescue &&
            selection.Documents[0].DescriptorSha256 == expected.ComputeSha256() &&
            selection.Documents[0].ImportFromInactive, message);

    private static void CheckPermutations(Evidence fixture, string history)
    {
        var expected = Signature(fixture.Select());
        var reordered = fixture.Restart();
        var groups = new[] { reordered.Snapshot.Checkpoints, reordered.Snapshot.Rescues,
            reordered.Snapshot.Pending, reordered.Snapshot.Decisions, reordered.Snapshot.GlobalRecords };
        VisitGroup(0);

        void VisitGroup(int group)
        {
            if (group == groups.Length)
            {
                Check(Signature(reordered.Select()) == expected, $"Input permutation changed {history}.");
                return;
            }
            Permute(0);
            void Permute(int index)
            {
                var list = groups[group];
                if (index == list.Count) { VisitGroup(group + 1); return; }
                for (var other = index; other < list.Count; other++)
                {
                    (list[index], list[other]) = (list[other], list[index]);
                    Permute(index + 1);
                    (list[index], list[other]) = (list[other], list[index]);
                }
            }
        }
    }

    private static string Signature(RecoverySelection selection) => string.Join("|",
        selection.Stamp.ScopeId, selection.Stamp.EpochId, selection.SelectedEditorId,
        selection.TabScrollViewerHorizontalOffset, selection.SelectedCheckpointAddress, selection.SelectedCheckpointSha256,
        selection.Outcome, selection.Blocked, selection.AttentionReason, selection.InactiveOutcome, selection.InactiveAttentionReason,
        string.Join(";", selection.Documents.Select(row => string.Join(",", row.Id, row.Membership,
            row.DescriptorSha256, row.Descriptor.ComputeSha256(), row.ConsumptionCutoff, row.ImportFromInactive,
            row.RescueRootAddress, row.RescueRootSha256,
            string.Join("/", row.Fallbacks.Select(item => item.DescriptorSha256)),
            string.Join("/", row.ProtectedNewer.Select(item => item.DescriptorSha256))))));

    private static TextEditorSessionDataV2 Copy(TextEditorSessionDataV2 descriptor) => new RecoveryDescriptor(descriptor).Descriptor;
    private static Guid Id(int value) => new(value, 0, 0, new byte[8]);
    private static void Check(bool condition, string message)
    {
        Assert.True(condition, message);
    }
    private static void Throws<T>(Action action, string message) where T : Exception
    {
        try { action(); }
        catch (T) { return; }
        throw new InvalidOperationException(message);
    }

    // These are already verified metadata reads, as handed from the I/O catalog to
    // the selector. No payload files, timestamp ordering or process-global state are involved.
    private sealed class Evidence
    {
        private int operation = 1000;
        public RecoveryScopeSnapshot Snapshot { get; } = new()
        { Stamp = new RecoveryStamp { ScopeId = Id(1), EpochId = Id(1) } };

        public RecoverySelection Select() => SessionRecoverySelector.Select(Snapshot);
        public Evidence Restart()
        {
            var copy = new Evidence { operation = operation };
            copy.Snapshot.Stamp = SessionRecoveryAuthority.CopyStamp(Snapshot.Stamp);
            copy.Snapshot.ControlBlocked = Snapshot.ControlBlocked;
            copy.Snapshot.ControlOutcome = Snapshot.ControlOutcome;
            copy.Snapshot.ControlAttentionReason = Snapshot.ControlAttentionReason;
            copy.Snapshot.Checkpoints.AddRange(Snapshot.Checkpoints);
            copy.Snapshot.Rescues.AddRange(Snapshot.Rescues);
            copy.Snapshot.Pending.AddRange(Snapshot.Pending);
            copy.Snapshot.Decisions.AddRange(Snapshot.Decisions);
            copy.Snapshot.GlobalRecords.AddRange(Snapshot.GlobalRecords);
            copy.Snapshot.RetiredContentOperations.UnionWith(Snapshot.RetiredContentOperations);
            return copy;
        }

        public TextEditorSessionDataV2 Editor(int id, long revision)
        {
            var owner = Snapshot.Stamp.ScopeId;
            var generation = Id(200 + id);
            DocumentBaselineData Baseline() => new()
            {
                OwnerId = owner, GenerationId = generation,
                Sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            };
            return new TextEditorSessionDataV2
            {
                Id = Id(id), CaptureRevision = revision, SavedBaseline = Baseline(), RecoveryBaseline = Baseline(),
                Journal = new DocumentJournalData
                {
                    OwnerId = owner, GenerationId = generation, FileName = $"{owner:N}-{generation:N}.npj",
                    CommittedByteLength = 48, PrefixSha256 = new string('b', 64)
                },
                StateMetaData = new DocumentMetadata { FontZoomFactor = 1, FileNamePlaceholder = "untitled" }
            };
        }

        public RecoveryReadResult Checkpoint(ulong ordinal, TextEditorSessionDataV2[] visible,
            TextEditorSessionDataV2[] pending = null, Guid selected = default, double scroll = 0) => Add(
            Snapshot.Checkpoints, RecoveryAreaKind.Checkpoints, new RecoveryRecord
            {
                Kind = RecoveryRecordKind.Checkpoint,
                Session = new NotepadsSessionDataV2
                {
                    Scope = new SessionScopeData { OwnerId = Snapshot.Stamp.ScopeId,
                        InstanceId = Snapshot.Stamp.ScopeId, Kind = SessionScopeData.Secondary },
                    TextEditors = visible.ToList(), PendingRescues = (pending ?? []).ToList(),
                    SelectedTextEditor = selected, TabScrollViewerHorizontalOffset = scroll
                }
            }, ordinal);

        public RecoveryReadResult Rescue(TextEditorSessionDataV2 editor, ulong ordinal = 1) => Add(
            Snapshot.Rescues, RecoveryAreaKind.Rescue,
            new RecoveryRecord { Kind = RecoveryRecordKind.Rescue, Editor = editor }, ordinal, document: editor.Id);
        public void Pending(TextEditorSessionDataV2 editor) => Add(Snapshot.Pending, RecoveryAreaKind.Pending,
            new RecoveryRecord { Kind = RecoveryRecordKind.Pending, Editor = editor }, 1, document: editor.Id);
        public void Adopt(TextEditorSessionDataV2 editor, RecoveryRecordKind kind = RecoveryRecordKind.AdoptInactive) =>
            Add(Snapshot.Decisions, RecoveryAreaKind.Decisions, new RecoveryRecord { Kind = kind, Editor = editor },
                (ulong)Snapshot.Decisions.Count + 1);
        public RecoveryReadResult Receipt(TextEditorSessionDataV2 editor) => Add(Snapshot.GlobalRecords, RecoveryAreaKind.Transfers,
            new RecoveryRecord
            {
                Kind = RecoveryRecordKind.TransferReceipt, Editor = editor,
                SourceStamp = new RecoveryStamp { ScopeId = Id(2), EpochId = Id(2) },
                TargetStamp = SessionRecoveryAuthority.CopyStamp(Snapshot.Stamp)
            }, 2, root: Id(operation));
        public void Offer(TextEditorSessionDataV2 editor, Guid root = default, ulong ordinal = 1) => Add(
            Snapshot.GlobalRecords, RecoveryAreaKind.Transfers, new RecoveryRecord
            { Kind = RecoveryRecordKind.TransferOffer, Editor = editor, SourceStamp = SessionRecoveryAuthority.CopyStamp(Snapshot.Stamp) },
            ordinal, root: root == Guid.Empty ? Id(operation) : root);
        public void Close(Guid id) => Add(Snapshot.Decisions, RecoveryAreaKind.Decisions,
            new RecoveryRecord { Kind = RecoveryRecordKind.Closed, ClosedEditorId = id }, (ulong)Snapshot.Decisions.Count + 1);
        public void Consume(Guid id, long cutoff) => Add(Snapshot.GlobalRecords, RecoveryAreaKind.Decisions, new RecoveryRecord
        {
            Kind = RecoveryRecordKind.AdoptInactive,
            Consumptions = [new RecoveryConsumption { Kind = RecoveryConsumptionKind.Inactive,
                SourceScopeId = Snapshot.Stamp.ScopeId, SourceEpochId = Snapshot.Stamp.EpochId,
                SourceEditorId = id, SourceCaptureRevision = cutoff }]
        }, 1, owner: Id(2));
        public void Move(TextEditorSessionDataV2 source, long cutoff) => Add(Snapshot.GlobalRecords, RecoveryAreaKind.Transfers,
            new RecoveryRecord { Kind = RecoveryRecordKind.SourceMoved, SourceEditor = source,
                SourceStamp = SessionRecoveryAuthority.CopyStamp(Snapshot.Stamp),
                TargetStamp = new RecoveryStamp { ScopeId = Id(2), EpochId = Id(2) },
                Session = new NotepadsSessionDataV2 { SourceRemovalRevision = cutoff } }, 3, root: Id(operation));
        public void AcknowledgeReceipt(RecoveryReadResult receipt) => Add(Snapshot.GlobalRecords, RecoveryAreaKind.Transfers,
            new RecoveryRecord { Kind = RecoveryRecordKind.SourceMoved,
                SourceStamp = receipt.Record.SourceStamp, TargetStamp = receipt.Record.TargetStamp,
                SourceEditor = new TextEditorSessionDataV2 { Id = Id(90) },
                TargetReceiptOperationId = receipt.Address.OperationId,
                Session = new NotepadsSessionDataV2() }, 3, root: receipt.Address.Area.RootId);

        public RecoveryReadResult Damaged(RecoveryAreaKind kind, RecoveryReadState state, ulong ordinal, Guid document = default)
        {
            var address = new RecoveryAddress(new RecoveryArea(kind, Snapshot.Stamp.ScopeId, document), ordinal, Id(operation++));
            var read = new RecoveryReadResult(state, address);
            (kind == RecoveryAreaKind.Checkpoints ? Snapshot.Checkpoints : Snapshot.Rescues).Add(read);
            return read;
        }

        private RecoveryReadResult Add(List<RecoveryReadResult> list, RecoveryAreaKind kind,
            RecoveryRecord record, ulong ordinal, Guid document = default, Guid root = default, Guid owner = default)
        {
            record.Stamp = SessionRecoveryAuthority.CopyStamp(Snapshot.Stamp);
            if (owner != Guid.Empty) record.Stamp.ScopeId = record.Stamp.EpochId = owner;
            record.Stamp.Ordinal = ordinal;
            record.Stamp.OperationId = Id(operation++);
            var address = new RecoveryAddress(new RecoveryArea(kind,
                kind == RecoveryAreaKind.Transfers ? Guid.Empty : record.Stamp.ScopeId, document, root),
                ordinal, record.Stamp.OperationId);
            var hash = record.Stamp.OperationId.ToString("N") + record.Stamp.OperationId.ToString("N");
            var read = new RecoveryReadResult(RecoveryReadState.Valid, address, record, hash,
                Redundancy: RecoveryRedundancy.Mirrored);
            list.Add(read);
            return read;
        }
    }
}
