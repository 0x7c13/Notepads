// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.Features.Sessions.Validation;

internal sealed record RecoveryEncodedRecord(RecoveryRecord Record, string PayloadSha256, byte[] EnvelopeBytes);

/// <summary>One bounded wire format for recovery content and lifecycle metadata.</summary>
internal static class RecoveryRecordCodec
{
    public const int FormatVersion = 1;
    public const int MaximumMetadataByteLength = 8 * 1024 * 1024;
    public const int MaximumDepth = 32;
    private static readonly UTF8Encoding StrictUtf8 = new(false, true);

    public static RecoveryEncodedRecord Encode(RecoveryRecord record, RecoveryAddress address)
    {
        Validate(record, address);
        byte[] payload;
        try
        {
            using var buffer = new MetadataBuffer();
            JsonSerializer.Serialize(buffer, record, SessionJsonContext.Default.RecoveryRecord);
            payload = buffer.ToArray();
        }
        catch (JsonException error) { throw new InvalidDataException("Recovery metadata could not be serialized.", error); }
        ValidateJson(payload);
        var hash = Convert.ToHexStringLower(SHA256.HashData(payload));
        using var output = new MetadataBuffer();
        using (var writer = new Utf8JsonWriter(output))
        {
            writer.WriteStartObject();
            writer.WriteNumber("FormatVersion", FormatVersion);
            writer.WriteString("Sha256", hash);
            writer.WritePropertyName("Payload");
            writer.WriteRawValue(payload, skipInputValidation: true);
            writer.WriteEndObject();
        }
        if (output.Length > MaximumMetadataByteLength)
            throw new InvalidDataException("Recovery metadata exceeds the 8 MiB limit.");
        // Own a deserialized snapshot; subsequent caller mutations cannot alter the published identity.
        return Decode(output.ToArray(), address);
    }

    public static RecoveryEncodedRecord Decode(ReadOnlySpan<byte> envelope, RecoveryAddress address)
    {
        ValidateJson(envelope);
        try
        {
            var reader = new Utf8JsonReader(envelope, new JsonReaderOptions { MaxDepth = MaximumDepth });
            if (!reader.Read() || reader.TokenType != JsonTokenType.StartObject)
                throw new InvalidDataException("The recovery envelope must be an object.");
            int? version = null;
            string hash = null;
            ReadOnlySpan<byte> payload = default;
            while (reader.Read() && reader.TokenType != JsonTokenType.EndObject)
            {
                if (reader.TokenType != JsonTokenType.PropertyName)
                    throw new InvalidDataException("Invalid recovery envelope.");
                var name = reader.GetString();
                if (!reader.Read()) throw new InvalidDataException("Truncated recovery envelope.");
                switch (name)
                {
                    case "FormatVersion":
                        version = reader.GetInt32();
                        break;
                    case "Sha256":
                        hash = reader.GetString();
                        break;
                    case "Payload":
                        if (reader.TokenType != JsonTokenType.StartObject)
                            throw new InvalidDataException("The recovery payload must be an object.");
                        var start = checked((int)reader.TokenStartIndex);
                        reader.Skip();
                        payload = envelope.Slice(start, checked((int)reader.BytesConsumed) - start);
                        break;
                    default:
                        throw new InvalidDataException("Unknown recovery envelope field.");
                }
            }
            if (version != FormatVersion || !IsSha256(hash) || payload.IsEmpty)
                throw new InvalidDataException("Invalid recovery envelope version, hash or payload.");
            var actualHash = Convert.ToHexStringLower(SHA256.HashData(payload));
            if (!string.Equals(hash, actualHash, StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("The recovery payload checksum does not match its original bytes.");
            var record = JsonSerializer.Deserialize(payload, SessionJsonContext.Default.RecoveryRecord);
            Validate(record, address);
            return new RecoveryEncodedRecord(record, actualHash, envelope.ToArray());
        }
        catch (Exception error) when (error is JsonException or InvalidOperationException or FormatException or OverflowException)
        {
            throw new InvalidDataException("Invalid recovery envelope.", error);
        }
    }

    internal static bool IsSha256(string value)
    {
        if (value?.Length != 64) return false;
        foreach (var character in value)
            if (!(character is >= '0' and <= '9' or >= 'a' and <= 'f' or >= 'A' and <= 'F')) return false;
        return true;
    }

    public static void ValidateJson(ReadOnlySpan<byte> json)
    {
        if (json.Length == 0 || json.Length > MaximumMetadataByteLength)
            throw new InvalidDataException("Invalid recovery metadata length.");
        try
        {
            StrictUtf8.GetCharCount(json);
            var reader = new Utf8JsonReader(json, new JsonReaderOptions { MaxDepth = MaximumDepth });
            var objects = new Stack<HashSet<string>>();
            while (reader.Read())
            {
                switch (reader.TokenType)
                {
                    case JsonTokenType.StartObject:
                        objects.Push(new HashSet<string>(StringComparer.Ordinal));
                        break;
                    case JsonTokenType.EndObject:
                        objects.Pop();
                        break;
                    case JsonTokenType.PropertyName:
                        if (!objects.Peek().Add(reader.GetString()))
                            throw new InvalidDataException("Duplicate recovery metadata property.");
                        break;
                    case JsonTokenType.String:
                        // Escaped unpaired UTF-16 surrogates are invalid even when their source bytes are ASCII.
                        reader.GetString();
                        break;
                }
            }
        }
        catch (Exception error) when (error is JsonException or DecoderFallbackException or InvalidOperationException)
        {
            throw new InvalidDataException("Recovery metadata is invalid UTF-8 JSON or exceeds the depth limit.", error);
        }
    }

    private static void Validate(RecoveryRecord record, RecoveryAddress address)
    {
        var stamp = record?.Stamp;
        if (stamp == null || stamp.ScopeId == Guid.Empty || stamp.EpochId == Guid.Empty || stamp.OperationId == Guid.Empty || stamp.Ordinal == 0 ||
            address == null || address.OperationId != stamp.OperationId || address.Ordinal != stamp.Ordinal ||
            !address.Area.AllowsForeignOwners && address.Area.ScopeId != stamp.ScopeId || !Enum.IsDefined(record.Kind))
        {
            throw new InvalidDataException("Recovery identity does not match its owning path.");
        }

        var allowedKind = address.Area.Kind switch
        {
            RecoveryAreaKind.Checkpoints => record.Kind == RecoveryRecordKind.Checkpoint,
            RecoveryAreaKind.Decisions => record.Kind is RecoveryRecordKind.Reset or RecoveryRecordKind.Closed or
                RecoveryRecordKind.AdoptLegacy or RecoveryRecordKind.AdoptInactive,
            RecoveryAreaKind.Rescue => record.Kind == RecoveryRecordKind.Rescue,
            RecoveryAreaKind.Pending => record.Kind == RecoveryRecordKind.Pending,
            RecoveryAreaKind.Transfers => record.Kind is RecoveryRecordKind.TransferOffer or RecoveryRecordKind.TransferReceipt or RecoveryRecordKind.SourceMoved,
            RecoveryAreaKind.Archives => record.Kind == RecoveryRecordKind.Archive,
            _ => false
        };
        if (!allowedKind) throw new InvalidDataException("Recovery record kind does not match its directory.");
        ValidateExpectedStamp(record.SourceStamp);
        ValidateExpectedStamp(record.TargetStamp);
        if (record.Consumptions == null || record.ResetAcceptedLegacySources == null)
            throw new InvalidDataException("Invalid lifecycle metadata collections.");
        var consumptionIds = new HashSet<string>(StringComparer.Ordinal);
        foreach (var consumption in record.Consumptions)
        {
            if (consumption == null || !Enum.IsDefined(consumption.Kind) || consumption.SourceEditorId == Guid.Empty ||
                consumption.SourceCaptureRevision < 0 || consumption.SourceCaptureRevision == long.MaxValue ||
                consumption.Kind == RecoveryConsumptionKind.Legacy && !IsSha256(consumption.LegacyFingerprint) ||
                consumption.Kind == RecoveryConsumptionKind.Inactive && (consumption.SourceScopeId == Guid.Empty || consumption.SourceEpochId == Guid.Empty))
            {
                throw new InvalidDataException("Invalid source consumption identity.");
            }

            var identity = $"{consumption.Kind}/{consumption.LegacyFingerprint?.ToLowerInvariant()}/{consumption.SourceScopeId:N}/{consumption.SourceEpochId:N}/{consumption.SourceEditorId:N}";
            if (!consumptionIds.Add(identity)) throw new InvalidDataException("Duplicate source consumption identity.");
        }
        var fingerprints = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var fingerprint in record.ResetAcceptedLegacySources)
        {
            if (!IsSha256(fingerprint) || !fingerprints.Add(fingerprint))
                throw new InvalidDataException("Invalid or duplicate legacy source suppression.");
        }

        if (record.Kind == RecoveryRecordKind.Reset && (record.PreviousEpochId == Guid.Empty || record.PreviousEpochId == stamp.EpochId) ||
            record.Kind == RecoveryRecordKind.Closed && record.ClosedEditorId == Guid.Empty ||
            record.Kind == RecoveryRecordKind.Checkpoint && record.Session == null ||
            record.Kind is RecoveryRecordKind.Rescue or RecoveryRecordKind.Pending or RecoveryRecordKind.AdoptLegacy or RecoveryRecordKind.AdoptInactive && record.Editor == null)
        {
            throw new InvalidDataException("Incomplete recovery record.");
        }

        if (record.Kind == RecoveryRecordKind.AdoptLegacy && (record.Consumptions.Count != 1 || record.Consumptions[0].Kind != RecoveryConsumptionKind.Legacy) ||
            record.Kind == RecoveryRecordKind.AdoptInactive && (record.SourceStamp == null || record.Consumptions.Count != 1 ||
                record.Consumptions[0].Kind != RecoveryConsumptionKind.Inactive ||
                record.Consumptions[0].SourceScopeId != record.SourceStamp.ScopeId || record.Consumptions[0].SourceEpochId != record.SourceStamp.EpochId))
        {
            throw new InvalidDataException("An adoption must bind its exact source consumption identity.");
        }

        if (record.TargetReceiptSha256 != null && !IsSha256(record.TargetReceiptSha256))
            throw new InvalidDataException("Invalid target receipt checksum.");
        var localOwner = address.Area.AllowsForeignOwners ? Guid.Empty : stamp.ScopeId;
        if (record.Session != null)
        {
            if (record.Session.Version != 2 || record.Session.Scope == null || record.Session.Scope.OwnerId != stamp.ScopeId ||
                record.Session.TextEditors == null || record.Session.UnrecoveredLegacyEditors == null ||
                !address.Area.AllowsForeignOwners && record.Session.UnrecoveredLegacyEditors.Count != 0 ||
                record.Session.SourceRemovalRevision < 0 || record.Session.SourceRemovalRevision == long.MaxValue ||
                !double.IsFinite(record.Session.TabScrollViewerHorizontalOffset) || record.Session.TabScrollViewerHorizontalOffset < 0)
            {
                throw new InvalidDataException("Invalid recovery session.");
            }

            record.Session.Scope.Validate();
            var editors = new HashSet<Guid>();
            foreach (var editor in record.Session.TextEditors)
            {
                ValidateEditor(editor, localOwner);
                if (!editors.Add(editor.Id)) throw new InvalidDataException("Duplicate editor incarnation.");
            }
            if (record.Session.SelectedTextEditor != Guid.Empty && !editors.Contains(record.Session.SelectedTextEditor))
                throw new InvalidDataException("The selected editor is absent from its recovery session.");
            foreach (var editor in record.Session.UnrecoveredLegacyEditors)
            {
                if (editor == null || editor.Id == Guid.Empty || !editors.Add(editor.Id))
                    throw new InvalidDataException("Invalid or duplicate legacy editor incarnation.");
            }

            if (record.Session.AcknowledgedSource != null) ValidateEditor(record.Session.AcknowledgedSource, localOwner);
            SessionRecoveryReferences.ValidateTransferReferences(record.Session);
        }
        if (record.Editor != null) ValidateEditor(record.Editor, localOwner);
        if (record.SourceEditor != null) ValidateEditor(record.SourceEditor, localOwner);
        if (address.Area.Kind is RecoveryAreaKind.Rescue or RecoveryAreaKind.Pending &&
            address.Area.DocumentId != Guid.Empty && record.Editor?.Id != address.Area.DocumentId)
        {
            throw new InvalidDataException("The recovery descriptor does not match its document directory.");
        }

        ValidateTransfer(record, address);
    }

    private static void ValidateTransfer(RecoveryRecord record, RecoveryAddress address)
    {
        if (record.Kind is not (RecoveryRecordKind.TransferOffer or RecoveryRecordKind.TransferReceipt or RecoveryRecordKind.SourceMoved)) return;
        if (record.Session == null || record.SourceStamp == null || record.Editor == null ||
            record.Kind != RecoveryRecordKind.TransferOffer && record.TargetStamp == null || address.Area.RootId == Guid.Empty)
        {
            throw new InvalidDataException("Incomplete durable transfer record.");
        }

        var source = record.Kind switch
        {
            RecoveryRecordKind.TransferOffer => record.Session.TransferSource,
            RecoveryRecordKind.TransferReceipt => record.Session.TransferReceipt?.Source,
            _ => record.Session.TransferAcknowledgement
        };
        if (source == null || source.TransferId != address.Area.RootId ||
            source.SourceOwnerId != record.SourceStamp.ScopeId || source.SourceEpochId != record.SourceStamp.EpochId)
        {
            throw new InvalidDataException("Transfer identity does not match its path or frozen source epoch.");
        }

        source.Validate();
        var expectedOwner = record.Kind == RecoveryRecordKind.TransferOffer ? record.SourceStamp : record.TargetStamp;
        if (record.Editor.Journal.OwnerId != expectedOwner.ScopeId || record.Editor.SavedBaseline.OwnerId != expectedOwner.ScopeId ||
            record.Editor.RecoveryBaseline.OwnerId != expectedOwner.ScopeId)
        {
            throw new InvalidDataException("A transfer descriptor does not belong to its expected scope.");
        }

        var publicationOwner = record.Kind == RecoveryRecordKind.TransferReceipt ? record.TargetStamp : record.SourceStamp;
        if (record.Stamp.ScopeId != publicationOwner.ScopeId || record.Stamp.EpochId != publicationOwner.EpochId)
            throw new InvalidDataException("A transfer publisher does not match its expected epoch.");
        if (record.Kind == RecoveryRecordKind.TransferOffer)
        {
            if (record.Session.TextEditors.Count != 1 ||
                !string.Equals(record.Editor.ComputeSha256(), source.DescriptorSha256, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidDataException("A transfer offer does not bind its complete source descriptor.");
            }
        }
        else if (record.Kind == RecoveryRecordKind.TransferReceipt)
        {
            var receipt = record.Session.TransferReceipt;
            if (receipt.TargetOwnerId != record.TargetStamp.ScopeId || receipt.TargetEpochId != record.TargetStamp.EpochId ||
                !string.Equals(record.Editor.ComputeSha256(), receipt.TargetDescriptorSha256, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidDataException("A receipt does not bind its complete target descriptor and frozen epoch.");
            }
        }
        else
        {
            if (record.SourceEditor == null || record.SourceEditor.Journal.OwnerId != record.SourceStamp.ScopeId ||
                record.SourceEditor.SavedBaseline.OwnerId != record.SourceStamp.ScopeId || record.SourceEditor.RecoveryBaseline.OwnerId != record.SourceStamp.ScopeId ||
                record.SourceEditor.Id != source.SourceEditorId || record.TargetReceiptOperationId == Guid.Empty || !IsSha256(record.TargetReceiptSha256) ||
                !string.Equals(record.SourceEditor.ComputeSha256(), source.DescriptorSha256, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidDataException("A source move does not bind its source capture and durable target receipt.");
            }
        }
    }

    private static void ValidateExpectedStamp(RecoveryStamp stamp)
    {
        if (stamp != null && (stamp.ScopeId == Guid.Empty || stamp.EpochId == Guid.Empty))
            throw new InvalidDataException("Invalid expected recovery scope and epoch.");
    }

    private static void ValidateEditor(TextEditorSessionDataV2 editor, Guid localOwner)
    {
        if (editor?.Id == Guid.Empty || editor == null || editor.CaptureRevision < 0 || editor.CaptureRevision == long.MaxValue || editor.StateMetaData == null ||
            editor.SavedBaseline == null || editor.RecoveryBaseline == null || editor.Journal == null ||
            editor.SavedBaseline.OwnerId == Guid.Empty || editor.RecoveryBaseline.OwnerId == Guid.Empty || editor.Journal.OwnerId == Guid.Empty ||
            localOwner != Guid.Empty && (editor.SavedBaseline.OwnerId != localOwner || editor.RecoveryBaseline.OwnerId != localOwner || editor.Journal.OwnerId != localOwner))
        {
            throw new InvalidDataException("Invalid or foreign scope-local recovery descriptor.");
        }

        editor.SavedBaseline.Validate();
        editor.RecoveryBaseline.Validate();
        editor.Journal.Validate();
        if (editor.EditingFileFutureAccessToken != null)
        {
            var prefix = $"Notepads:DocumentOwner:{editor.Journal.OwnerId:N}:{editor.Id:N}-";
            var token = editor.EditingFileFutureAccessToken;
            if (!token.StartsWith(prefix, StringComparison.Ordinal) || token.Length != prefix.Length + 32 ||
                !Guid.TryParseExact(token.AsSpan(prefix.Length), "N", out var grantId) || grantId == Guid.Empty)
            {
                throw new InvalidDataException("A recovery descriptor references a foreign or invalid file grant.");
            }
        }
        var state = editor.StateMetaData;
        if (state.SelectionStartPosition < 0 || state.SelectionEndPosition < 0 || !double.IsFinite(state.FontZoomFactor) || state.FontZoomFactor < 0 ||
            !double.IsFinite(state.ScrollViewerHorizontalOffset) || state.ScrollViewerHorizontalOffset < 0 ||
            !double.IsFinite(state.ScrollViewerVerticalOffset) || state.ScrollViewerVerticalOffset < 0)
        {
            throw new InvalidDataException("Invalid recovery editor view ranges.");
        }
    }

    private sealed class MetadataBuffer : MemoryStream
    {
        public override void Write(byte[] buffer, int offset, int count)
        {
            CheckLength(count);
            base.Write(buffer, offset, count);
        }

        public override void Write(ReadOnlySpan<byte> buffer)
        {
            CheckLength(buffer.Length);
            base.Write(buffer);
        }

        private void CheckLength(int count)
        {
            if (Position + count > MaximumMetadataByteLength)
                throw new InvalidDataException("Recovery metadata exceeds the 8 MiB limit.");
        }
    }
}
