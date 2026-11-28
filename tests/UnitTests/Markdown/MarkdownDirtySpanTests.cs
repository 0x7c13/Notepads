// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Text;
using Notepads.Presentation.Controls.Markdown;

namespace Notepads.UnitTests;

public sealed class MarkdownDirtySpanTests
{
    private static readonly UTF8Encoding StrictUtf8 = new(false, true);
    private static readonly string[] Scalars = ["a", "Ω", "中", "😀", "e", "\u0301", "\r", "\n", "\r\n", "\0", "\t"];

    [Fact]
    public void ClearRestoresTheEmptyStateAndTheNextEditHasNoPreviousCoordinates()
    {
        var span = new MarkdownDirtySpan();
        Assert.False(span.HasChanges);
        Assert.Equal((0L, 0L, 0L), Coordinates(span));
        span.Add(4, 7, 2);
        span.Clear();
        Assert.False(span.HasChanges);
        Assert.Equal((0L, 0L, 0L), Coordinates(span));
        span.Add(1, 0, 4);
        Assert.True(span.HasChanges);
        Assert.Equal((1L, 0L, 4L), Coordinates(span));
    }

    [Fact]
    public void ADeletionAcrossThePreviousSpanAndRemoteEditsReconstructTheOriginal()
    {
        var original = StrictUtf8.GetBytes("0123456789");
        var current = original;
        var span = new MarkdownDirtySpan();
        Apply(original, ref current, span, 3, 2, "XYZ");
        Apply(original, ref current, span, 1, 7, "Ω");
        Assert.Equal("0Ω789", StrictUtf8.GetString(current));
        Apply(original, ref current, span, current.Length, 0, "😀\r\n\0");
        Apply(original, ref current, span, 0, 1, "e\u0301");
        Apply(original, ref current, span, 0, current.Length, "");
        Assert.Empty(current);
    }

    [Fact]
    public void EveryPairOfSmallAsciiSplicesHasAnIndependentByteOracle()
    {
        var original = StrictUtf8.GetBytes("abcd");
        foreach (var firstStart in Enumerable.Range(0, original.Length + 1))
        foreach (var firstEnd in Enumerable.Range(firstStart, original.Length - firstStart + 1))
        foreach (var firstText in new[] { "", "X", "YZ" })
        {
            var intermediate = Splice(original, firstStart, firstEnd - firstStart, StrictUtf8.GetBytes(firstText));
            foreach (var secondStart in Enumerable.Range(0, intermediate.Length + 1))
            foreach (var secondEnd in Enumerable.Range(secondStart, intermediate.Length - secondStart + 1))
            foreach (var secondText in new[] { "", "Q", "RS" })
            {
                var current = original;
                var span = new MarkdownDirtySpan();
                Apply(original, ref current, span, firstStart, firstEnd - firstStart, firstText);
                Apply(original, ref current, span, secondStart, secondEnd - secondStart, secondText);
            }
        }
    }

    [Theory]
    [InlineData(7)]
    [InlineData(39)]
    [InlineData(20261104)]
    [InlineData(2147483647)]
    public void SequentialUnicodeEditsMatchAnIndependentUtf8SpliceOracle(int seed)
    {
        var random = new Random(seed);
        for (var sequence = 0; sequence < 32; sequence++)
        {
            var original = StrictUtf8.GetBytes("CR\rLF\nCRLF\r\n中Ω😀e\u0301\0" + RandomText(random, 12));
            var current = original;
            var span = new MarkdownDirtySpan();
            for (var operation = 0; operation < 100; operation++)
            {
                var boundaries = Utf8Boundaries(current);
                var first = random.Next(boundaries.Length);
                var last = random.Next(first, boundaries.Length);
                Apply(original, ref current, span, boundaries[first], boundaries[last] - boundaries[first],
                    RandomText(random, random.Next(0, 9)));
            }
            span.Clear();
            Assert.False(span.HasChanges);
        }
    }

    [Theory]
    [InlineData(-1, 0, 0)]
    [InlineData(0, -1, 0)]
    [InlineData(0, 0, -1)]
    public void NegativeInputsAreRejectedWithoutChangingAnAcceptedSpan(long position, long removed, long inserted)
    {
        var span = new MarkdownDirtySpan();
        span.Add(4, 2, 3);
        var before = Coordinates(span);
        Assert.Throws<ArgumentOutOfRangeException>(() => span.Add(position, removed, inserted));
        Assert.True(span.HasChanges);
        Assert.Equal(before, Coordinates(span));
    }

    [Fact]
    public void OverflowIsRejectedBeforePublishingPartialCoordinates()
    {
        var empty = new MarkdownDirtySpan();
        Assert.Throws<OverflowException>(() => empty.Add(long.MaxValue, 1, 0));
        Assert.False(empty.HasChanges);
        Assert.Equal((0L, 0L, 0L), Coordinates(empty));
        Assert.Throws<OverflowException>(() => empty.Add(long.MaxValue, 0, 1));
        Assert.False(empty.HasChanges);

        var span = new MarkdownDirtySpan();
        span.Add(0, long.MaxValue, 0);
        var before = Coordinates(span);
        Assert.Throws<OverflowException>(() => span.Add(1, 0, 0));
        Assert.Equal(before, Coordinates(span));
        Assert.Throws<OverflowException>(() => span.Add(long.MaxValue, 0, 1));
        Assert.Equal(before, Coordinates(span));
    }

    private static (long Start, long DeleteLength, long ReplacementLength) Coordinates(MarkdownDirtySpan span) =>
        (span.Start, span.DeleteLength, span.ReplacementLength);

    private static void Apply(byte[] original, ref byte[] current, MarkdownDirtySpan span, int start, int removed, string inserted)
    {
        var replacement = StrictUtf8.GetBytes(inserted);
        var expected = Splice(current, start, removed, replacement);
        span.Add(start, removed, replacement.Length);
        Assert.True(span.HasChanges);
        Assert.InRange(span.Start, 0L, original.Length);
        Assert.InRange(span.DeleteLength, 0L, original.Length - span.Start);
        Assert.InRange(span.ReplacementLength, 0L, expected.Length - span.Start);
        var reconstructed = original.AsSpan(0, checked((int)span.Start)).ToArray()
            .Concat(expected.AsSpan(checked((int)span.Start), checked((int)span.ReplacementLength)).ToArray())
            .Concat(original.AsSpan(checked((int)(span.Start + span.DeleteLength))).ToArray()).ToArray();
        Assert.Equal(expected, reconstructed);
        Assert.Equal(StrictUtf8.GetString(expected), StrictUtf8.GetString(reconstructed));
        current = expected;
    }

    private static byte[] Splice(byte[] value, int start, int removed, byte[] inserted) =>
        value.AsSpan(0, start).ToArray().Concat(inserted).Concat(value.AsSpan(start + removed).ToArray()).ToArray();

    private static int[] Utf8Boundaries(byte[] value) => Enumerable.Range(0, value.Length)
        .Where(index => (value[index] & 0xc0) != 0x80).Append(value.Length).ToArray();

    private static string RandomText(Random random, int count) =>
        string.Concat(Enumerable.Range(0, count).Select(_ => Scalars[random.Next(Scalars.Length)]));
}
