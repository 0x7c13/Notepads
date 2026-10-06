// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Text;

namespace NotepadsEditorTests;

internal static class DocumentTextCodecTests
{
    public static async Task RunAsync()
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        var encodings = new Encoding[]
        {
            new UTF8Encoding(false, true), new UTF8Encoding(true, true),
            new UnicodeEncoding(false, false, true), new UnicodeEncoding(false, true, true),
            new UnicodeEncoding(true, false, true), new UnicodeEncoding(true, true, true),
            new UTF32Encoding(false, false, true), new UTF32Encoding(false, true, true),
            new UTF32Encoding(true, false, true), new UTF32Encoding(true, true, true),
            Encoding.GetEncoding(1252), Encoding.GetEncoding(1256), Encoding.GetEncoding(932),
            Encoding.GetEncoding(50220)
        };
        var texts = new[]
        {
            string.Empty, "\ufeffleading content\0tail", "中文 العربية 日本語 😀\r\n\0after\nlast\r",
            "a\rb\nc\r\nd\n\re\r", "\r\n\r\n",
            new string('a', DocumentTextCodec.BufferSize - 1) + "\r\n😀中文\0尾\r"
        };

        foreach (var encoding in encodings)
        {
            foreach (var text in texts)
            {
                var encoded = encoding.GetPreamble().Concat(encoding.GetBytes(text)).ToArray();
                foreach (var readSize in new[] { 1, 7, DocumentTextCodec.BufferSize })
                {
                    using (var input = new ShortReadStream(encoded, readSize))
                    using (var output = new MemoryStream())
                    {
                        var selected = DocumentTextCodec.DetectPreamble(encoded, encoded.Length, encoding, out var preambleLength);
                        // File loading follows StreamReader BOM detection: an initial
                        // signature is a preamble even when the selected encoding emits none.
                        var actualSource = selected.GetString(encoded, preambleLength, encoded.Length - preambleLength);
                        var result = await DocumentTextCodec.DecodeToCanonicalUtf8Async(input, output, selected, preambleLength);
                        Equal(Canonicalize(actualSource), new UTF8Encoding(false, true).GetString(output.ToArray()), "decode/chunk boundary");
                        if (result.InputBytes != encoded.Length || result.CanonicalBytes != output.Length ||
                            result.LineEnding != LineEndingUtility.GetLineEndingTypeFromText(actualSource))
                        {
                            throw new Exception("Streaming decode byte counts or mixed-ending priority changed.");
                        }

                        if (!input.CanRead || !output.CanWrite) throw new Exception("Codec disposed caller-owned streams.");
                    }
                }
            }
        }

        var canonicalText = "\ufeffcontent\0中文 العربية 日本語 😀\rnext\r";
        foreach (var encoding in encodings)
        {
            foreach (var ending in new[] { LineEnding.Cr, LineEnding.Lf, LineEnding.Crlf })
            {
                foreach (var readSize in new[] { 1, 5, DocumentTextCodec.BufferSize })
                {
                    var canonical = new UTF8Encoding(false, true).GetBytes(canonicalText);
                    using (var input = new ShortReadStream(canonical, readSize))
                    using (var baseline = new MemoryStream())
                    using (var destination = new MemoryStream())
                    {
                        var copied = await DocumentTextCodec.CopyFromNativeAsync(input, baseline, destination, encoding, ending);
                        EqualBytes(canonical, baseline.ToArray(), "unchanged canonical baseline");
                        EqualBytes(encoding.GetPreamble().Concat(encoding.GetBytes(LineEndingUtility.ApplyLineEnding(canonicalText, ending))).ToArray(),
                            destination.ToArray(), "requested encoding/EOL output");
                        if (copied != canonical.Length || !input.CanRead || !baseline.CanWrite || !destination.CanWrite)
                            throw new Exception("Save byte count or stream ownership changed.");
                    }
                }
            }
        }

        await VerifyLateInvalidFallbackAsync();
        await VerifyCancellationAndLimitsAsync();
        await VerifyWideEncodingAdmissionAsync();
        await VerifyInvalidCanonicalAsync();
        Console.WriteLine("PASS: V2 bounded document codecs, BOM overrides, stateful encoding/chunk boundaries, Unicode/NUL, EOL priority, whole-candidate fallback, cancellation, and admission limit.");
    }

    private static async Task VerifyWideEncodingAdmissionAsync()
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
                maximumCanonicalBytes: canonical.Length);
            EqualBytes(canonical, output.ToArray(), "wide-encoding canonical admission");
            if (result.InputBytes <= result.CanonicalBytes)
                throw new Exception("The wide-encoding admission fixture did not exercise distinct byte counts.");
        }
        using (var input = new ShortReadStream(canonical, 1))
        using (var baseline = new MemoryStream())
        using (var output = new MemoryStream())
        {
            await DocumentTextCodec.CopyFromNativeAsync(input, baseline, output, encoding, LineEnding.Cr);
            EqualBytes(encoded, output.ToArray(), "wide-encoding saved output admission");
        }
    }

    private static async Task VerifyLateInvalidFallbackAsync()
    {
        var original = Enumerable.Repeat((byte)'a', DocumentTextCodec.BufferSize).Concat(new byte[] { 0xc3, 0xa9, 0xff }).ToArray();
        var unchanged = original.ToArray();
        var candidateStreams = new List<TrackingStream>();
        var attempts = 0;
        var fallback = Encoding.GetEncoding(1252);
        var actual = await DocumentTextCodec.RetryDecodeAsync(async encoding =>
        {
            attempts++;
            using (var source = new ShortReadStream(original, 17))
            using (var candidate = new TrackingStream())
            {
                candidateStreams.Add(candidate);
                await DocumentTextCodec.DecodeToCanonicalUtf8Async(source, candidate, encoding);
                return candidate.ToArray();
            }
        }, new UTF8Encoding(false, true), fallback);
        EqualBytes(new UTF8Encoding(false, true).GetBytes(fallback.GetString(original)), actual, "restart fallback from byte zero");
        EqualBytes(unchanged, original, "source not modified");
        if (attempts != 2 || candidateStreams.Count != 2 || candidateStreams.Any(candidate => !candidate.Disposed))
            throw new Exception("A late invalid decode did not discard its entire first candidate.");
        if (candidateStreams[0].BytesBeforeDispose == 0)
            throw new Exception("Late failure fixture did not exercise a partially written candidate.");

        // UTF-32's little-endian BOM must win over the shorter UTF-16 prefix,
        // and a BOM overrides an explicit conflicting requested encoding.
        var utf32 = new UTF32Encoding(false, true);
        var sourceBytes = utf32.GetPreamble().Concat(utf32.GetBytes("😀\0tail")).ToArray();
        var selection = DocumentTextCodec.DetectPreamble(sourceBytes, sourceBytes.Length, Encoding.ASCII, out var skipped);
        if (selection.CodePage != utf32.CodePage || skipped != 4)
            throw new Exception("UTF-32 BOM precedence changed.");
    }

    private static async Task VerifyCancellationAndLimitsAsync()
    {
        var sourceBytes = new UTF8Encoding(false, true).GetBytes(new string('a', DocumentTextCodec.BufferSize + 11));
        using (var cancellation = new CancellationTokenSource())
        using (var source = new ShortReadStream(sourceBytes, 9))
        using (var output = new MemoryStream())
        {
            try
            {
                await DocumentTextCodec.DecodeToCanonicalUtf8Async(source, output, new UTF8Encoding(false, true),
                    progress: new InlineProgress(_ => cancellation.Cancel()), cancellationToken: cancellation.Token);
                throw new Exception("Canceled decode succeeded.");
            }
            catch (OperationCanceledException) { }
            if (!source.CanRead || !output.CanWrite || output.Length >= sourceBytes.Length)
                throw new Exception("Cancellation changed stream ownership or finished the input.");
        }

        using (var source = new ShortReadStream(new UTF8Encoding(false, true).GetBytes("汉汉汉"), 3))
        using (var output = new MemoryStream())
        {
            try
            {
                await DocumentTextCodec.DecodeToCanonicalUtf8Async(source, output, new UTF8Encoding(false, true), maximumCanonicalBytes: 8);
                throw new Exception("Canonical byte limit was not enforced.");
            }
            catch (NotSupportedException) { }
            if (output.Length > 8) throw new Exception("Canonical output exceeded its admission limit before rejection.");
        }

        using (var cancellation = new CancellationTokenSource())
        using (var source = new ShortReadStream(sourceBytes, 9))
        using (var canonical = new MemoryStream())
        using (var encoded = new MemoryStream())
        {
            cancellation.Cancel();
            try
            {
                await DocumentTextCodec.CopyFromNativeAsync(source, canonical, encoded, new UTF8Encoding(true), LineEnding.Crlf,
                    cancellationToken: cancellation.Token);
                throw new Exception("Canceled save succeeded.");
            }
            catch (OperationCanceledException) { }
            if (canonical.Length != 0 || encoded.Length != 0) throw new Exception("Pre-canceled save wrote even its BOM.");
        }
    }

    private static async Task VerifyInvalidCanonicalAsync()
    {
        foreach (var bytes in new[] { [0xff], Encoding.UTF8.GetBytes("not\ncanonical") })
        {
            using (var source = new ShortReadStream(bytes, 1))
            using (var canonical = new MemoryStream())
            using (var encoded = new MemoryStream())
            {
                try
                {
                    await DocumentTextCodec.CopyFromNativeAsync(source, canonical, encoded, new UTF8Encoding(false), LineEnding.Crlf);
                    throw new Exception("Invalid native canonical input was accepted.");
                }
                catch (DecoderFallbackException) { }
                catch (InvalidDataException) { }
            }
        }
    }

    private static string Canonicalize(string text) => text.Replace("\r\n", "\r").Replace('\n', '\r');

    private static void Equal(string expected, string actual, string label)
    {
        if (!string.Equals(expected, actual, StringComparison.Ordinal))
            throw new Exception(label + ": expected " + expected.Length + " UTF-16 units, got " + actual.Length + ".");
    }

    private static void EqualBytes(byte[] expected, byte[] actual, string label)
    {
        if (!expected.SequenceEqual(actual))
            throw new Exception(label + ": expected " + expected.Length + " bytes, got " + actual.Length + ".");
    }

    private sealed class ShortReadStream : MemoryStream
    {
        private readonly int _maximumRead;
        public ShortReadStream(byte[] bytes, int maximumRead) : base(bytes, writable: false) { _maximumRead = maximumRead; }
        public override Task<int> ReadAsync(byte[] buffer, int offset, int count, CancellationToken cancellationToken)
            => base.ReadAsync(buffer, offset, Math.Min(count, _maximumRead), cancellationToken);
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
