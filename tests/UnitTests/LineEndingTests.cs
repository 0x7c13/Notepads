// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Text;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Text;

namespace Notepads.UnitTests;

public sealed class LineEndingTests
{
    public static TheoryData<string, string> Samples => new()
    {
        { "empty", "" },
        { "unicode-and-NUL", "no newline\0中文😀" },
        { "empty-lines", "\r\n\r\n" },
        { "mixed", "a\rb\nc\r\nd\n\re\r" },
        { "writer-boundary", new string('a', 16382) + "😀\r\n" + new string('b', 16382) + "\r\n尾\0\n" }
    };

    [Theory]
    [MemberData(nameof(Samples))]
    public async Task NormalizeAndStreamPreserveContent(string scenario, string text)
    {
        foreach (var ending in new[] { LineEnding.Cr, LineEnding.Lf, LineEnding.Crlf })
        {
            var sequence = ending == LineEnding.Cr ? "\r" : ending == LineEnding.Lf ? "\n" : "\r\n";
            var expected = text.Replace("\r\n", "\n").Replace("\r", "\n").Replace("\n", sequence);
            Assert.True(expected == LineEndingUtility.ApplyLineEnding(text, ending), $"{scenario}: {ending} normalization");
            using var stream = new MemoryStream();
            using (var writer = new StreamWriter(stream, new UTF8Encoding(false, true), 1024, true))
                await LineEndingUtility.WriteAsync(writer, text, ending);
            Assert.Equal(expected, new UTF8Encoding(false, true).GetString(stream.ToArray()));
            if (expected == text) Assert.Same(text, LineEndingUtility.ApplyLineEnding(text, ending));
        }
    }

    [Fact]
    public async Task UnspecifiedEndingPreservesRawText()
    {
        using var writer = new StringWriter();
        const string raw = "\r\n\0😀\n\r";
        await LineEndingUtility.WriteAsync(writer, raw);
        Assert.Equal(raw, writer.ToString());
    }

    [Theory]
    [InlineData("", LineEnding.Crlf)]
    [InlineData("text\0😀", LineEnding.Crlf)]
    [InlineData("a\nb\n", LineEnding.Lf)]
    [InlineData("a\nb\rc", LineEnding.Cr)]
    [InlineData("a\rb\r\nc\n", LineEnding.Crlf)]
    public void MixedEndingDetectionHasDefinedPriority(string text, LineEnding expected) =>
        Assert.Equal(expected, LineEndingUtility.GetLineEndingTypeFromText(text));
}
