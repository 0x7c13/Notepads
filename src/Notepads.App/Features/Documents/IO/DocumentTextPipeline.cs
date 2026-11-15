// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Documents.Text;
using Windows.Storage;

namespace Notepads.Features.Documents.IO;

internal static class DocumentTextPipeline
{
    public static async Task<DocumentSnapshot> DecodeFileAsync(
        StorageFile file,
        DocumentLoadOptions options,
        Guid ownerId,
        IProgress<DocumentTextProgress> progress = null,
        CancellationToken cancellationToken = default,
        long maximumInputBytes = long.MaxValue)
    {
        ArgumentNullException.ThrowIfNull(file);
        ArgumentNullException.ThrowIfNull(options);
        var selected = options.GetInitialEncoding();
        cancellationToken.ThrowIfCancellationRequested();
        var properties = await file.GetBasicPropertiesAsync();
        cancellationToken.ThrowIfCancellationRequested();
        if (maximumInputBytes < 0 || properties.Size > (ulong)maximumInputBytes)
            throw new NotSupportedException("The original file exceeds the supported input size.");

        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        var sample = new byte[DocumentTextCodec.BufferSize];
        using var source = await file.OpenStreamForReadAsync().ConfigureAwait(false);
        cancellationToken.ThrowIfCancellationRequested();
        if (!source.CanSeek)
            throw new NotSupportedException("Document loading requires a seekable source for complete encoding retries.");
        if ((ulong)source.Length != properties.Size)
            throw new IOException("The file changed before document loading started.");

        async Task ValidateSourceAsync()
        {
            cancellationToken.ThrowIfCancellationRequested();
            var current = await file.GetBasicPropertiesAsync();
            cancellationToken.ThrowIfCancellationRequested();
            if (current.Size != properties.Size || current.DateModified != properties.DateModified ||
                (ulong)source.Length != properties.Size)
            {
                throw new IOException("The file changed while its document was being loaded.");
            }
        }

        return await DecodeSourceAsync(source, sample, selected, ownerId, properties.Size,
            ValidateSourceAsync, () => properties.DateModified.ToFileTime(), progress,
            cancellationToken).ConfigureAwait(false);
    }

    /// <summary>
    /// Decode the complete immutable snapshot without taking ownership of its stream.
    /// The caller keeps the seekable source open and unchanged through all retries.
    /// </summary>
    public static async Task<DocumentSnapshot> DecodeSnapshotAsync(
        Stream source,
        DocumentLoadOptions options,
        Guid ownerId,
        IProgress<DocumentTextProgress> progress = null,
        CancellationToken cancellationToken = default,
        long maximumInputBytes = long.MaxValue)
    {
        ArgumentNullException.ThrowIfNull(source);
        ArgumentNullException.ThrowIfNull(options);
        var selected = options.GetInitialEncoding();
        cancellationToken.ThrowIfCancellationRequested();
        if (!source.CanRead)
            throw new ArgumentException("A readable input stream is required.", nameof(source));
        if (!source.CanSeek)
            throw new NotSupportedException("Document loading requires a seekable source for complete encoding retries.");
        var sourceLength = source.Length;
        if (maximumInputBytes < 0 || sourceLength > maximumInputBytes)
            throw new NotSupportedException("The original file exceeds the supported input size.");

        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        var sample = new byte[DocumentTextCodec.BufferSize];
        source.Position = 0;

        Task ValidateSourceAsync()
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (source.Length != sourceLength)
                throw new IOException("The snapshot stream changed while its document was being loaded.");
            return Task.CompletedTask;
        }

        return await DecodeSourceAsync(source, sample, selected, ownerId, (ulong)sourceLength,
            ValidateSourceAsync, () => 0, progress, cancellationToken).ConfigureAwait(false);
    }

    private static async Task<DocumentSnapshot> DecodeSourceAsync(
        Stream source,
        byte[] sample,
        Encoding selected,
        Guid ownerId,
        ulong expectedInputBytes,
        Func<Task> validateSourceAsync,
        Func<long> getDateModifiedFileTime,
        IProgress<DocumentTextProgress> progress,
        CancellationToken cancellationToken)
    {
        var sampleCount = 0;
        while (sampleCount < sample.Length)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var count = await source.ReadAsync(sample, sampleCount, sample.Length - sampleCount, cancellationToken).ConfigureAwait(false);
            if (count == 0) break;
            sampleCount += count;
        }

        await validateSourceAsync().ConfigureAwait(false);
        // Retain the existing UTF-7-signature behavior: StreamReader's BOM
        // detector does not recognize UTF-7 and starts with default UTF-8.
        var legacyUtf7Signature = sampleCount >= 3 && sample[0] == 0x2b && sample[1] == 0x2f && sample[2] == 0x76;
        var bomEncoding = DocumentTextCodec.DetectPreamble(sample.AsSpan(0, sampleCount), new UTF8Encoding(false), out var knownPreamble);
        if (knownPreamble > 0)
        {
            selected = bomEncoding;
        }
        else if (selected == null && legacyUtf7Signature)
        {
            selected = new UTF8Encoding(false);
        }
        else if (selected == null)
        {
            using var boundedSample = new MemoryStream(sample, 0, sampleCount, writable: false);
            selected = DocumentEncodingPolicy.TryGuessEncoding(boundedSample, out var guessed) ?
                guessed : new UTF8Encoding(false, true);
        }

        async Task<DocumentSnapshot> DecodeCandidateAsync(Encoding candidateEncoding)
        {
            // A fallback owns a fresh output and rewinds the same source
            // lease; it cannot accidentally observe a newly opened file.
            await validateSourceAsync().ConfigureAwait(false);
            source.Position = 0;
            var effectiveEncoding = DocumentTextCodec.DetectPreamble(sample.AsSpan(0, sampleCount), candidateEncoding, out var preambleLength);
            DocumentDecodeResult decoded = null;
            var baseline = await DocumentBaseline.CreateAsync(async canonical =>
            {
                decoded = await DocumentTextCodec.DecodeToCanonicalUtf8Async(source, canonical,
                    effectiveEncoding, preambleLength, progress, cancellationToken).ConfigureAwait(false);
            }, ownerId, cancellationToken).ConfigureAwait(false);
            try
            {
                await validateSourceAsync().ConfigureAwait(false);
                if ((ulong)decoded.InputBytes != expectedInputBytes)
                    throw new IOException("The file changed while its document was being loaded.");
                var savedEncoding = decoded.Encoding is UTF8Encoding ? new UTF8Encoding(preambleLength == 3) : decoded.Encoding;
                return new DocumentSnapshot(baseline, savedEncoding, decoded.LineEnding, getDateModifiedFileTime());
            }
            finally { await baseline.DisposeAsync().ConfigureAwait(false); }
        }

        return await DocumentTextCodec.RetryDecodeAsync(DecodeCandidateAsync, selected,
            EncodingCatalog.GetFallbackEncoding()).ConfigureAwait(false);
    }
}
