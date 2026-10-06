// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.Text;
using Notepads.Features.Sessions.Storage;
using Notepads.Infrastructure.Storage;
using Windows.Storage;
using Windows.Storage.AccessCache;
using Windows.Storage.Streams;

namespace NotepadsEditorTests;

internal static class SessionStorageTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            "SessionStorageTests-" + Guid.NewGuid().ToString("N"), CreationCollisionOption.FailIfExists);
        var accessTokens = new List<string>();
        try
        {
            var file = await folder.CreateFileAsync("manifest", CreationCollisionOption.FailIfExists);
            const string previous = "previous complete generation\r\n中文😀\0";
            await AtomicFileWriter.WriteAsync(file, stream => WriteUtf8Async(stream, previous));
            await AssertUtf8Async(file, previous, "initial complete commit");

            var failed = false;
            try
            {
                await AtomicFileWriter.WriteAsync(file, async stream =>
                {
                    await WriteUtf8Async(stream, "partial replacement");
                    throw new IOException("Injected writer failure before commit.");
                });
            }
            catch (IOException)
            {
                failed = true;
            }
            if (!failed) throw new Exception("A failed writer reported a successful transaction.");
            await AssertUtf8Async(file, previous, "failed write retains committed bytes");

            try
            {
                await AtomicFileWriter.WriteAsync(file, async stream =>
                {
                    await WriteUtf8Async(stream, "canceled replacement");
                    throw new OperationCanceledException();
                });
                throw new Exception("A canceled writer reported a successful transaction.");
            }
            catch (OperationCanceledException)
            {
                await AssertUtf8Async(file, previous, "canceled write retains committed bytes");
            }

            const string shorter = "新\0😀";
            await AtomicFileWriter.WriteAsync(file, stream => WriteUtf8Async(stream, shorter));
            await AssertUtf8Async(file, shorter, "shorter replacement truncates the old contents");

            // A recovery file must represent edits outside the user's selected
            // save encoding. Its BOM also remains readable by the legacy reader.
            const string recovery = "ASCII file, new 中文 العربية 😀\0\rnext";
            await AtomicFileWriter.WriteAsync(file, async stream =>
            {
                using (var writer = new StreamWriter(stream, new UTF8Encoding(true), 1024, leaveOpen: true))
                {
                    await LineEndingUtility.WriteAsync(writer, recovery);
                    await writer.FlushAsync();
                }
            });
            using (var stream = await file.OpenStreamForReadAsync())
            using (var reader = new StreamReader(stream, Encoding.ASCII))
            {
                if (!string.Equals(await reader.ReadToEndAsync(), recovery, StringComparison.Ordinal))
                {
                    throw new Exception("Recovery storage lost text outside the original save encoding.");
                }
            }

            var committedToken = "SessionStorageTests-" + Guid.NewGuid().ToString("N");
            accessTokens.Add(committedToken);
            StorageApplicationPermissions.FutureAccessList.AddOrReplace(committedToken, file);
            var candidate = await folder.CreateFileAsync("candidate", CreationCollisionOption.FailIfExists);
            var candidateToken = "SessionStorageTests-" + Guid.NewGuid().ToString("N");
            accessTokens.Add(candidateToken);
            var failedDraft = new SessionGenerationDraft();
            failedDraft.RegisterAccessToken(candidateToken);
            StorageApplicationPermissions.FutureAccessList.AddOrReplace(candidateToken, candidate);
            var failures = await failedDraft.RollbackAsync();
            if (failures.Count > 0) throw new AggregateException(failures);
            if (StorageApplicationPermissions.FutureAccessList.ContainsItem(candidateToken) ||
                await folder.TryGetItemAsync("candidate") == null)
            {
                throw new Exception("Failed session draft did not roll back only its newly granted permission.");
            }
            if (!StorageApplicationPermissions.FutureAccessList.ContainsItem(committedToken) ||
                await folder.TryGetItemAsync("manifest") == null)
            {
                throw new Exception("Session draft rollback removed committed resources.");
            }

            var published = await folder.CreateFileAsync("published", CreationCollisionOption.FailIfExists);
            var publishedToken = "SessionStorageTests-" + Guid.NewGuid().ToString("N");
            accessTokens.Add(publishedToken);
            var committedDraft = new SessionGenerationDraft();
            committedDraft.RegisterAccessToken(publishedToken);
            StorageApplicationPermissions.FutureAccessList.AddOrReplace(publishedToken, published);
            committedDraft.Commit();
            failures = await committedDraft.RollbackAsync();
            if (failures.Count > 0) throw new AggregateException(failures);
            if (!StorageApplicationPermissions.FutureAccessList.ContainsItem(publishedToken) ||
                await folder.TryGetItemAsync("published") == null)
            {
                throw new Exception("Committed session draft failed to transfer resource ownership.");
            }

            var uncertainDraft = new SessionGenerationDraft();
            var uncertainToken = "SessionStorageTests-" + Guid.NewGuid().ToString("N");
            accessTokens.Add(uncertainToken);
            uncertainDraft.RegisterAccessToken(uncertainToken);
            StorageApplicationPermissions.FutureAccessList.AddOrReplace(uncertainToken, published);
            uncertainDraft.PreserveForRecovery();
            await uncertainDraft.RollbackAsync();
            if (!StorageApplicationPermissions.FutureAccessList.ContainsItem(uncertainToken))
                throw new Exception("An uncertain final publication rolled back a potentially committed permission.");
            log.AppendLine("PASS: V2 atomic file storage, failed/canceled retention, unpublished permission rollback, uncertain-publication grant retention, shorter rewrites, and lossless recovery bytes.");
        }
        finally
        {
            foreach (var token in accessTokens)
            {
                if (StorageApplicationPermissions.FutureAccessList.ContainsItem(token))
                {
                    StorageApplicationPermissions.FutureAccessList.Remove(token);
                }
            }
            await folder.DeleteAsync(StorageDeleteOption.PermanentDelete);
        }
    }

    private static Task WriteUtf8Async(Stream stream, string text)
    {
        var bytes = new UTF8Encoding(false, true).GetBytes(text);
        return stream.WriteAsync(bytes, 0, bytes.Length);
    }

    private static async Task AssertUtf8Async(StorageFile file, string expected, string label)
    {
        var buffer = await FileIO.ReadBufferAsync(file);
        var bytes = new byte[buffer.Length];
        using (var reader = DataReader.FromBuffer(buffer)) reader.ReadBytes(bytes);
        if (!bytes.SequenceEqual(new UTF8Encoding(false, true).GetBytes(expected)))
        {
            throw new Exception("Atomic storage mismatch: " + label + ".");
        }
    }
}
