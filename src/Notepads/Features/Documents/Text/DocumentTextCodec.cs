// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;

namespace Notepads.Features.Documents.Text;

internal sealed class DocumentTextProgress
{
    public DocumentTextProgress(long inputBytes, long outputBytes)
    {
        InputBytes = inputBytes;
        OutputBytes = outputBytes;
    }

    public long InputBytes { get; }
    public long OutputBytes { get; }
}

internal sealed class DocumentDecodeResult
{
    public DocumentDecodeResult(Encoding encoding, LineEnding lineEnding, long inputBytes, long canonicalBytes)
    {
        Encoding = (Encoding)encoding.Clone();
        LineEnding = lineEnding;
        InputBytes = inputBytes;
        CanonicalBytes = canonicalBytes;
    }

    public Encoding Encoding { get; }
    public LineEnding LineEnding { get; }
    public long InputBytes { get; }
    public long CanonicalBytes { get; }
}

/// <summary>
/// Fixed-memory conversion between encoded files and the editor's UTF-8/CR
/// representation. Callers own streams and publish completed candidates.
/// </summary>
internal static class DocumentTextCodec
{
    internal const int BufferSize = 64 * 1024;
    private static readonly Encoding CanonicalEncoding = new UTF8Encoding(false, true);

    public static async Task<DocumentDecodeResult> DecodeToCanonicalUtf8Async(
        Stream input,
        Stream canonicalOutput,
        Encoding sourceEncoding,
        int preambleLength = 0,
        IProgress<DocumentTextProgress> progress = null,
        CancellationToken cancellationToken = default,
        long maximumCanonicalBytes = DocumentLimits.MaximumCanonicalByteLength)
    {
        ValidateStreams(input, canonicalOutput);
        if (sourceEncoding == null) throw new ArgumentNullException(nameof(sourceEncoding));
        if (preambleLength < 0) throw new ArgumentOutOfRangeException(nameof(preambleLength));
        if (maximumCanonicalBytes < 0 || maximumCanonicalBytes > DocumentLimits.MaximumCanonicalByteLength)
            throw new ArgumentOutOfRangeException(nameof(maximumCanonicalBytes));

        sourceEncoding = (Encoding)sourceEncoding.Clone();
        var decoder = sourceEncoding.GetDecoder();
        var bytes = new byte[BufferSize];
        var characters = new char[sourceEncoding.GetMaxCharCount(BufferSize)];
        var canonicalCharacters = new char[characters.Length];
        var writer = new EncodedWriter(canonicalOutput, CanonicalEncoding, characters.Length, maximumCanonicalBytes);
        var endings = new InputLineEndings();
        long inputBytes = 0;

        while (preambleLength > 0)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var count = await input.ReadAsync(bytes, 0, Math.Min(preambleLength, bytes.Length), cancellationToken).ConfigureAwait(false);
            if (count == 0) throw new EndOfStreamException("The file changed before its encoding preamble could be read.");
            inputBytes += count;
            preambleLength -= count;
        }

        while (true)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var count = await input.ReadAsync(bytes, 0, bytes.Length, cancellationToken).ConfigureAwait(false);
            inputBytes = checked(inputBytes + count);
            var final = count == 0;
            var offset = 0;
            bool complete;
            do
            {
                cancellationToken.ThrowIfCancellationRequested();
                decoder.Convert(bytes, offset, count - offset, characters, 0, characters.Length, final,
                    out var bytesUsed, out var charactersUsed, out complete);
                offset += bytesUsed;
                var canonicalCount = endings.Normalize(characters, charactersUsed, canonicalCharacters);
                await writer.WriteAsync(canonicalCharacters, canonicalCount, final && complete, cancellationToken).ConfigureAwait(false);
                if (!complete && bytesUsed == 0 && charactersUsed == 0)
                    throw new InvalidDataException("The document decoder could not make progress.");
            }
            while (!complete);

            progress?.Report(new DocumentTextProgress(inputBytes, writer.BytesWritten));
            if (final) break;
        }

        endings.Complete();
        cancellationToken.ThrowIfCancellationRequested();
        return new DocumentDecodeResult(sourceEncoding, endings.DetectedEnding, inputBytes, writer.BytesWritten);
    }

    public static async Task<long> CopyFromNativeAsync(
        Stream canonicalUtf8Reader,
        Stream preparedCanonicalBaseline,
        Stream encodedDestination,
        Encoding requestedEncoding,
        LineEnding requestedLineEnding,
        IProgress<DocumentTextProgress> progress = null,
        CancellationToken cancellationToken = default)
    {
        ValidateStreams(canonicalUtf8Reader, preparedCanonicalBaseline);
        ValidateStreams(canonicalUtf8Reader, encodedDestination);
        if (ReferenceEquals(preparedCanonicalBaseline, encodedDestination))
            throw new ArgumentException("Canonical and encoded outputs must be separate streams.");
        if (requestedEncoding == null) throw new ArgumentNullException(nameof(requestedEncoding));
        var ending = GetLineEnding(requestedLineEnding);
        requestedEncoding = (Encoding)requestedEncoding.Clone();
        var decoder = CanonicalEncoding.GetDecoder();
        var bytes = new byte[BufferSize];
        var characters = new char[CanonicalEncoding.GetMaxCharCount(BufferSize)];
        var encodedCharacters = new char[checked(characters.Length * 2)];
        var writer = new EncodedWriter(encodedDestination, requestedEncoding, encodedCharacters.Length, long.MaxValue);
        long canonicalBytes = 0;
        var preamble = requestedEncoding.GetPreamble();
        cancellationToken.ThrowIfCancellationRequested();
        if (preamble.Length > 0)
            await encodedDestination.WriteAsync(preamble, 0, preamble.Length, cancellationToken).ConfigureAwait(false);

        while (true)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var count = await canonicalUtf8Reader.ReadAsync(bytes, 0, bytes.Length, cancellationToken).ConfigureAwait(false);
            if (count > DocumentLimits.MaximumCanonicalByteLength - canonicalBytes)
                throw new NotSupportedException("The document exceeds the supported canonical UTF-8 size.");
            canonicalBytes += count;
            if (count > 0)
                await preparedCanonicalBaseline.WriteAsync(bytes, 0, count, cancellationToken).ConfigureAwait(false);
            var final = count == 0;
            var offset = 0;
            bool complete;
            do
            {
                cancellationToken.ThrowIfCancellationRequested();
                decoder.Convert(bytes, offset, count - offset, characters, 0, characters.Length, final,
                    out var bytesUsed, out var charactersUsed, out complete);
                offset += bytesUsed;
                var convertedCount = 0;
                for (var index = 0; index < charactersUsed; index++)
                {
                    var character = characters[index];
                    if (character == '\n')
                        throw new InvalidDataException("The native document is not in canonical CR format.");
                    if (character == '\r')
                    {
                        foreach (var endingCharacter in ending) encodedCharacters[convertedCount++] = endingCharacter;
                    }
                    else
                    {
                        encodedCharacters[convertedCount++] = character;
                    }
                }
                await writer.WriteAsync(encodedCharacters, convertedCount, final && complete, cancellationToken).ConfigureAwait(false);
                if (!complete && bytesUsed == 0 && charactersUsed == 0)
                    throw new InvalidDataException("The native UTF-8 decoder could not make progress.");
            }
            while (!complete);

            progress?.Report(new DocumentTextProgress(canonicalBytes, checked(writer.BytesWritten + preamble.Length)));
            if (final) break;
        }
        cancellationToken.ThrowIfCancellationRequested();
        return canonicalBytes;
    }

    // The attempt owns its candidate and resets its source. Never append a
    // fallback decode to the already partially decoded first generation.
    public static async Task<T> RetryDecodeAsync<T>(Func<Encoding, Task<T>> attempt, Encoding encoding, Encoding fallbackEncoding)
    {
        if (attempt == null) throw new ArgumentNullException(nameof(attempt));
        if (encoding == null) throw new ArgumentNullException(nameof(encoding));
        if (fallbackEncoding == null) throw new ArgumentNullException(nameof(fallbackEncoding));
        try { return await attempt(encoding).ConfigureAwait(false); }
        catch (DecoderFallbackException) { return await attempt(fallbackEncoding).ConfigureAwait(false); }
    }

    internal static Encoding DetectPreamble(byte[] sample, int count, Encoding selectedEncoding, out int preambleLength)
    {
        if (sample == null) throw new ArgumentNullException(nameof(sample));
        if (count < 0 || count > sample.Length) throw new ArgumentOutOfRangeException(nameof(count));
        if (selectedEncoding == null) throw new ArgumentNullException(nameof(selectedEncoding));
        Encoding encoding = selectedEncoding;
        preambleLength = 0;
        if (count >= 4 && sample[0] == 0xff && sample[1] == 0xfe && sample[2] == 0 && sample[3] == 0)
        {
            encoding = new UTF32Encoding(false, true);
            preambleLength = 4;
        }
        else if (count >= 4 && sample[0] == 0 && sample[1] == 0 && sample[2] == 0xfe && sample[3] == 0xff)
        {
            encoding = new UTF32Encoding(true, true);
            preambleLength = 4;
        }
        else if (count >= 3 && sample[0] == 0xef && sample[1] == 0xbb && sample[2] == 0xbf)
        {
            encoding = new UTF8Encoding(true);
            preambleLength = 3;
        }
        else if (count >= 2 && sample[0] == 0xff && sample[1] == 0xfe)
        {
            encoding = new UnicodeEncoding(false, true);
            preambleLength = 2;
        }
        else if (count >= 2 && sample[0] == 0xfe && sample[1] == 0xff)
        {
            encoding = new UnicodeEncoding(true, true);
            preambleLength = 2;
        }
        else
        {
            var preamble = encoding.GetPreamble();
            if (preamble.Length > 0 && count >= preamble.Length)
            {
                var matches = true;
                for (var index = 0; index < preamble.Length; index++) matches &= sample[index] == preamble[index];
                if (matches) preambleLength = preamble.Length;
            }
        }
        return encoding;
    }

    private static void ValidateStreams(Stream input, Stream output)
    {
        if (input == null) throw new ArgumentNullException(nameof(input));
        if (output == null) throw new ArgumentNullException(nameof(output));
        if (!input.CanRead) throw new ArgumentException("A readable input stream is required.", nameof(input));
        if (!output.CanWrite) throw new ArgumentException("A writable output stream is required.", nameof(output));
        if (ReferenceEquals(input, output)) throw new ArgumentException("Input and output must be separate streams.");
    }

    private static string GetLineEnding(LineEnding ending)
    {
        return ending switch
        {
            LineEnding.Cr => "\r",
            LineEnding.Lf => "\n",
            LineEnding.Crlf => "\r\n",
            _ => throw new ArgumentOutOfRangeException(nameof(ending)),
        };
    }

    private sealed class EncodedWriter
    {
        private readonly Stream _output;
        private readonly Encoder _encoder;
        private readonly byte[] _bytes;
        private readonly long _maximumBytes;

        public EncodedWriter(Stream output, Encoding encoding, int characterCapacity, long maximumBytes)
        {
            _output = output;
            _encoder = encoding.GetEncoder();
            _bytes = new byte[encoding.GetMaxByteCount(characterCapacity)];
            _maximumBytes = maximumBytes;
        }

        public long BytesWritten { get; private set; }

        public async Task WriteAsync(char[] characters, int count, bool final, CancellationToken cancellationToken)
        {
            var offset = 0;
            bool complete;
            do
            {
                cancellationToken.ThrowIfCancellationRequested();
                _encoder.Convert(characters, offset, count - offset, _bytes, 0, _bytes.Length, final,
                    out var charactersUsed, out var bytesUsed, out complete);
                offset += charactersUsed;
                if (bytesUsed > _maximumBytes - BytesWritten)
                    throw new NotSupportedException("The document exceeds the supported encoded size.");
                if (bytesUsed > 0)
                {
                    await _output.WriteAsync(_bytes, 0, bytesUsed, cancellationToken).ConfigureAwait(false);
                    BytesWritten += bytesUsed;
                }
                if (!complete && bytesUsed == 0 && charactersUsed == 0)
                    throw new InvalidDataException("The document encoder could not make progress.");
            }
            while (!complete);
        }
    }

    private sealed class InputLineEndings
    {
        private bool _afterCr;
        private bool _hasCr;
        private bool _hasLf;
        private bool _hasCrlf;

        public LineEnding DetectedEnding => _hasCrlf ? LineEnding.Crlf : _hasCr ? LineEnding.Cr : _hasLf ? LineEnding.Lf : LineEnding.Crlf;

        public int Normalize(char[] input, int count, char[] output)
        {
            var written = 0;
            for (var index = 0; index < count; index++)
            {
                var character = input[index];
                if (_afterCr)
                {
                    _afterCr = false;
                    if (character == '\n')
                    {
                        _hasCrlf = true;
                        continue;
                    }
                    _hasCr = true;
                }
                if (character == '\r') _afterCr = true;
                else if (character == '\n') _hasLf = true;
                output[written++] = character == '\n' ? '\r' : character;
            }
            return written;
        }

        public void Complete()
        {
            if (_afterCr) _hasCr = true;
        }
    }
}
