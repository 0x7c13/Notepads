// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
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
            await CheckSpanAndMemoryStreamsAsync(owner);
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
            await CheckAutoDetectedEncodingsAsync(input, owner);
            await FileIO.WriteBytesAsync(input, []);
            using var empty = await DocumentTextPipeline.DecodeFileAsync(input, automatic, owner);
            Assert(empty.Baseline.ByteLength == 0, "Auto detection rejected a valid empty file.");
        }
        finally
        {
            await folder.DeleteAsync(StorageDeleteOption.PermanentDelete);
        }
    }

    private static async Task CheckAutoDetectedEncodingsAsync(StorageFile input, Guid owner)
    {
        // These known texts cross the detector's internal chunk boundary. Verify
        // decoded content and retained save encoding through the production pipeline,
        // not the detector's own confidence or a particular alias for a code page.
        var samples = new (int CodePage, string Text)[]
        {
            (65001, "Plain text document. Editing, saving and restoring remain consistent. 0123456789\r\n"),
            (65001, "中文文本，测试自动识别文件编码。繁體中文與日本語、العربية、한국어、Русский、café、😀。\r\n"),
            (936, "这是用于检测文件编码的中文文档。打开文件以后，编辑器应该正确显示所有文字，保存和重新加载不能改变原有内容。春天的风吹过城市，书桌上放着一本关于计算机科学的书。\r\n"),
            (54936, "这是用于检测文件编码的中文文档。打开文件以后，编辑器应该正确显示所有文字，保存和重新加载不能改变原有内容。春天的风吹过城市，书桌上放着一本关于计算机科学的书。𠀀😀\r\n"),
            (950, "這是用於檢測檔案編碼的繁體中文文件。開啟檔案以後，編輯器應該正確顯示所有文字，儲存和重新載入不能改變原有內容。春天的風吹過城市，書桌上放著一本關於電腦科學的書。\r\n"),
            (932, "これは文字コードを自動的に検出するための日本語の文章です。ファイルを開いて編集してから保存しても、元の文字列が正しく表示されなければなりません。春の風が街を吹き抜けています。\r\n"),
            (51932, "これは文字コードを自動的に検出するための日本語の文章です。ファイルを開いて編集してから保存しても、元の文字列が正しく表示されなければなりません。春の風が街を吹き抜けています。\r\n"),
            (949, "이 문서는 문자 인코딩을 자동으로 감지하는 기능을 확인하기 위한 한국어 문서입니다. 파일을 열고 내용을 편집한 다음 저장해도 모든 문자가 정확하게 표시되어야 합니다. 오늘은 날씨가 좋습니다.\r\n"),
            (1251, "Это русский текст для проверки автоматического определения кодировки. Редактор должен правильно открывать документы, сохранять изменения и восстанавливать исходное содержимое без потери символов.\r\n"),
            (1252, "L’été arrive bientôt. François prépare son café et écrit une lettre à sa sœur. Déjà, les fenêtres éclairées révèlent une agréable lumière. Grüße aus München, für größere Möglichkeiten.\r\n")
        };
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        var automatic = new DocumentLoadOptions(decodingMode: DocumentDecodingMode.AutoDetect);
        foreach (var (codePage, sample) in samples)
        {
            var text = string.Concat(Enumerable.Repeat(sample, 40));
            var encoding = Encoding.GetEncoding(codePage, EncoderFallback.ExceptionFallback, DecoderFallback.ExceptionFallback);
            var bytes = encoding.GetBytes(text);
            await FileIO.WriteBytesAsync(input, bytes);
            using var snapshot = await DocumentTextPipeline.DecodeFileAsync(input, automatic, owner);
            Assert(await snapshot.ReadTextAsync() == text.Replace("\r\n", "\r", StringComparison.Ordinal),
                $"Auto detection changed the text for code page {codePage}.");
            Assert(snapshot.Encoding.GetBytes(text).SequenceEqual(bytes) && snapshot.LineEnding == LineEnding.Crlf,
                $"Auto detection lost the original byte encoding or line endings for code page {codePage}.");
            Assert((await FileIO.ReadBufferAsync(input)).ToArray().SequenceEqual(bytes),
                $"Auto detection modified the source file for code page {codePage}.");
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
        await AssertThrowsAsync<DecoderFallbackException>(() => DocumentBaseline.CreateAsync(
            stream => stream.WriteAsync(new byte[] { 0xF0, 0x9F }.AsMemory()).AsTask(), ownerId: owner));
        await AssertThrowsAsync<InvalidOperationException>(() => DocumentBaseline.CreateAsync(async stream =>
        {
            try { await stream.WriteAsync(new byte[] { 10 }.AsMemory()); }
            catch (InvalidDataException) { }
        }, ownerId: owner));
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
            using var snapshot = new DocumentSnapshot(baseline, Encoding.UTF8, LineEnding.Crlf);
            await baseline.DisposeAsync();
            Assert(await snapshot.ReadTextAsync() == string.Empty, "An empty snapshot could not be independently retained.");
        }
        finally { await baseline.DisposeAsync(); }
    }

    private static string Hash(byte[] bytes)
    {
        using var hash = SHA256.Create();
        return BitConverter.ToString(hash.ComputeHash(bytes)).Replace("-", string.Empty).ToLowerInvariant();
    }

    private static async Task CheckSpanAndMemoryStreamsAsync(Guid owner)
    {
        // Invalid bytes around the slice must neither be validated, hashed nor written.
        byte[] padded = [0xff, 0xe4, 0xb8, 0xad, 0xf0, 0x9f, 0x98, 0x80, 0x0d, 0x00, 0xff];
        var expected = padded[1..^1];
        var baseline = await DocumentBaseline.CreateAsync(async stream =>
        {
            stream.Write(padded.AsSpan(1, 2));
            await Task.Yield();
            await stream.WriteAsync(padded.AsMemory(3, padded.Length - 4));
        }, owner);
        try
        {
            Assert(baseline.ByteLength == expected.Length && baseline.Sha256 == Hash(expected),
                "Span and Memory writes changed the baseline bytes or hash.");
            using var reader = await baseline.OpenReadStreamAsync();
            var output = Enumerable.Repeat((byte)0xcc, expected.Length + 2).ToArray();
            var count = reader.Read(output.AsSpan(1, 2));
            Assert(await reader.ReadAsync(Memory<byte>.Empty) == 0 && reader.Position == count,
                "An empty Memory read advanced the lease.");
            while (count < expected.Length)
            {
                var read = await reader.ReadAsync(output.AsMemory(1 + count, expected.Length - count));
                Assert(read > 0, "The leased reader ended before its committed prefix.");
                count += read;
            }
            Assert(output[0] == 0xcc && output[^1] == 0xcc && output.AsSpan(1, expected.Length).SequenceEqual(expected),
                "The leased Span/Memory reader wrote outside the requested slice or changed its content.");
            Assert(await reader.ReadAsync(output.AsMemory(1, 1)) == 0, "A lease read past its committed prefix.");
        }
        finally { await baseline.DisposeAsync(); }
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
