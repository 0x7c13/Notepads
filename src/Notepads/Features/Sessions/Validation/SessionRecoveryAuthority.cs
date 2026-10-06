// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.Features.Sessions.Validation;

/// <summary>Pure lifecycle interpretation shared by startup, transfer admission and collection.</summary>
internal static class SessionRecoveryAuthority
{
    public static RecoveryStamp CopyStamp(RecoveryStamp stamp) => stamp == null ? null : new RecoveryStamp
    {
        ScopeId = stamp.ScopeId,
        EpochId = stamp.EpochId,
        OperationId = stamp.OperationId,
        Ordinal = stamp.Ordinal
    };

    public static bool SameEpoch(RecoveryStamp left, RecoveryStamp right) => left != null && right != null &&
        left.ScopeId == right.ScopeId && left.EpochId == right.EpochId;

    public static SessionRecoveryOutcome CombineOutcomes(SessionRecoveryOutcome left, SessionRecoveryOutcome right)
    {
        static int Severity(SessionRecoveryOutcome outcome) => outcome switch
        {
            SessionRecoveryOutcome.Blocked => 6,
            SessionRecoveryOutcome.Partial => 5,
            SessionRecoveryOutcome.Rescue => 4,
            SessionRecoveryOutcome.OlderCheckpoint => 3,
            SessionRecoveryOutcome.MirrorRepaired => 2,
            SessionRecoveryOutcome.Exact => 1,
            _ => 0
        };
        return Severity(right) > Severity(left) ? right : left;
    }

    public static long GetCaptureRevisionFloor(IEnumerable<RecoveryRecord> records)
    {
        long floor = 0;
        foreach (var record in records)
        {
            floor = Math.Max(floor, record.Editor?.CaptureRevision ?? 0);
            floor = Math.Max(floor, record.SourceEditor?.CaptureRevision ?? 0);
            floor = Math.Max(floor, record.Session?.AcknowledgedSource?.CaptureRevision ?? 0);
            floor = Math.Max(floor, record.Session?.SourceRemovalRevision ?? 0);
            if (record.Session != null)
                foreach (var editor in record.Session.TextEditors) floor = Math.Max(floor, editor.CaptureRevision);
            foreach (var key in record.Consumptions) floor = Math.Max(floor, key.SourceCaptureRevision);
        }
        return floor;
    }

    public static RecoveryStamp ResolveEpoch(Guid scopeId, IEnumerable<RecoveryRecord> decisions)
    {
        var epoch = scopeId;
        foreach (var reset in decisions.Where(record => record.Kind == RecoveryRecordKind.Reset)
            .OrderBy(record => record.Stamp.Ordinal))
        {
            if (reset.Stamp.ScopeId != scopeId || reset.PreviousEpochId != epoch || reset.Stamp.EpochId == epoch)
                throw new InvalidDataException("The recovery reset chain is inconsistent.");
            epoch = reset.Stamp.EpochId;
        }
        return new RecoveryStamp { ScopeId = scopeId, EpochId = epoch };
    }

    public static bool IsClosed(RecoveryStamp stamp, Guid editorId, IEnumerable<RecoveryRecord> decisions) =>
        decisions.Any(record => record.Kind == RecoveryRecordKind.Closed &&
            SameEpoch(stamp, record.Stamp) && record.ClosedEditorId == editorId);

    public static bool IsLegacyConsumed(string fingerprint, Guid editorId, IEnumerable<RecoveryRecord> decisions) =>
        decisions.Any(record => record.Kind == RecoveryRecordKind.Reset &&
            record.ResetAcceptedLegacySources.Contains(fingerprint, StringComparer.OrdinalIgnoreCase) ||
            record.Consumptions.Any(key => key.Kind == RecoveryConsumptionKind.Legacy &&
                string.Equals(key.LegacyFingerprint, fingerprint, StringComparison.OrdinalIgnoreCase) && key.SourceEditorId == editorId));

    public static bool IsInactiveConsumed(RecoveryStamp stamp, TextEditorSessionDataV2 editor,
        IEnumerable<RecoveryRecord> decisions) => decisions.Any(record => record.Consumptions.Any(key =>
            key.Kind == RecoveryConsumptionKind.Inactive && key.SourceScopeId == stamp.ScopeId &&
            key.SourceEpochId == stamp.EpochId && key.SourceEditorId == editor.Id &&
            editor.CaptureRevision <= key.SourceCaptureRevision));

    public static bool IsMoved(RecoveryStamp stamp, TextEditorSessionDataV2 editor,
        IEnumerable<RecoveryRecord> records)
    {
        foreach (var record in records.Where(record => record.Kind == RecoveryRecordKind.SourceMoved))
        {
            if (!SameEpoch(stamp, record.SourceStamp)) continue;
            var source = record.SourceEditor;
            if (source?.Id != editor.Id) continue;
            // A removal applies only to the frozen source incarnation and cutoff.
            // A later saved/edited state is never consumed just because its ID agrees.
            if (editor.CaptureRevision > record.Session.SourceRemovalRevision) continue;
            return true;
        }
        return false;
    }

    public static bool IsEligible(RecoveryStamp candidateStamp, TextEditorSessionDataV2 editor,
        RecoveryStamp currentStamp, IEnumerable<RecoveryRecord> scopeDecisions,
        IEnumerable<RecoveryRecord> allDecisions) => SameEpoch(candidateStamp, currentStamp) &&
            !IsClosed(candidateStamp, editor.Id, scopeDecisions) &&
            !IsInactiveConsumed(candidateStamp, editor, allDecisions) &&
            !IsMoved(candidateStamp, editor, allDecisions);

    public static void RequireCurrent(RecoveryStamp expected, RecoveryScopeSnapshot current)
    {
        if (current.Blocked || !SameEpoch(expected, current.Stamp))
            throw new InvalidDataException("The recovery workspace changed or requires attention; the old operation cannot publish.");
    }
}

/// <summary>A catalog view owns no document streams and never turns content fallback into lifecycle rollback.</summary>
internal sealed class RecoveryScopeSnapshot
{
    public RecoveryStamp Stamp { get; set; }
    public bool Blocked { get; set; }
    public SessionRecoveryOutcome Outcome { get; set; } = SessionRecoveryOutcome.Absent;
    public List<RecoveryReadResult> Checkpoints { get; } = [];
    public List<RecoveryReadResult> Rescues { get; } = [];
    public List<RecoveryReadResult> Decisions { get; } = [];
    public List<RecoveryReadResult> Pending { get; } = [];
    public List<RecoveryReadResult> GlobalRecords { get; } = [];
    public IEnumerable<RecoveryRecord> AllDecisions => Decisions.Concat(GlobalRecords)
        .Where(read => read.State == RecoveryReadState.Valid).Select(read => read.Record);
    public IEnumerable<RecoveryRecord> VerifiedRecords => Checkpoints.Concat(Rescues).Concat(Pending)
        .Concat(Decisions).Concat(GlobalRecords).Where(read => read.State == RecoveryReadState.Valid).Select(read => read.Record);
    public NotepadsSessionDataV2 Session { get; set; }
    public RecoveryReadResult SelectedCheckpoint { get; set; }
    public bool HasCheckpointEvidence { get; set; }
    public string AttentionReason { get; set; }
    public List<RecoveryReadResult> EligibleRescues { get; } = [];

    public IEnumerable<TextEditorSessionDataV2> CurrentDescriptors(Guid editorId)
    {
        foreach (var record in VerifiedRecords.Where(record => SessionRecoveryAuthority.SameEpoch(record.Stamp, Stamp)))
        {
            if (record.Editor?.Id == editorId && record.Editor.Journal.OwnerId == Stamp.ScopeId) yield return record.Editor;
            if (record.SourceEditor?.Id == editorId && record.SourceEditor.Journal.OwnerId == Stamp.ScopeId) yield return record.SourceEditor;
            if (record.Session != null)
            {
                foreach (var editor in record.Session.TextEditors)
                    if (editor.Id == editorId && editor.Journal.OwnerId == Stamp.ScopeId) yield return editor;
            }
        }
    }

    public long ConsumptionCutoff(Guid editorId)
    {
        var floor = CurrentDescriptors(editorId).Select(editor => editor.CaptureRevision).DefaultIfEmpty(0).Max();
        foreach (var record in VerifiedRecords)
        {
            foreach (var key in record.Consumptions)
            {
                if (key.Kind == RecoveryConsumptionKind.Inactive && key.SourceScopeId == Stamp.ScopeId &&
                    key.SourceEpochId == Stamp.EpochId && key.SourceEditorId == editorId)
                {
                    floor = Math.Max(floor, key.SourceCaptureRevision);
                }
            }

            if (record.Kind == RecoveryRecordKind.SourceMoved && SessionRecoveryAuthority.SameEpoch(record.SourceStamp, Stamp) &&
                record.SourceEditor.Id == editorId)
            {
                floor = Math.Max(floor, record.Session.SourceRemovalRevision);
            }
        }
        return floor;
    }

    public IEnumerable<TextEditorSessionDataV2> EarlierDescriptors(Guid editorId) =>
        Checkpoints.Where(read => read.State == RecoveryReadState.Valid &&
            read.Record.Stamp.EpochId == Stamp.EpochId &&
            (SelectedCheckpoint == null || read.Record.Stamp.Ordinal < SelectedCheckpoint.Record.Stamp.Ordinal))
            .OrderByDescending(read => read.Record.Stamp.Ordinal)
            .SelectMany(read => read.Record.Session.TextEditors).Where(editor => editor.Id == editorId)
            .Concat(Rescues.Where(read => read.State == RecoveryReadState.Valid &&
                read.Record.Stamp.EpochId == Stamp.EpochId && read.Record.Editor.Id == editorId)
                .OrderByDescending(read => read.Record.Stamp.Ordinal).Select(read => read.Record.Editor));
}
