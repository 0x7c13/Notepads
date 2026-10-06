// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.Features.Sessions.Validation;

internal static class RecoveryRecordValidator
{
    internal static void ValidateAcknowledgement(NotepadsSessionDataV2 session, TransferToken token)
    {
        if (session?.Version != 2 || session.TextEditors == null || session.TextEditors.Count != 0 ||
            session.UnrecoveredLegacyEditors == null || session.UnrecoveredLegacyEditors.Count != 0 ||
            session.TransferSource != null || session.TransferReceipt != null || !token.Matches(session.TransferAcknowledgement) ||
            session.AcknowledgedSource?.Id != token.SourceEditorId || session.AcknowledgedSource.Journal?.OwnerId != token.SourceOwnerId ||
            session.SourceRemovalRevision < session.AcknowledgedSource.CaptureRevision ||
            !string.Equals(HashDescriptor(session.AcknowledgedSource), token.DescriptorSha256, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("The source acknowledgement does not match its transfer.");
        }

        CreateEditorRoot(session.AcknowledgedSource);
    }

    internal static NotepadsSessionDataV2 CreateEditorRoot(TextEditorSessionDataV2 editor)
    {
        if (editor?.StateMetaData == null || editor.Id == Guid.Empty)
            throw new InvalidDataException("A transfer requires a complete editor descriptor.");
        var session = new NotepadsSessionDataV2 { SelectedTextEditor = editor.Id };
        session.TextEditors.Add(editor);
        new SessionRecoveryReferences().Include(session);
        return session;
    }

    internal static TextEditorSessionDataV2 ValidateEditorRoot(NotepadsSessionDataV2 session)
    {
        if (session?.Version != 2 || session.TextEditors == null || session.TextEditors.Count != 1 ||
            session.UnrecoveredLegacyEditors == null || session.UnrecoveredLegacyEditors.Count != 0)
        {
            throw new InvalidDataException("Invalid durable transfer root.");
        }

        var editor = session.TextEditors[0];
        CreateEditorRoot(editor);
        if (session.SelectedTextEditor != editor.Id)
            throw new InvalidDataException("The durable transfer root selects an invalid editor.");
        return editor;
    }

    internal static TextEditorSessionDataV2 ValidateReceipt(NotepadsSessionDataV2 session, TransferToken token)
    {
        var editor = ValidateEditorRoot(session);
        if (session.TransferSource != null || session.TransferAcknowledgement != null || session.TransferReceipt == null)
            throw new InvalidDataException("The transfer has no target receipt.");
        session.TransferReceipt.Validate();
        if (!token.Matches(session.TransferReceipt.Source) ||
            !string.Equals(session.TransferReceipt.TargetDescriptorSha256, HashDescriptor(editor), StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("The durable receipt does not match its transfer or target descriptor.");
        }

        return editor;
    }

    internal static void ValidateIndependentTarget(TextEditorSessionDataV2 source, TextEditorSessionDataV2 target)
    {
        if (target.SavedBaseline.OwnerId != target.Journal.OwnerId || target.RecoveryBaseline.OwnerId != target.Journal.OwnerId ||
            SameIdentity(target.SavedBaseline, source.SavedBaseline) || SameIdentity(target.SavedBaseline, source.RecoveryBaseline) ||
            SameIdentity(target.RecoveryBaseline, source.SavedBaseline) || SameIdentity(target.RecoveryBaseline, source.RecoveryBaseline) ||
            target.Journal.OwnerId == source.Journal.OwnerId && target.Journal.GenerationId == source.Journal.GenerationId ||
            !SameContents(target.SavedBaseline, source.SavedBaseline) || !SameContents(target.RecoveryBaseline, source.RecoveryBaseline) ||
            target.Journal.FormatVersion != source.Journal.FormatVersion || target.Journal.BaselineSequence != source.Journal.BaselineSequence ||
            target.Journal.CommittedSequence != source.Journal.CommittedSequence || target.Journal.CommittedByteLength != source.Journal.CommittedByteLength ||
            target.Journal.DocumentByteLength != source.Journal.DocumentByteLength ||
            !string.Equals(target.Journal.PrefixSha256, source.Journal.PrefixSha256, StringComparison.OrdinalIgnoreCase) ||
            target.TextDirty != source.TextDirty || !string.Equals(target.EditingFileName, source.EditingFileName, StringComparison.Ordinal) ||
            !string.Equals(target.EditingFilePath, source.EditingFilePath, StringComparison.OrdinalIgnoreCase) ||
            !SameDocumentMetadata(target, source))
        {
            throw new InvalidDataException("The transfer target is not an independent, equivalent recovery generation.");
        }
    }

    private static bool SameDocumentMetadata(TextEditorSessionDataV2 target, TextEditorSessionDataV2 source)
    {
        var left = target.StateMetaData;
        var right = source.StateMetaData;
        // Viewport and preview flags are restored after attaching the target
        // tab. Their temporary differences cannot authorize losing save state.
        return string.Equals(left.FileNamePlaceholder, right.FileNamePlaceholder, StringComparison.Ordinal) &&
            string.Equals(left.LanguageOverride, right.LanguageOverride, StringComparison.Ordinal) &&
            string.Equals(left.LastSavedEncoding, right.LastSavedEncoding, StringComparison.OrdinalIgnoreCase) &&
            string.Equals(left.LastSavedLineEnding, right.LastSavedLineEnding, StringComparison.OrdinalIgnoreCase) &&
            left.DateModifiedFileTime == right.DateModifiedFileTime &&
            string.Equals(left.RequestedEncoding, right.RequestedEncoding, StringComparison.OrdinalIgnoreCase) &&
            string.Equals(left.RequestedLineEnding, right.RequestedLineEnding, StringComparison.OrdinalIgnoreCase) &&
            left.HasEditingFile == right.HasEditingFile && left.RequiresSaveAs == right.RequiresSaveAs && left.IsModified == right.IsModified;
    }

    private static bool SameIdentity(DocumentBaselineData left, DocumentBaselineData right) =>
        left.OwnerId == right.OwnerId && left.GenerationId == right.GenerationId;

    private static bool SameContents(DocumentBaselineData left, DocumentBaselineData right) =>
        left.ByteLength == right.ByteLength && string.Equals(left.Sha256, right.Sha256, StringComparison.OrdinalIgnoreCase);

    internal static string HashDescriptor(TextEditorSessionDataV2 editor) => editor.ComputeSha256();

    internal static SessionScopeData GetProvenScope(Guid ownerId, Guid instanceId)
    {
        if (ownerId == SessionScopeData.GetStableOwner()) return new SessionScopeData { OwnerId = ownerId, InstanceId = instanceId, Kind = SessionScopeData.Primary };
        return GetRetiredSecondaryScope(ownerId, instanceId);
    }

    internal static SessionScopeData GetRetiredSecondaryScope(Guid ownerId, Guid instanceId) => ownerId == instanceId && !SessionScopeData.IsReservedOwner(ownerId) ?
        new SessionScopeData { OwnerId = ownerId, InstanceId = instanceId, Kind = SessionScopeData.Secondary } : null;

    internal static void ValidateSessionRecords(NotepadsSessionDataV2 session)
    {
        if (session?.Version != 2 || session.TextEditors == null || session.UnrecoveredLegacyEditors == null ||
            session.TextEditors.Any(editor => editor?.StateMetaData == null || editor.Id == Guid.Empty) ||
            session.UnrecoveredLegacyEditors.Any(editor => editor?.StateMetaData == null || editor.Id == Guid.Empty) ||
            session.TextEditors.Select(editor => editor.Id).Concat(session.UnrecoveredLegacyEditors.Select(editor => editor.Id))
                .Distinct().Count() != session.TextEditors.Count + session.UnrecoveredLegacyEditors.Count)
        {
            throw new InvalidDataException("Invalid inactive session records.");
        }
    }

}
