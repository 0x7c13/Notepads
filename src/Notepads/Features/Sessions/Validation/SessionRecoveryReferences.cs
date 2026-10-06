// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;

namespace Notepads.Features.Sessions.Validation;

/// <summary>Complete file and permission roots for a committed or explicitly archived session.</summary>
internal sealed class SessionRecoveryReferences : SessionReferenceSet
{
    public bool BlocksGlobalCollection { get; set; }
    public HashSet<Guid> BlockedOwners { get; } = [];

    public void Include(SessionRecoveryReferences references)
    {
        base.Include(references);
        BlocksGlobalCollection |= references.BlocksGlobalCollection;
        BlockedOwners.UnionWith(references.BlockedOwners);
    }

    public void Include(RecoveryRecord record)
    {
        if (record.Session != null) Include(record.Session);
        if (record.Editor != null) Include(RecoveryRecordValidator.CreateEditorRoot(record.Editor));
        if (record.SourceEditor != null) Include(RecoveryRecordValidator.CreateEditorRoot(record.SourceEditor));
    }

    public void IncludeEditor(TextEditorSessionDataV2 editor) => Include(RecoveryRecordValidator.CreateEditorRoot(editor));

    public void Include(NotepadsSessionDataV2 session)
    {
        if (session?.TextEditors == null || session.UnrecoveredLegacyEditors == null)
            throw new InvalidDataException("Invalid V2 recovery reference collection.");
        ValidateTransferReferences(session);
        foreach (var editor in session.TextEditors)
        {
            if (editor == null) throw new InvalidDataException("Invalid V2 recovery record.");
            Include(editor.SavedBaseline);
            Include(editor.RecoveryBaseline);
            if (editor.Journal == null) throw new InvalidDataException("A V2 editor has no journal reference.");
            editor.Journal.Validate();
            JournalFileNames.Add(editor.Journal.FileName);
            if (editor.EditingFileFutureAccessToken != null) FutureAccessTokens.Add(editor.EditingFileFutureAccessToken);
        }
        foreach (var editor in session.UnrecoveredLegacyEditors) Include(editor);
        if (session.AcknowledgedSource != null)
        {
            var source = new NotepadsSessionDataV2();
            source.TextEditors.Add(session.AcknowledgedSource);
            Include(source);
        }
    }

    internal static void ValidateTransferReferences(NotepadsSessionDataV2 session)
    {
        var kinds = (session.TransferSource == null ? 0 : 1) + (session.TransferReceipt == null ? 0 : 1) +
            (session.TransferAcknowledgement == null ? 0 : 1);
        if (kinds > 1 || session.TransferAcknowledgement == null && session.AcknowledgedSource != null)
            throw new InvalidDataException("Conflicting durable transfer root kinds.");
        if (session.TransferAcknowledgement != null)
        {
            var token = session.TransferAcknowledgement;
            token.Validate();
            var source = session.AcknowledgedSource;
            if (session.TextEditors.Count != 0 || session.UnrecoveredLegacyEditors.Count != 0 || source?.StateMetaData == null ||
                source.Id != token.SourceEditorId || source.Journal?.OwnerId != token.SourceOwnerId ||
                session.SourceRemovalRevision < source.CaptureRevision ||
                !string.Equals(source.ComputeSha256(), token.DescriptorSha256, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidDataException("Invalid durable source-removal acknowledgement.");
            }
        }
        else if (kinds != 0)
        {
            if (session.TextEditors.Count != 1 || session.UnrecoveredLegacyEditors.Count != 0 || session.TextEditors[0]?.StateMetaData == null)
                throw new InvalidDataException("Invalid durable transfer editor graph.");
            var editor = session.TextEditors[0];
            if (session.TransferSource != null)
            {
                var token = session.TransferSource;
                token.Validate();
                if (editor.Id != token.SourceEditorId || editor.Journal?.OwnerId != token.SourceOwnerId ||
                    !string.Equals(editor.ComputeSha256(), token.DescriptorSha256, StringComparison.OrdinalIgnoreCase))
                {
                    throw new InvalidDataException("Invalid durable source transfer graph.");
                }
            }
            else
            {
                session.TransferReceipt.Validate();
                if (!string.Equals(editor.ComputeSha256(), session.TransferReceipt.TargetDescriptorSha256, StringComparison.OrdinalIgnoreCase))
                    throw new InvalidDataException("Invalid durable target receipt graph.");
            }
        }
    }

    public void Include(NotepadsSessionDataV1 session)
    {
        if (session?.TextEditors == null) throw new InvalidDataException("Invalid V1 recovery reference collection.");
        foreach (var editor in session.TextEditors) Include(editor);
    }

    public static SessionRecoveryReferences FromJson(string json)
    {
        var references = new SessionRecoveryReferences();
        using (var document = JsonDocument.Parse(json))
        {
            switch (document.RootElement.GetProperty("Version").GetInt32())
            {
                case 1:
                    references.Include(JsonSerializer.Deserialize<NotepadsSessionDataV1>(json, SessionJsonContext.Default.NotepadsSessionDataV1));
                    break;
                case 2:
                    references.Include(JsonSerializer.Deserialize<NotepadsSessionDataV2>(json, SessionJsonContext.Default.NotepadsSessionDataV2));
                    break;
                default:
                    throw new InvalidDataException("Unknown archived session version; recovery GC must retain its assets.");
            }
        }
        return references;
    }

    private void Include(DocumentBaselineData baseline)
    {
        if (baseline == null) throw new InvalidDataException("A V2 editor has no immutable baseline reference.");
        baseline.Validate();
        if (baseline.FileName != null) BaselineFileNames.Add(baseline.FileName);
    }

    private void Include(TextEditorSessionDataV1 editor)
    {
        if (editor == null) throw new InvalidDataException("Invalid legacy recovery record.");
        if (editor.LastSavedBackupFilePath != null) LegacyBackupPaths.Add(editor.LastSavedBackupFilePath);
        if (editor.PendingBackupFilePath != null) LegacyBackupPaths.Add(editor.PendingBackupFilePath);
        if (editor.EditingFileFutureAccessToken != null) FutureAccessTokens.Add(editor.EditingFileFutureAccessToken);
    }
}
