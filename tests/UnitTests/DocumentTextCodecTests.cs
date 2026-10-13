// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Text;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Text;

namespace Notepads.UnitTests;

public sealed class DocumentTextCodecTests
{
    static DocumentTextCodecTests() => Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);

    private static readonly string[] Encodings =
    [
        "utf8", "utf8-bom", "utf16-le", "utf16-le-bom", "utf16-be", "utf16-be-bom",
        "utf32-le", "utf32-le-bom", "utf32-be", "utf32-be-bom", "1252", "1256", "932", "50220"
    ];

    public static IEnumerable<object[]> DecodeCases()
    {
        foreach (var encoding in Encodings)
        foreach (var sample in new[] { "empty", "leading-bom-content", "unicode", "mixed", "blank-lines", "buffer-boundary" })
        foreach (var readSize in new[] { 1, 7, DocumentTextCodec.BufferSize })
            yield return [encoding, sample, readSize];
    }

    [Theory]
    [MemberData(nameof(DecodeCases))]
    public async Task DecodeAcrossShortReadsAndCharacterBoundaries(string name, string sample, int readSize)
    {
        var encoding = EncodingFor(name);
        var text = Sample(sample);
        var encoded = encoding.GetPreamble().Concat(encoding.GetBytes(text)).ToArray();
        using var input = new ShortReadStream(encoded, readSize);
        using var output = new MemoryStream();
        var selected = DocumentTextCodec.DetectPreamble(encoded, encoding, out var preambleLength);
        // The BCL decoder is an independent reference for character/chunk behavior.
        // Fixed byte oracles below independently check BOM choice and EOL semantics.
        var decoded = selected.GetString(encoded, preambleLength, encoded.Length - preambleLength);
        var result = await DocumentTextCodec.DecodeToCanonicalUtf8Async(input, output, selected, preambleLength,
            cancellationToken: TestContext.Current.CancellationToken);
        Assert.Equal(Canonicalize(decoded), new UTF8Encoding(false, true).GetString(output.ToArray()));
        Assert.Equal(encoded.Length, result.InputBytes);
        Assert.Equal(output.Length, result.CanonicalBytes);
        input.AssertReadBoundaries();
        Assert.True(input.CanRead && output.CanWrite, "The codec must not dispose caller-owned streams.");
    }

    public static IEnumerable<object[]> SaveCases()
    {
        foreach (var encoding in Encodings)
        foreach (var ending in new[] { LineEnding.Cr, LineEnding.Lf, LineEnding.Crlf })
        foreach (var readSize in new[] { 1, 5, DocumentTextCodec.BufferSize })
            yield return [encoding, ending, readSize];
    }

    [Theory]
    [MemberData(nameof(SaveCases))]
    public async Task SavePreservesCanonicalBaselineAndUsesRequestedEncodingAndEol(string name, LineEnding ending, int readSize)
    {
        const string text = "\ufeffcontent\0中文 العربية 日本語 😀\rnext\r";
        var encoding = EncodingFor(name);
        var canonical = new UTF8Encoding(false, true).GetBytes(text);
        using var input = new ShortReadStream(canonical, readSize);
        using var baseline = new MemoryStream();
        using var destination = new MemoryStream();
        var copied = await DocumentTextCodec.CopyFromNativeAsync(input, baseline, destination, encoding, ending,
            cancellationToken: TestContext.Current.CancellationToken);
        Assert.Equal(canonical, baseline.ToArray());
        var eol = ending == LineEnding.Cr ? "\r" : ending == LineEnding.Lf ? "\n" : "\r\n";
        Assert.Equal(encoding.GetPreamble().Concat(encoding.GetBytes(text.Replace("\r", eol))).ToArray(), destination.ToArray());
        Assert.Equal(canonical.Length, copied);
        input.AssertReadBoundaries();
        Assert.True(input.CanRead && baseline.CanWrite && destination.CanWrite, "Save must retain caller-owned streams.");
    }

    public static TheoryData<string, string, int, LineEnding> FixedByteOracles => new()
    {
        { "utf8-bom", "EFBBBF410D0AC3A900", 3, LineEnding.Crlf },
        { "utf16-le-bom", "FFFE41000D000A00E9000000", 2, LineEnding.Crlf },
        { "utf16-be-bom", "FEFF0041000D000A00E90000", 2, LineEnding.Crlf },
        { "utf32-le-bom", "FFFE0000410000000D0000000A000000E900000000000000", 4, LineEnding.Crlf },
        { "utf32-be-bom", "0000FEFF000000410000000D0000000A000000E900000000", 4, LineEnding.Crlf },
        { "utf8", "410AC3A900", 0, LineEnding.Lf }
    };

    [Theory]
    [MemberData(nameof(FixedByteOracles))]
    public async Task FixedBytesDetermineBomEncodingAndCanonicalOutput(string name, string hex, int bomLength, LineEnding ending)
    {
        var bytes = Convert.FromHexString(hex);
        var selected = DocumentTextCodec.DetectPreamble(bytes,
            bomLength == 0 ? new UTF8Encoding(false, true) : Encoding.ASCII, out var skipped);
        Assert.Equal(EncodingFor(name).CodePage, selected.CodePage);
        Assert.Equal(bomLength, skipped);
        using var input = new ShortReadStream(bytes, 1);
        using var output = new MemoryStream();
        var result = await DocumentTextCodec.DecodeToCanonicalUtf8Async(input, output, selected, skipped,
            cancellationToken: TestContext.Current.CancellationToken);
        Assert.Equal(new byte[] { 0x41, 0x0D, 0xC3, 0xA9, 0x00 }, output.ToArray());
        Assert.Equal(ending, result.LineEnding);
        Assert.Equal(bytes.Length, result.InputBytes);
        Assert.Equal(5, result.CanonicalBytes);
    }

    [Theory]
    [MemberData(nameof(FixedByteOracles))]
    public async Task FixedSaveBytesIncludeExactlyTheRequestedBomAndLineEnding(string name, string hex, int bomLength, LineEnding ending)
    {
        byte[] canonical = [0x41, 0x0D, 0xC3, 0xA9, 0x00];
        using var input = new ShortReadStream(canonical, 1);
        using var baseline = new MemoryStream();
        using var output = new MemoryStream();
        await DocumentTextCodec.CopyFromNativeAsync(input, baseline, output, EncodingFor(name), ending,
            cancellationToken: TestContext.Current.CancellationToken);
        Assert.Equal(Convert.FromHexString(hex), output.ToArray());
        Assert.Equal(canonical, baseline.ToArray());
        Assert.Equal(bomLength, EncodingFor(name).GetPreamble().Length);
    }

    [Theory]
    [InlineData("EF", 1)]
    [InlineData("EFBB", 2)]
    [InlineData("FF", 1)]
    [InlineData("00", 1)]
    [InlineData("0000FE", 3)]
    public void IncompletePreambleCannotConsumeContent(string hex, int available)
    {
        var bytes = Convert.FromHexString(hex);
        var fallback = Encoding.ASCII;
        var selected = DocumentTextCodec.DetectPreamble(bytes.AsSpan(0, available), fallback, out var skipped);
        Assert.Same(fallback, selected);
        Assert.Equal(0, skipped);
    }

    private static Encoding EncodingFor(string name) => name switch
    {
        "utf8" => new UTF8Encoding(false, true),
        "utf8-bom" => new UTF8Encoding(true, true),
        "utf16-le" => new UnicodeEncoding(false, false, true),
        "utf16-le-bom" => new UnicodeEncoding(false, true, true),
        "utf16-be" => new UnicodeEncoding(true, false, true),
        "utf16-be-bom" => new UnicodeEncoding(true, true, true),
        "utf32-le" => new UTF32Encoding(false, false, true),
        "utf32-le-bom" => new UTF32Encoding(false, true, true),
        "utf32-be" => new UTF32Encoding(true, false, true),
        "utf32-be-bom" => new UTF32Encoding(true, true, true),
        _ => Encoding.GetEncoding(int.Parse(name, System.Globalization.CultureInfo.InvariantCulture))
    };

    [Fact]
    public void PreambleDetectionUsesOnlyTheSuppliedSlice()
    {
        byte[] bytes = [0xcc, 0xff, 0xfe, 0x00, 0x00, 0xcc];
        var encoding = DocumentTextCodec.DetectPreamble(bytes.AsSpan(1, 2), Encoding.ASCII, out var length);
        Assert.Equal(1200, encoding.CodePage);
        Assert.Equal(2, length);
        Assert.Same(Encoding.ASCII, DocumentTextCodec.DetectPreamble(bytes.AsSpan(1, 1), Encoding.ASCII, out length));
        Assert.Equal(0, length);
    }

    [Fact]
    public async Task ConcurrentConversionsRetainBuffersUntilDelayedWritesFinish()
    {
        var firstWrites = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var arrived = 0;
        async Task WaitForAllWritersAsync()
        {
            if (Interlocked.Increment(ref arrived) == 8) firstWrites.SetResult();
            await firstWrites.Task.WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);
        }
        await Task.WhenAll(Enumerable.Range(0, 8).Select(async index =>
        {
            var text = new string((char)('a' + index), DocumentTextCodec.BufferSize + index) + "😀中\r\n\0tail";
            var sourceEncoding = new UTF32Encoding(index % 2 == 0, false, true);
            using var input = new ShortReadStream(sourceEncoding.GetBytes(text), 8191);
            using var canonical = new DelayedWriteStream(WaitForAllWritersAsync);
            await DocumentTextCodec.DecodeToCanonicalUtf8Async(input, canonical, sourceEncoding,
                cancellationToken: TestContext.Current.CancellationToken);
            var expected = Encoding.UTF8.GetBytes(text.Replace("\r\n", "\r", StringComparison.Ordinal));
            Assert.Equal(expected, canonical.ToArray());

            canonical.Position = 0;
            using var baseline = new DelayedWriteStream();
            using var encoded = new DelayedWriteStream();
            var destinationEncoding = new UnicodeEncoding(false, true, true);
            await DocumentTextCodec.CopyFromNativeAsync(canonical, baseline, encoded, destinationEncoding, LineEnding.Crlf,
                cancellationToken: TestContext.Current.CancellationToken);
            Assert.Equal(expected, baseline.ToArray());
            Assert.Equal(destinationEncoding.GetPreamble().Concat(destinationEncoding.GetBytes(text)).ToArray(), encoded.ToArray());
        }));
    }

    private sealed class DelayedWriteStream : MemoryStream
    {
        private Func<Task> _beforeFirstWrite;
        public DelayedWriteStream(Func<Task> beforeFirstWrite = null) => _beforeFirstWrite = beforeFirstWrite;

        public override async ValueTask WriteAsync(ReadOnlyMemory<byte> buffer, CancellationToken cancellationToken = default)
        {
            if (_beforeFirstWrite != null)
            {
                await _beforeFirstWrite();
                _beforeFirstWrite = null;
            }
            // Consume the borrowed memory only after other conversions can rent and use their buffers.
            await Task.Yield();
            cancellationToken.ThrowIfCancellationRequested();
            base.Write(buffer.Span);
        }
    }

    private static string Sample(string name) => name switch
    {
        "empty" => "",
        "leading-bom-content" => "\ufeffleading content\0tail",
        "unicode" => "中文 العربية 日本語 😀\r\n\0after\nlast\r",
        "mixed" => "a\rb\nc\r\nd\n\re\r",
        "blank-lines" => "\r\n\r\n",
        "buffer-boundary" => new string('a', DocumentTextCodec.BufferSize - 1) + "\r\n😀中文\0尾\r",
        _ => throw new ArgumentOutOfRangeException(nameof(name))
    };

    [Fact]
    public async Task WideEncodingAdmission()
    {
        // Admission follows native UTF-8 bytes, not the encoded file size.
        // UTF-32 can use more source/output bytes for the same small document.
        var encoding = new UTF32Encoding(false, false, true);
        var canonical = new UTF8Encoding(false, true).GetBytes("ab\0\r");
        var encoded = encoding.GetBytes("ab\0\r");
        using (var input = new ShortReadStream(encoded, 1))
        using (var output = new MemoryStream())
        {
            var result = await DocumentTextCodec.DecodeToCanonicalUtf8Async(input, output, encoding,
                maximumCanonicalBytes: canonical.Length, cancellationToken: TestContext.Current.CancellationToken);
            EqualBytes(canonical, output.ToArray(), "wide-encoding canonical admission");
            if (result.InputBytes <= result.CanonicalBytes)
                throw new Exception("The wide-encoding admission fixture did not exercise distinct byte counts.");
        }
        using (var input = new ShortReadStream(canonical, 1))
        using (var baseline = new MemoryStream())
        using (var output = new MemoryStream())
        {
            await DocumentTextCodec.CopyFromNativeAsync(input, baseline, output, encoding, LineEnding.Cr,
                cancellationToken: TestContext.Current.CancellationToken);
            EqualBytes(encoded, output.ToArray(), "wide-encoding saved output admission");
        }
    }

    [Fact]
    public async Task LateInvalidFallback()
    {
        var original = Enumerable.Repeat((byte)'a', DocumentTextCodec.BufferSize).Concat(new byte[] { 0xc3, 0xa9, 0xff }).ToArray();
        var unchanged = original.ToArray();
        var candidateStreams = new List<TrackingStream>();
        var attempts = 0;
        var fallback = Encoding.GetEncoding(1252);
        var actual = await DocumentTextCodec.RetryDecodeAsync(async encoding =>
        {
            attempts++;
            using var source = new ShortReadStream(original, 17);
            using var candidate = new TrackingStream();
            candidateStreams.Add(candidate);
            await DocumentTextCodec.DecodeToCanonicalUtf8Async(source, candidate, encoding,
                cancellationToken: TestContext.Current.CancellationToken);
            return candidate.ToArray();
        }, new UTF8Encoding(false, true), fallback);
        EqualBytes(new UTF8Encoding(false, true).GetBytes(fallback.GetString(original)), actual, "restart fallback from byte zero");
        EqualBytes(unchanged, original, "source not modified");
        if (attempts != 2 || candidateStreams.Count != 2 || candidateStreams.Any(candidate => !candidate.Disposed))
            throw new Exception("A late invalid decode did not discard its entire first candidate.");
        if (candidateStreams[0].BytesBeforeDispose == 0)
            throw new Exception("Late failure fixture did not exercise a partially written candidate.");

    }

    [Fact]
    public async Task CancellationStopsDecodeAndRetainsCallerStreams()
    {
        var sourceBytes = new UTF8Encoding(false, true).GetBytes(new string('a', DocumentTextCodec.BufferSize + 11));
        using var cancellation = CancellationTokenSource.CreateLinkedTokenSource(TestContext.Current.CancellationToken);
        using var source = new ShortReadStream(sourceBytes, 9);
        using var output = new MemoryStream();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
            DocumentTextCodec.DecodeToCanonicalUtf8Async(source, output, new UTF8Encoding(false, true),
                progress: new InlineProgress(_ => cancellation.Cancel()), cancellationToken: cancellation.Token));
        Assert.True(source.CanRead && output.CanWrite);
        Assert.True(output.Length < sourceBytes.Length, "Canceled decode must stop before completing the document.");
    }

    [Fact]
    public async Task CanonicalLimitIsMeasuredInBytesAndEnforcedBeforeWritingBeyondTheLimit()
    {
        using var source = new ShortReadStream(new UTF8Encoding(false, true).GetBytes("汉汉汉"), 3);
        using var output = new MemoryStream();
        await Assert.ThrowsAsync<NotSupportedException>(() =>
            DocumentTextCodec.DecodeToCanonicalUtf8Async(source, output, new UTF8Encoding(false, true),
                maximumCanonicalBytes: 8, cancellationToken: TestContext.Current.CancellationToken));
        Assert.True(output.Length <= 8);
    }

    [Fact]
    public async Task PreCanceledSaveWritesNeitherContentNorBom()
    {
        using var cancellation = CancellationTokenSource.CreateLinkedTokenSource(TestContext.Current.CancellationToken);
        using var source = new ShortReadStream([0x41], 1);
        using var canonical = new MemoryStream();
        using var encoded = new MemoryStream();
        cancellation.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
            DocumentTextCodec.CopyFromNativeAsync(source, canonical, encoded, new UTF8Encoding(true), LineEnding.Crlf,
                cancellationToken: cancellation.Token));
        Assert.Equal(0, canonical.Length);
        Assert.Equal(0, encoded.Length);
    }

    [Theory]
    [InlineData("FF", typeof(DecoderFallbackException))]
    [InlineData("6E6F740A63616E6F6E6963616C", typeof(InvalidDataException))]
    public async Task InvalidNativeCanonicalInputIsRejected(string hex, Type expectedException)
    {
        using var source = new ShortReadStream(Convert.FromHexString(hex), 1);
        using var canonical = new MemoryStream();
        using var encoded = new MemoryStream();
        await Assert.ThrowsAsync(expectedException, () =>
            DocumentTextCodec.CopyFromNativeAsync(source, canonical, encoded, new UTF8Encoding(false), LineEnding.Crlf,
                cancellationToken: TestContext.Current.CancellationToken));
    }

    [Theory]
    [InlineData("", "", LineEnding.Crlf)]
    [InlineData("610D", "610D", LineEnding.Cr)]
    [InlineData("610A620D", "610D620D", LineEnding.Cr)]
    [InlineData("610D620A630D0A", "610D620D630D", LineEnding.Crlf)]
    [InlineData("0A0D00", "0D0D00", LineEnding.Cr)]
    public async Task FixedMixedEndingBytesKeepTheSpecifiedPriorityAcrossSingleByteReads(string hex, string canonicalHex, LineEnding expected)
    {
        using var source = new ShortReadStream(Convert.FromHexString(hex), 1);
        using var output = new MemoryStream();
        var result = await DocumentTextCodec.DecodeToCanonicalUtf8Async(source, output, new UTF8Encoding(false, true),
            cancellationToken: TestContext.Current.CancellationToken);
        Assert.Equal(Convert.FromHexString(canonicalHex), output.ToArray());
        Assert.Equal(expected, result.LineEnding);
    }

    private static string Canonicalize(string text) => text.Replace("\r\n", "\r").Replace('\n', '\r');

    private static void EqualBytes(byte[] expected, byte[] actual, string label)
    {
        if (expected.SequenceEqual(actual)) return;
        var offset = 0;
        while (offset < Math.Min(expected.Length, actual.Length) && expected[offset] == actual[offset]) offset++;
        var expectedByte = offset < expected.Length ? $"0x{expected[offset]:X2}" : "<end>";
        var actualByte = offset < actual.Length ? $"0x{actual[offset]:X2}" : "<end>";
        throw new Exception($"{label}: first difference at byte {offset}.\n" +
            $"  Expected {expected.Length} bytes, {expectedByte} at this offset.\n" +
            $"  Actual {actual.Length} bytes, {actualByte} at this offset.");
    }

    private sealed class ShortReadStream : MemoryStream
    {
        private readonly int _maximumRead;
        private int _nonEmptyReads;
        private int _largestRead;
        public ShortReadStream(byte[] bytes, int maximumRead) : base(bytes, writable: false) { _maximumRead = maximumRead; }
        public override Task<int> ReadAsync(byte[] buffer, int offset, int count, CancellationToken cancellationToken)
            => ReadAsync(buffer.AsMemory(offset, count), cancellationToken).AsTask();

        public override async ValueTask<int> ReadAsync(Memory<byte> buffer, CancellationToken cancellationToken = default)
        {
            var count = await base.ReadAsync(buffer[..Math.Min(buffer.Length, _maximumRead)], cancellationToken);
            if (count > 0) _nonEmptyReads++;
            _largestRead = Math.Max(_largestRead, count);
            return count;
        }

        public void AssertReadBoundaries()
        {
            Assert.InRange(_largestRead, 0, _maximumRead);
            Assert.True(_nonEmptyReads >= (Length + _maximumRead - 1) / _maximumRead,
                "The codec must exercise the configured short reads, not bypass the stream's async overrides.");
        }
    }

    private sealed class TrackingStream : MemoryStream
    {
        public bool Disposed { get; private set; }
        public long BytesBeforeDispose { get; private set; }
        protected override void Dispose(bool disposing)
        {
            if (!Disposed) BytesBeforeDispose = Length;
            Disposed = true;
            base.Dispose(disposing);
        }
    }

    private sealed class InlineProgress : IProgress<DocumentTextProgress>
    {
        private readonly Action<DocumentTextProgress> _report;
        public InlineProgress(Action<DocumentTextProgress> report) { _report = report; }
        public void Report(DocumentTextProgress progress) => _report(progress);
    }
}
