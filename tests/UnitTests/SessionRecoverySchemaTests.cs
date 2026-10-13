// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Text.Json;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Validation;

namespace Notepads.UnitTests;

public sealed class SessionRecoverySchemaTests
{
    [Fact]
    public void StableOwnerRetainsThePersistedIdentity()
    {
        var expected = new Guid("30465059-7d20-d6d3-3375-af64c4b81a1f");
        Assert.Equal(expected, SessionScopeData.GetStableOwner());
        Assert.True(SessionScopeData.IsReservedOwner(expected));
        Assert.Equal("NotepadsSessionData.json", SessionScopeData.GetManifestFileName(expected));
    }

    [Fact]
    public void LegacyV1MissingFieldsRetainTheirOriginalDefaults()
    {
        const string json = """
            {"TextEditors":[{"Id":"00112233-4455-6677-8899-aabbccddeeff","LastSavedBackupFilePath":"saved",
            "PendingBackupFilePath":"pending","StateMetaData":{"LastSavedEncoding":"UTF-8","FontZoomFactor":1.25}}]}
            """;
        var legacy = JsonSerializer.Deserialize(json, SessionJsonContext.Default.NotepadsSessionDataV1);
        Assert.Equal(1, legacy.Version);
        Assert.Null(legacy.TextEditors[0].BackupEncoding);
        Assert.Equal(1.25, legacy.TextEditors[0].StateMetaData.FontZoomFactor);
        Assert.Null(legacy.TextEditors[0].StateMetaData.FirstVisibleDocumentLine);
    }

    [Fact]
    public void ViewportAnchorSurvivesCaptureAndSerialization()
    {
        var session = ValidSession();
        var metadata = session.TextEditors[0].StateMetaData;
        metadata.FirstVisibleDocumentLine = 800;
        metadata.FirstVisibleSubLine = 2;
        var capture = metadata.Clone();
        metadata.FirstVisibleDocumentLine = 0;
        session.TextEditors[0].StateMetaData = capture;
        var restored = JsonSerializer.Deserialize(Serialize(session), SessionJsonContext.Default.NotepadsSessionDataV2);
        Assert.Equal(800, restored.TextEditors[0].StateMetaData.FirstVisibleDocumentLine);
        Assert.Equal(2, restored.TextEditors[0].StateMetaData.FirstVisibleSubLine);
    }

    [Fact]
    public void RoundtripKeepsSavedRecoveryDirtinessAndPendingSettingsIndependent()
    {
        var session = ValidSession();
        var restored = JsonSerializer.Deserialize(Serialize(session), SessionJsonContext.Default.NotepadsSessionDataV2);
        Assert.Equal(2, restored.Version);
        Assert.Single(restored.TextEditors);
        Assert.Single(restored.UnrecoveredLegacyEditors);
        var editor = restored.TextEditors[0];
        Assert.False(editor.TextDirty);
        Assert.True(editor.StateMetaData.IsModified);
        Assert.True(editor.StateMetaData.RequiresSaveAs);
        Assert.Equal(5UL, editor.Journal.CommittedSequence);
        Assert.NotEqual(editor.SavedBaseline.GenerationId, editor.RecoveryBaseline.GenerationId);
    }

    [Fact]
    public void ReferencesRetainFailedLegacyRecordsAndCaseSensitivePermissionTokens()
    {
        var session = ValidSession();
        var references = new SessionRecoveryReferences();
        references.Include(session);
        Assert.Equal(2, references.FutureAccessTokens.Count);
        Assert.Contains("Token", references.FutureAccessTokens);
        Assert.Contains("token", references.FutureAccessTokens);
        Assert.Contains("legacy-pending", references.LegacyBackupPaths);
        Assert.Contains(session.TextEditors[0].Journal.FileName, references.JournalFileNames);
        Assert.Equal(2, references.LegacyBackupPaths.Count);
    }

    [Fact]
    public void TransferAcknowledgementProtectsTheSourceGraphAndPermission()
    {
        var acknowledgement = Acknowledgement();
        var references = new SessionRecoveryReferences();
        references.Include(acknowledgement);
        Assert.Contains(acknowledgement.AcknowledgedSource.Journal.FileName, references.JournalFileNames);
        Assert.Contains("Token", references.FutureAccessTokens);
    }

    [Fact]
    public void MissingAcknowledgedSourceRejectsTheIncompleteRoot()
    {
        var acknowledgement = Acknowledgement();
        acknowledgement.AcknowledgedSource = null;
        Assert.Throws<InvalidDataException>(() => new SessionRecoveryReferences().Include(acknowledgement));
    }

    [Fact]
    public void OversizedBaselineIsRejectedBeforeNativeAdmission()
    {
        var baseline = EmptyBaseline(Guid.NewGuid());
        baseline.ByteLength = DocumentLimits.MaximumCanonicalByteLength + 1;
        Assert.Throws<InvalidDataException>(() => baseline.Validate());
    }

    [Fact]
    public void BaselineFilenameCannotEscapeItsRecoveryDirectory()
    {
        var baseline = EmptyBaseline(Guid.NewGuid());
        baseline.FileName = "../other-window.utf8";
        Assert.Throws<InvalidDataException>(() => baseline.Validate());
    }

    [Fact]
    public void InvalidJournalChecksumRejectsTheRootInsteadOfDroppingItsReferences()
    {
        var session = ValidSession();
        session.TextEditors[0].Journal.PrefixSha256 = "invalid";
        Assert.Throws<InvalidDataException>(() => new SessionRecoveryReferences().Include(session));
    }

    private static NotepadsSessionDataV2 ValidSession()
    {
        var owner = Guid.NewGuid();
        var generation = Guid.NewGuid();
        var session = new NotepadsSessionDataV2();
        session.TextEditors.Add(new TextEditorSessionDataV2
        {
            Id = Guid.NewGuid(),
            SavedBaseline = EmptyBaseline(owner),
            RecoveryBaseline = EmptyBaseline(owner),
            Journal = new DocumentJournalData
            {
                OwnerId = owner,
                GenerationId = generation,
                FileName = owner.ToString("N") + "-" + generation.ToString("N") + ".npj",
                BaselineSequence = 5,
                CommittedSequence = 5,
                CommittedByteLength = 48,
                DocumentByteLength = 0,
                PrefixSha256 = new string('a', 64)
            },
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
            Id = Guid.NewGuid(), PendingBackupFilePath = "legacy-pending", LastSavedBackupFilePath = "legacy-saved",
            BackupEncoding = "UTF-8-BOM", EditingFileFutureAccessToken = "token",
            StateMetaData = new DocumentMetadata { IsModified = true }
        });
        return session;
    }

    private static NotepadsSessionDataV2 Acknowledgement()
    {
        var editor = ValidSession().TextEditors[0];
        return new NotepadsSessionDataV2
        {
            TransferAcknowledgement = new TransferToken
            {
                TransferId = Guid.NewGuid(), SourceInstanceId = Guid.NewGuid(), SourceEditorId = editor.Id,
                SourceOwnerId = editor.Journal.OwnerId, SourceEpochId = Guid.NewGuid(), Nonce = Guid.NewGuid(),
                DescriptorSha256 = editor.ComputeSha256()
            },
            AcknowledgedSource = editor, SourceRemovalRevision = editor.CaptureRevision
        };
    }

    private static string Serialize(NotepadsSessionDataV2 session) =>
        JsonSerializer.Serialize(session, SessionJsonContext.Default.NotepadsSessionDataV2);

    private static DocumentBaselineData EmptyBaseline(Guid owner) => new()
    {
        OwnerId = owner, GenerationId = Guid.NewGuid(), ByteLength = 0,
        Sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    };
}
