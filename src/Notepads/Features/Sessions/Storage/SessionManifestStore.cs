// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Storage;
using Windows.Storage;

namespace Notepads.Features.Sessions.Storage;

internal static class SessionManifestStore
{
    public static async Task<StorageFolder> GetBackupFolderAsync(string backupFolderName)
    {
        return await FileStorage.GetOrCreateAppFolderAsync(backupFolderName);
    }

    public static async Task<IReadOnlyList<StorageFile>> GetAllFilesInBackupFolderAsync(string backupFolderName)
    {
        StorageFolder backupFolder = await GetBackupFolderAsync(backupFolderName);
        return await backupFolder.GetFilesAsync();
    }

    internal static async Task<(NotepadsSessionDataV1 Session, string Fingerprint, Exception Error)> ReadLegacyAsync(
        string fileName, CancellationToken cancellation)
    {
        var path = Path.Combine(ApplicationData.Current.LocalFolder.Path, fileName);
        byte[] bytes;
        try
        {
            using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read,
                16384, FileOptions.Asynchronous | FileOptions.SequentialScan);
            if (stream.Length > RecoveryRecordCodec.MaximumMetadataByteLength)
                throw new InvalidDataException("The legacy session index exceeds the metadata limit.");
            using var output = new MemoryStream();
            await stream.CopyToAsync(output, cancellation);
            bytes = output.ToArray();
        }
        catch (FileNotFoundException) { return (null, null, null); }
        using var fingerprint = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        fingerprint.AppendData(Encoding.UTF8.GetBytes(Path.GetFullPath(path).ToUpperInvariant()));
        fingerprint.AppendData([0]);
        fingerprint.AppendData(bytes);
        var identity = Convert.ToHexStringLower(fingerprint.GetHashAndReset());
        try
        {
            var jsonBytes = bytes.AsSpan();
            if (jsonBytes.StartsWith(new byte[] { 0xef, 0xbb, 0xbf })) jsonBytes = jsonBytes.Slice(3);
            RecoveryRecordCodec.ValidateJson(jsonBytes);
            var session = JsonSerializer.Deserialize(jsonBytes, SessionJsonContext.Default.NotepadsSessionDataV1);
            if (session?.Version != 1 || session.TextEditors == null ||
                session.TextEditors.Any(editor => editor?.StateMetaData == null || editor.Id == Guid.Empty) ||
                session.TextEditors.Select(editor => editor.Id).Distinct().Count() != session.TextEditors.Count)
            {
                throw new InvalidDataException("The legacy session index has invalid records.");
            }

            return (session, identity, null);
        }
        catch (Exception error) when (error is JsonException or InvalidDataException)
        {
            // Preserve the immutable source and its byte identity. Failure to
            // parse migration input must not erase valid new-format authority.
            return (null, identity, error);
        }
    }

}
