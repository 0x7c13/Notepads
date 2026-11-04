// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Buffers;
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
        ArgumentNullException.ThrowIfNull(sourceEncoding);
        ArgumentOutOfRangeException.ThrowIfNegative(preambleLength);
        if (maximumCanonicalBytes < 0 || maximumCanonicalBytes > DocumentLimits.MaximumCanonicalByteLength)
            throw new ArgumentOutOfRangeException(nameof(maximumCanonicalBytes));

        sourceEncoding = (Encoding)sourceEncoding.Clone();
        var decoder = sourceEncoding.GetDecoder();
        var characterCapacity = sourceEncoding.GetMaxCharCount(BufferSize);
        using var byteBuffer = MemoryPool<byte>.Shared.Rent(BufferSize);
        using var characterBuffer = MemoryPool<char>.Shared.Rent(characterCapacity);
        using var canonicalBuffer = MemoryPool<char>.Shared.Rent(characterCapacity);
        using var encodedBuffer = MemoryPool<byte>.Shared.Rent(CanonicalEncoding.GetMaxByteCount(characterCapacity));
        var bytes = byteBuffer.Memory[..BufferSize];
        var characters = characterBuffer.Memory[..characterCapacity];
        var canonicalCharacters = canonicalBuffer.Memory[..characterCapacity];
        var writer = new EncodedWriter(canonicalOutput, CanonicalEncoding, encodedBuffer.Memory, maximumCanonicalBytes);
        var endings = new InputLineEndings();
        long inputBytes = 0;

        while (preambleLength > 0)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var count = await input.ReadAsync(bytes[..Math.Min(preambleLength, bytes.Length)], cancellationToken).ConfigureAwait(false);
            if (count == 0) throw new EndOfStreamException("The file changed before its encoding preamble could be read.");
            inputBytes += count;
            preambleLength -= count;
        }

        while (true)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var count = await input.ReadAsync(bytes, cancellationToken).ConfigureAwait(false);
            inputBytes = checked(inputBytes + count);
            var final = count == 0;
            var offset = 0;
            bool complete;
            do
            {
                cancellationToken.ThrowIfCancellationRequested();
                decoder.Convert(bytes.Span.Slice(offset, count - offset), characters.Span, final,
                    out var bytesUsed, out var charactersUsed, out complete);
                offset += bytesUsed;
                var canonicalCount = endings.Normalize(characters.Span[..charactersUsed], canonicalCharacters.Span);
                await writer.WriteAsync(canonicalCharacters[..canonicalCount], final && complete, cancellationToken).ConfigureAwait(false);
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
        ArgumentNullException.ThrowIfNull(requestedEncoding);
        var ending = GetLineEnding(requestedLineEnding);
        requestedEncoding = (Encoding)requestedEncoding.Clone();
        var decoder = CanonicalEncoding.GetDecoder();
        var characterCapacity = CanonicalEncoding.GetMaxCharCount(BufferSize);
        var encodedCharacterCapacity = checked(characterCapacity * 2);
        using var byteBuffer = MemoryPool<byte>.Shared.Rent(BufferSize);
        using var characterBuffer = MemoryPool<char>.Shared.Rent(characterCapacity);
        using var encodedCharacterBuffer = MemoryPool<char>.Shared.Rent(encodedCharacterCapacity);
        using var encodedBuffer = MemoryPool<byte>.Shared.Rent(requestedEncoding.GetMaxByteCount(encodedCharacterCapacity));
        var bytes = byteBuffer.Memory[..BufferSize];
        var characters = characterBuffer.Memory[..characterCapacity];
        var encodedCharacters = encodedCharacterBuffer.Memory[..encodedCharacterCapacity];
        var writer = new EncodedWriter(encodedDestination, requestedEncoding, encodedBuffer.Memory, long.MaxValue);
        long canonicalBytes = 0;
        var preamble = requestedEncoding.GetPreamble();
        cancellationToken.ThrowIfCancellationRequested();
        if (preamble.Length > 0)
            await encodedDestination.WriteAsync(preamble, cancellationToken).ConfigureAwait(false);

        while (true)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var count = await canonicalUtf8Reader.ReadAsync(bytes, cancellationToken).ConfigureAwait(false);
            if (count > DocumentLimits.MaximumCanonicalByteLength - canonicalBytes)
                throw new NotSupportedException("The document exceeds the supported canonical UTF-8 size.");
            canonicalBytes += count;
            if (count > 0)
                await preparedCanonicalBaseline.WriteAsync(bytes[..count], cancellationToken).ConfigureAwait(false);
            var final = count == 0;
            var offset = 0;
            bool complete;
            do
            {
                cancellationToken.ThrowIfCancellationRequested();
                decoder.Convert(bytes.Span.Slice(offset, count - offset), characters.Span, final,
                    out var bytesUsed, out var charactersUsed, out complete);
                offset += bytesUsed;
                var convertedCount = ExpandLineEndings(characters.Span[..charactersUsed], encodedCharacters.Span, ending);
                await writer.WriteAsync(encodedCharacters[..convertedCount], final && complete, cancellationToken).ConfigureAwait(false);
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
        ArgumentNullException.ThrowIfNull(attempt);
        ArgumentNullException.ThrowIfNull(encoding);
        ArgumentNullException.ThrowIfNull(fallbackEncoding);
        try { return await attempt(encoding).ConfigureAwait(false); }
        catch (DecoderFallbackException) { return await attempt(fallbackEncoding).ConfigureAwait(false); }
    }

    internal static Encoding DetectPreamble(ReadOnlySpan<byte> sample, Encoding selectedEncoding, out int preambleLength)
    {
        ArgumentNullException.ThrowIfNull(selectedEncoding);
        Encoding encoding = selectedEncoding;
        preambleLength = 0;
        if (sample.StartsWith<byte>([0xff, 0xfe, 0x00, 0x00]))
        {
            encoding = new UTF32Encoding(false, true);
            preambleLength = 4;
        }
        else if (sample.StartsWith<byte>([0x00, 0x00, 0xfe, 0xff]))
        {
            encoding = new UTF32Encoding(true, true);
            preambleLength = 4;
        }
        else if (sample.StartsWith<byte>([0xef, 0xbb, 0xbf]))
        {
            encoding = new UTF8Encoding(true);
            preambleLength = 3;
        }
        else if (sample.StartsWith<byte>([0xff, 0xfe]))
        {
            encoding = new UnicodeEncoding(false, true);
            preambleLength = 2;
        }
        else if (sample.StartsWith<byte>([0xfe, 0xff]))
        {
            encoding = new UnicodeEncoding(true, true);
            preambleLength = 2;
        }
        else
        {
            var preamble = encoding.Preamble;
            if (!preamble.IsEmpty && sample.StartsWith(preamble)) preambleLength = preamble.Length;
        }
        return encoding;
    }

    private static void ValidateStreams(Stream input, Stream output)
    {
        ArgumentNullException.ThrowIfNull(input);
        ArgumentNullException.ThrowIfNull(output);
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

    private static int ExpandLineEndings(ReadOnlySpan<char> input, Span<char> output, ReadOnlySpan<char> ending)
    {
        var written = 0;
        foreach (var character in input)
        {
            if (character == '\n')
                throw new InvalidDataException("The native document is not in canonical CR format.");
            if (character == '\r')
            {
                ending.CopyTo(output[written..]);
                written += ending.Length;
            }
            else output[written++] = character;
        }
        return written;
    }

    private sealed class EncodedWriter
    {
        private readonly Stream _output;
        private readonly Encoder _encoder;
        private readonly Memory<byte> _bytes;
        private readonly long _maximumBytes;

        public EncodedWriter(Stream output, Encoding encoding, Memory<byte> bytes, long maximumBytes)
        {
            _output = output;
            _encoder = encoding.GetEncoder();
            _bytes = bytes;
            _maximumBytes = maximumBytes;
        }

        public long BytesWritten { get; private set; }

        public async ValueTask WriteAsync(ReadOnlyMemory<char> characters, bool final, CancellationToken cancellationToken)
        {
            var offset = 0;
            bool complete;
            do
            {
                cancellationToken.ThrowIfCancellationRequested();
                _encoder.Convert(characters.Span[offset..], _bytes.Span, final,
                    out var charactersUsed, out var bytesUsed, out complete);
                offset += charactersUsed;
                if (bytesUsed > _maximumBytes - BytesWritten)
                    throw new NotSupportedException("The document exceeds the supported encoded size.");
                if (bytesUsed > 0)
                {
                    await _output.WriteAsync(_bytes[..bytesUsed], cancellationToken).ConfigureAwait(false);
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

        public int Normalize(ReadOnlySpan<char> input, Span<char> output)
        {
            var written = 0;
            for (var index = 0; index < input.Length; index++)
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
