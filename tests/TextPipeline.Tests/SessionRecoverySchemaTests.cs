// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text.Json;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Validation;

namespace NotepadsEditorTests;

internal static class SessionRecoverySchemaTests
{
    private static void CheckLegacyJsonWriter()
    {
        // Captured from the pre-migration DTOs and System.Text.Json 9.0.2 writer.
        var path = Path.Combine(AppContext.BaseDirectory, "Fixtures", "editor-v2.json");
        var json = File.ReadAllText(path);
        var editor = JsonSerializer.Deserialize(json, SessionJsonContext.Default.TextEditorSessionDataV2);
        Assert(JsonSerializer.Serialize(editor, SessionJsonContext.Default.TextEditorSessionDataV2) == json,
            "source generation preserves legacy property order, Unicode escaping, nulls and floating-point formatting");
        Assert(editor.ComputeSha256() == File.ReadAllText(Path.ChangeExtension(path, ".sha256")),
            "source generation preserves the previous writer's recovery checksum");
        const string v1 = """
            {"TextEditors":[{"Id":"00112233-4455-6677-8899-aabbccddeeff","LastSavedBackupFilePath":"saved",
            "PendingBackupFilePath":"pending","StateMetaData":{"LastSavedEncoding":"UTF-8","FontZoomFactor":1.25}}]}
            """;
        var legacy = JsonSerializer.Deserialize(v1, SessionJsonContext.Default.NotepadsSessionDataV1);
        Assert(legacy.Version == 1 && legacy.TextEditors[0].BackupEncoding == null &&
            legacy.TextEditors[0].StateMetaData.FontZoomFactor == 1.25,
            "V1 records retain defaults for omitted version and backup encoding");
    }

    public static void Run()
    {
        CheckLegacyJsonWriter();
        var owner = Guid.NewGuid();
        var saved = EmptyBaseline(owner);
        var recovery = EmptyBaseline(owner);
        var journalGeneration = Guid.NewGuid();
        var session = new NotepadsSessionDataV2();
        session.TextEditors.Add(new TextEditorSessionDataV2
        {
            Id = Guid.NewGuid(),
            SavedBaseline = saved,
            RecoveryBaseline = recovery,
            Journal = new DocumentJournalData
            {
                OwnerId = owner,
                GenerationId = journalGeneration,
                FileName = owner.ToString("N") + "-" + journalGeneration.ToString("N") + ".npj",
                BaselineSequence = 5,
                CommittedSequence = 5,
                CommittedByteLength = 48,
                DocumentByteLength = 0,
                PrefixSha256 = new string('a', 64)
            },
            // Returning to the text save point does not discard later operation
            // sequences; pending save settings remain independently modified.
            TextDirty = false,
            EditingFileFutureAccessToken = "Token",
            StateMetaData = new DocumentMetadata
            {
                LastSavedEncoding = "UTF-8",
                LastSavedLineEnding = "CRLF",
                RequestedEncoding = "Western (windows-1252)",
                RequiresSaveAs = true,
                IsModified = true
            }
        });
        session.UnrecoveredLegacyEditors.Add(new TextEditorSessionDataV1
        {
            Id = Guid.NewGuid(),
            PendingBackupFilePath = "legacy-pending",
            LastSavedBackupFilePath = "legacy-saved",
            BackupEncoding = "UTF-8-BOM",
            EditingFileFutureAccessToken = "token",
            StateMetaData = new DocumentMetadata { IsModified = true }
        });
        var json = JsonSerializer.Serialize(session, SessionJsonContext.Default.NotepadsSessionDataV2);
        var restored = JsonSerializer.Deserialize(json, SessionJsonContext.Default.NotepadsSessionDataV2);
        Assert(restored.Version == 2 && restored.TextEditors.Count == 1 && restored.UnrecoveredLegacyEditors.Count == 1,
            "Round-trip lost a live V2 or failed legacy record.");
        var editor = restored.TextEditors[0];
        Assert(!editor.TextDirty && editor.StateMetaData.IsModified && editor.StateMetaData.RequiresSaveAs &&
            editor.Journal.CommittedSequence == 5 &&
            editor.SavedBaseline.GenerationId != editor.RecoveryBaseline.GenerationId,
            "Saved metadata, recovery baseline, native dirtiness, and journal sequence were conflated.");
        var references = SessionRecoveryReferences.FromJson(json);
        Assert(references.FutureAccessTokens.Count == 2 && references.FutureAccessTokens.Contains("Token") &&
            references.FutureAccessTokens.Contains("token") && references.LegacyBackupPaths.Contains("legacy-pending") &&
            references.JournalFileNames.Contains(editor.Journal.FileName),
            "Manifest/archive reference roots omitted failed records or compared permission tokens without case sensitivity.");

        var legacy = new NotepadsSessionDataV1();
        legacy.TextEditors.Add(session.UnrecoveredLegacyEditors[0]);
        Assert(SessionRecoveryReferences.FromJson(JsonSerializer.Serialize(legacy, SessionJsonContext.Default.NotepadsSessionDataV1)).LegacyBackupPaths.Count == 2,
            "The V1 backward reference reader lost existing backup assets.");
        var acknowledgement = new NotepadsSessionDataV2
        {
            TransferAcknowledgement = new TransferToken
            {
                TransferId = Guid.NewGuid(),
                SourceInstanceId = Guid.NewGuid(),
                SourceEditorId = editor.Id,
                SourceOwnerId = owner,
                SourceEpochId = Guid.NewGuid(),
                Nonce = Guid.NewGuid(),
                DescriptorSha256 = editor.ComputeSha256()
            },
            AcknowledgedSource = editor,
            SourceRemovalRevision = editor.CaptureRevision
        };
        var acknowledgedReferences = SessionRecoveryReferences.FromJson(JsonSerializer.Serialize(acknowledgement, SessionJsonContext.Default.NotepadsSessionDataV2));
        Assert(acknowledgedReferences.JournalFileNames.Contains(editor.Journal.FileName) &&
            acknowledgedReferences.FutureAccessTokens.Contains("Token"),
            "An acknowledgement omitted the moved source's retained graph or file grant.");
        acknowledgement.AcknowledgedSource = null;
        AssertThrows<InvalidDataException>(() => SessionRecoveryReferences.FromJson(JsonSerializer.Serialize(acknowledgement, SessionJsonContext.Default.NotepadsSessionDataV2)));
        AssertThrows<InvalidDataException>(() => SessionRecoveryReferences.FromJson("{\"Version\":3}"));
        var bad = EmptyBaseline(owner);
        bad.ByteLength = DocumentLimits.MaximumCanonicalByteLength + 1;
        AssertThrows<InvalidDataException>(() => bad.Validate());
        bad = EmptyBaseline(owner);
        bad.FileName = "../other-window.utf8";
        AssertThrows<InvalidDataException>(() => bad.Validate());
        editor.Journal.PrefixSha256 = "invalid";
        AssertThrows<InvalidDataException>(() => SessionRecoveryReferences.FromJson(JsonSerializer.Serialize(restored, SessionJsonContext.Default.NotepadsSessionDataV2)));
        Console.WriteLine("PASS: V2 recovery schema keeps saved/recovery baselines, text dirtiness, journal sequence, pending settings, and failed V1 reference roots independent; unknown/corrupt roots abort GC.");
    }

    private static DocumentBaselineData EmptyBaseline(Guid owner) => new()
    {
        OwnerId = owner,
        GenerationId = Guid.NewGuid(),
        ByteLength = 0,
        Sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    };

    private static void AssertThrows<TException>(Action action) where TException : Exception
    {
        try { action(); }
        catch (TException) { return; }
        throw new Exception("Expected " + typeof(TException).Name + " was not raised.");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new Exception(message);
    }
}
