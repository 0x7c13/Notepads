// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
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
        Guid? ownerId = null,
        IProgress<DocumentTextProgress> progress = null,
        CancellationToken cancellationToken = default)
    {
        if (file == null) throw new ArgumentNullException(nameof(file));
        if (options == null) throw new ArgumentNullException(nameof(options));
        var selected = options.GetInitialEncoding();
        cancellationToken.ThrowIfCancellationRequested();
        var properties = await file.GetBasicPropertiesAsync();
        cancellationToken.ThrowIfCancellationRequested();
        if (properties.Size > long.MaxValue)
            throw new NotSupportedException("The original file exceeds the supported input size.");

        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        var sample = new byte[DocumentTextCodec.BufferSize];
        var sampleCount = 0;
        using (var source = await file.OpenStreamForReadAsync().ConfigureAwait(false))
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (!source.CanSeek)
                throw new NotSupportedException("Document loading requires a seekable source for complete encoding retries.");
            if ((ulong)source.Length != properties.Size)
                throw new IOException("The file changed before document loading started.");

            while (sampleCount < sample.Length)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var count = await source.ReadAsync(sample, sampleCount, sample.Length - sampleCount, cancellationToken).ConfigureAwait(false);
                if (count == 0) break;
                sampleCount += count;
            }

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

            await ValidateSourceAsync().ConfigureAwait(false);
            // Retain the existing UTF-7-signature behavior: StreamReader's BOM
            // detector does not recognize UTF-7 and starts with default UTF-8.
            var legacyUtf7Signature = sampleCount >= 3 && sample[0] == 0x2b && sample[1] == 0x2f && sample[2] == 0x76;
            var bomEncoding = DocumentTextCodec.DetectPreamble(sample, sampleCount, new UTF8Encoding(false), out var knownPreamble);
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
                using (var boundedSample = new MemoryStream(sample, 0, sampleCount, writable: false))
                {
                    selected = DocumentEncodingPolicy.TryGuessEncoding(boundedSample, out var guessed) ?
                        guessed : new UTF8Encoding(false, true);
                }
            }

            async Task<DocumentSnapshot> DecodeCandidateAsync(Encoding candidateEncoding)
            {
                // A fallback owns a fresh output and rewinds the same source
                // lease; it cannot accidentally observe a newly opened file.
                await ValidateSourceAsync().ConfigureAwait(false);
                source.Position = 0;
                var effectiveEncoding = DocumentTextCodec.DetectPreamble(sample, sampleCount, candidateEncoding, out var preambleLength);
                DocumentDecodeResult decoded = null;
                var baseline = await DocumentBaseline.CreateAsync(async canonical =>
                {
                    decoded = await DocumentTextCodec.DecodeToCanonicalUtf8Async(source, canonical,
                        effectiveEncoding, preambleLength, progress, cancellationToken).ConfigureAwait(false);
                }, cancellationToken, ownerId).ConfigureAwait(false);
                try
                {
                    await ValidateSourceAsync().ConfigureAwait(false);
                    if ((ulong)decoded.InputBytes != properties.Size)
                        throw new IOException("The file changed while its document was being loaded.");
                    var savedEncoding = decoded.Encoding is UTF8Encoding ? new UTF8Encoding(preambleLength == 3) : decoded.Encoding;
                    return new DocumentSnapshot(baseline, savedEncoding, decoded.LineEnding, properties.DateModified.ToFileTime());
                }
                finally { await baseline.DisposeAsync().ConfigureAwait(false); }
            }

            return await DocumentTextCodec.RetryDecodeAsync(DecodeCandidateAsync, selected,
                EncodingCatalog.GetFallbackEncoding()).ConfigureAwait(false);
        }
    }
}
