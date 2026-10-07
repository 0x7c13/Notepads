// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Documents.Text;
using Windows.Storage;

namespace NotepadsEditorTests;

internal static class DocumentBaselineTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        var owner = Guid.NewGuid();
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            "DocumentBaselines", CreationCollisionOption.OpenIfExists);
        try
        {
            await CheckSnapshotAndReaderOwnershipAsync(folder, owner);
            await CheckCanonicalChunksAsync(owner);
            await CheckFailedCandidatesAsync(folder, owner);
            await CheckRecoveryOwnershipAsync(folder, owner);
            await CheckPreparedGenerationOwnershipAsync(folder, owner);
            CheckJournalCompactionPolicy();
            await CheckEmptyGenerationAsync(owner);
            await CheckFileDecodingPolicyAsync(owner);
            log.AppendLine("PASS: document baselines retain immutable canonical text, independent readers, metadata, and recovery ownership; failed candidates are removed; journal compaction follows document size.");
        }
        finally
        {
            // Remove only generations owned by this test, including a failed assertion.
            foreach (var file in await folder.GetFilesAsync())
            {
                if (file.Name.StartsWith(owner.ToString("N") + "-", StringComparison.Ordinal))
                    await file.DeleteAsync(StorageDeleteOption.PermanentDelete);
            }
        }
    }

    private static async Task CheckFileDecodingPolicyAsync(Guid owner)
    {
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            "DocumentDecodeTests-" + Guid.NewGuid().ToString("N"), CreationCollisionOption.FailIfExists);
        try
        {
            var input = await folder.CreateFileAsync("policy.txt", CreationCollisionOption.FailIfExists);
            var mutable = (Encoding)new UTF8Encoding(false).Clone();
            mutable.DecoderFallback = new DecoderReplacementFallback("[bad]");
            var isolated = new DocumentLoadOptions(mutable);
            mutable.DecoderFallback = DecoderFallback.ExceptionFallback;
            await FileIO.WriteBytesAsync(input, [0xFF]);
            using (var snapshot = await DocumentTextPipeline.DecodeFileAsync(input, isolated, owner))
                Assert(await snapshot.ReadTextAsync() == "[bad]", "Loading options retained the caller's mutable encoding policy.");
            var raw = new UTF8Encoding(false).GetBytes("abcd\r\n");
            await FileIO.WriteBytesAsync(input, raw);
            var configured = new UnicodeEncoding(false, false);
            var defaults = new DocumentLoadOptions(configuredDefaultEncoding: configured);
            var automatic = new DocumentLoadOptions(configuredDefaultEncoding: configured,
                decodingMode: DocumentDecodingMode.AutoDetect);
            var explicitOptions = new DocumentLoadOptions(configured, configured,
                DocumentDecodingMode.AutoDetect);
            using (var defaultSnapshot = await DocumentTextPipeline.DecodeFileAsync(input, defaults, owner))
            using (var autoSnapshot = await DocumentTextPipeline.DecodeFileAsync(input, automatic, owner))
            using (var explicitSnapshot = await DocumentTextPipeline.DecodeFileAsync(input, explicitOptions, owner))
            {
                var configuredText = LineEndingUtility.ApplyLineEnding(configured.GetString(raw), LineEnding.Cr);
                Assert(await defaultSnapshot.ReadTextAsync() == configuredText,
                    "Configured decoding did not retain the user's selected default.");
                Assert(await autoSnapshot.ReadTextAsync() == "abcd\r",
                    "Auto detection reused the configured decoding instead of its bounded source sample.");
                Assert(await explicitSnapshot.ReadTextAsync() == configuredText,
                    "An explicit encoding failed to override auto-detect mode.");
            }

            var bomEncoding = new UTF8Encoding(true);
            raw = bomEncoding.GetPreamble().Concat(bomEncoding.GetBytes("BOM العربية\r\n")).ToArray();
            await FileIO.WriteBytesAsync(input, raw);
            using (var snapshot = await DocumentTextPipeline.DecodeFileAsync(input, explicitOptions, owner))
            {
                Assert(await snapshot.ReadTextAsync() == "BOM العربية\r" && snapshot.Encoding.GetPreamble().Length == 3,
                    "The source BOM failed to override an explicit or configured encoding.");
            }

            raw = new byte[DocumentTextCodec.BufferSize + 4];
            for (var index = 0; index < raw.Length - 4; index++) raw[index] = (byte)'x';
            raw[raw.Length - 4] = 0xFF;
            raw[raw.Length - 3] = (byte)'b';
            raw[raw.Length - 2] = (byte)'\r';
            raw[raw.Length - 1] = (byte)'\n';
            await FileIO.WriteBytesAsync(input, raw);
            var fallback = EncodingCatalog.GetFallbackEncoding();
            using (var snapshot = await DocumentTextPipeline.DecodeFileAsync(input, automatic, owner))
            {
                Assert(await snapshot.ReadTextAsync() == LineEndingUtility.ApplyLineEnding(fallback.GetString(raw), LineEnding.Cr),
                    "A late strict UTF-8 failure did not restart the complete source with the established fallback.");
            }
            var disk = await FileIO.ReadBufferAsync(input);
            Assert(disk.ToArray().SequenceEqual(raw), "Encoding retries modified the source file.");
            await FileIO.WriteBytesAsync(input, []);
            using (var empty = await DocumentTextPipeline.DecodeFileAsync(input, automatic, owner))
                Assert(empty.Baseline.ByteLength == 0, "Auto detection rejected a valid empty file.");
        }
        finally
        {
            await folder.DeleteAsync(StorageDeleteOption.PermanentDelete);
        }
    }

    private static async Task CheckSnapshotAndReaderOwnershipAsync(StorageFolder folder, Guid owner)
    {
        // The first StreamWriter block ends in the middle of this surrogate pair.
        var original = "\uFEFF" + new string('a', 16381) + "😀\r\nالعربية\0\rNext\n";
        var canonical = LineEndingUtility.ApplyLineEnding(original, LineEnding.Cr);
        var bytes = new UTF8Encoding(false, true).GetBytes(canonical);
        var baseline = await DocumentBaseline.FromTextAsync(original, ownerId: owner);
        DocumentSnapshot snapshot = null;
        DocumentSnapshot retainedSnapshot = null;
        Stream reader = null;
        try
        {
            Assert(baseline.OwnerId == owner && baseline.GenerationId != Guid.Empty,
                "A baseline lost its owner or generation identity.");
            Assert(baseline.ByteLength == bytes.Length && baseline.Sha256 == Hash(bytes),
                "Canonical byte length or SHA-256 does not describe the stored bytes.");
            snapshot = new DocumentSnapshot(baseline, new UTF8Encoding(true), LineEnding.Crlf, 1234);
            retainedSnapshot = snapshot.Retain();
            var encodingCopy = snapshot.Encoding;
            encodingCopy.EncoderFallback = new EncoderReplacementFallback("changed");
            Assert(((EncoderReplacementFallback)snapshot.Encoding.EncoderFallback).DefaultString != "changed",
                "Changing exposed encoding metadata mutated the snapshot.");
            Assert(snapshot.LineEnding == LineEnding.Crlf && snapshot.DateModifiedFileTime == 1234,
                "Saved metadata was confused with the canonical representation.");
            var name = baseline.File.Name;
            await baseline.DisposeAsync();
            await snapshot.DisposeAsync();
            Assert(string.Equals(await retainedSnapshot.ReadTextAsync(), canonical, StringComparison.Ordinal),
                "Disposing other owners changed a retained snapshot or lost leading FEFF/NUL/non-BMP text.");
            reader = await retainedSnapshot.Baseline.OpenReadStreamAsync();
            await retainedSnapshot.DisposeAsync();
            Assert(await ExistsAsync(folder, name), "A live reader lost its immutable backing file.");
            using (var textReader = new StreamReader(reader, new UTF8Encoding(false, true), false, 1024, leaveOpen: true))
            {
                Assert(string.Equals(await textReader.ReadToEndAsync(), canonical, StringComparison.Ordinal),
                    "A reader did not preserve canonical text after its snapshot closed.");
            }
            reader.Dispose();
            reader = null;
            await baseline.DisposeAsync();
            Assert(!await ExistsAsync(folder, name), "The final temporary lease did not remove its generation.");
            await AssertThrowsAsync<ObjectDisposedException>(() => Task.FromResult(baseline.Retain()));
        }
        finally
        {
            reader?.Dispose();
            if (retainedSnapshot != null) await retainedSnapshot.DisposeAsync();
            if (snapshot != null) await snapshot.DisposeAsync();
            await baseline.DisposeAsync();
        }
    }

    private static async Task CheckCanonicalChunksAsync(Guid owner)
    {
        const string canonical = "\uFEFFالعربية😀\0\rnext";
        var bytes = new UTF8Encoding(false, true).GetBytes(canonical);
        var baseline = await DocumentBaseline.CreateAsync(async stream =>
        {
            // Exercise UTF-8 sequences split across writes rather than relying
            // on a decoder that receives whole characters in every block.
            for (var i = 0; i < bytes.Length; i++) await stream.WriteAsync(bytes, i, 1);
        }, ownerId: owner);
        try
        {
            using (var snapshot = new DocumentSnapshot(baseline, Encoding.ASCII, LineEnding.Lf))
            {
                Assert(await snapshot.ReadTextAsync() == canonical,
                    "Canonical storage used the user save encoding or broke split UTF-8 sequences.");
            }
            Assert(baseline.Sha256 == Hash(bytes), "Split writes changed the generation identity.");
        }
        finally { await baseline.DisposeAsync(); }
    }

    private static async Task CheckFailedCandidatesAsync(StorageFolder folder, Guid owner)
    {
        await AssertThrowsAsync<IOException>(() => DocumentBaseline.CreateAsync(async stream =>
        {
            await stream.WriteAsync([65], 0, 1);
            throw new IOException("Injected baseline writer failure.");
        }, ownerId: owner));
        await AssertThrowsAsync<DecoderFallbackException>(() => DocumentBaseline.CreateAsync(
            stream => stream.WriteAsync([0xF0, 0x9F], 0, 2), ownerId: owner));
        await AssertThrowsAsync<InvalidDataException>(() => DocumentBaseline.CreateAsync(
            stream => stream.WriteAsync([65, 10], 0, 2), ownerId: owner));
        await AssertThrowsAsync<InvalidOperationException>(() => DocumentBaseline.CreateAsync(async stream =>
        {
            try { await stream.WriteAsync([10], 0, 1); }
            catch (InvalidDataException) { }
        }, ownerId: owner));
        using (var cancellation = new CancellationTokenSource())
        {
            await AssertThrowsAsync<OperationCanceledException>(() => DocumentBaseline.CreateAsync(async stream =>
            {
                await stream.WriteAsync([65], 0, 1);
                cancellation.Cancel();
            }, owner, cancellation.Token));
        }
        Assert(!(await folder.GetFilesAsync()).Any(file => file.Name.StartsWith(
            owner.ToString("N") + "-", StringComparison.Ordinal)),
            "A failed or canceled candidate left a generation behind.");
    }

    private static async Task CheckRecoveryOwnershipAsync(StorageFolder folder, Guid owner)
    {
        var baseline = await DocumentBaseline.FromTextAsync("durable recovery\r中文", ownerId: owner);
        var file = baseline.File;
        DocumentBaseline reopened = null;
        try
        {
            baseline.PreserveForRecovery();
            await baseline.DisposeAsync();
            Assert(await ExistsAsync(folder, file.Name), "A manifest-owned recovery generation was deleted with its tab.");
            reopened = await DocumentBaseline.OpenExistingAsync(file, owner, baseline.GenerationId,
                baseline.ByteLength, baseline.Sha256);
            Assert(reopened.GenerationId == baseline.GenerationId, "Opening recovery replaced its immutable generation identity.");
            Assert(!await DocumentAssetLease.TryDeleteUnusedAsync(file), "GC removed an independently retained recovery generation.");
            await reopened.DisposeAsync();
            Assert(await DocumentAssetLease.TryDeleteUnusedAsync(file), "GC did not remove an unreferenced persistent generation.");
        }
        finally
        {
            if (reopened != null) await reopened.DisposeAsync();
            await baseline.DisposeAsync();
            if (await ExistsAsync(folder, file.Name)) await DocumentAssetLease.TryDeleteUnusedAsync(file);
        }
    }

    private static async Task CheckPreparedGenerationOwnershipAsync(StorageFolder folder, Guid owner)
    {
        var started = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        var preparation = DocumentBaseline.CreateAsync(async stream =>
        {
            await stream.WriteAsync([65], 0, 1);
            started.TrySetResult(true);
            await release.Task;
        }, ownerId: owner);
        DocumentBaseline baseline = null;
        try
        {
            await started.Task;
            var file = (await folder.GetFilesAsync()).Single(candidate => candidate.Name.StartsWith(
                owner.ToString("N") + "-", StringComparison.Ordinal));
            Assert(!await DocumentAssetLease.TryDeleteUnusedAsync(file), "GC deleted a generation while its writer was preparing it.");
        }
        finally
        {
            release.TrySetResult(true);
            baseline = await preparation;
            await baseline.DisposeAsync();
        }
    }

    private static void CheckJournalCompactionPolicy()
    {
        const ulong MiB = 1024 * 1024;
        Assert(DocumentJournal.ShouldCompact(4 * MiB, 1024) && DocumentJournal.ShouldCompact(4 * MiB, MiB) &&
            DocumentJournal.ShouldCompact(5 * MiB, 0), "A small document with a large journal was not compacted.");
        Assert(!DocumentJournal.ShouldCompact(4 * MiB - 1, 0), "A journal below the 4 MiB floor was compacted.");
        Assert(!DocumentJournal.ShouldCompact(4 * MiB, MiB + 1) && !DocumentJournal.ShouldCompact(64 * MiB - 1, 20 * MiB),
            "A large document whose journal is under four times its size was compacted.");
        Assert(DocumentJournal.ShouldCompact(64 * MiB, 100 * MiB), "A 64 MiB journal was not compacted.");
    }

    private static async Task CheckEmptyGenerationAsync(Guid owner)
    {
        var baseline = DocumentBaseline.CreateEmpty(owner);
        try
        {
            Assert(baseline.File == null && baseline.OwnerId == owner && baseline.ByteLength == 0 &&
                baseline.Sha256 == Hash([]), "An empty generation requires disk storage or has incorrect identity.");
            using (var snapshot = new DocumentSnapshot(baseline, Encoding.UTF8, LineEnding.Crlf))
            {
                await baseline.DisposeAsync();
                Assert(await snapshot.ReadTextAsync() == string.Empty, "An empty snapshot could not be independently retained.");
            }
        }
        finally { await baseline.DisposeAsync(); }
    }

    private static string Hash(byte[] bytes)
    {
        using (var hash = SHA256.Create())
            return BitConverter.ToString(hash.ComputeHash(bytes)).Replace("-", string.Empty).ToLowerInvariant();
    }

    private static async Task<bool> ExistsAsync(StorageFolder folder, string name)
    {
        return await folder.TryGetItemAsync(name) != null;
    }

    private static async Task AssertThrowsAsync<TException>(Func<Task> operation) where TException : Exception
    {
        try { await operation(); }
        catch (TException) { return; }
        throw new Exception("Expected " + typeof(TException).Name + " was not raised.");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new Exception(message);
    }
}
