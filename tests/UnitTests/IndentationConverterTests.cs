// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Text;

namespace Notepads.UnitTests;

public sealed class IndentationConverterTests
{
    [Theory]
    [InlineData("\t x\r \tx\n\t\t中😀\0\t  \r\n", "     x\r    x\n        中😀\0\t  \r\n", false, true)]
    [InlineData("     x\r \t x\n        中😀\0\t  \r\n", "\t x\r\t x\n\t\t中😀\0\t  \r\n", true, true)]
    [InlineData("\t  x\r\t", "\t  x\r\t", true, false)]
    [InlineData("  x\r  ", "  x\r  ", false, false)]
    [InlineData("\t", "    ", false, true)]
    [InlineData("", "", true, false)]
    public async Task ConversionPreservesBodyAndAllChunkBoundaries(string input, string expected, bool tabs, bool changed)
    {
        foreach (var chunk in new[] { 1, 2, 3, 7, 65536 })
        {
            using var source = new ChunkStream(Encoding.UTF8.GetBytes(input), chunk);
            using var result = new MemoryStream();
            Assert.Equal(changed, await IndentationConverter.CopyAsync(source, result, tabs, 4));
            Assert.Equal(Encoding.UTF8.GetBytes(expected), result.ToArray());
        }
    }

    [Fact]
    public async Task VeryLongPrefixesAndBodiesHaveNoLineLengthLimit()
    {
        var input = new string(' ', 65535) + "\t中文\0" + new string('x', 131073) + "\r \t";
        using var source = new MemoryStream(Encoding.UTF8.GetBytes(input));
        using var output = new MemoryStream();
        Assert.True(await IndentationConverter.CopyAsync(source, output, false, 4));
        Assert.Equal(new string(' ', 65536) + "中文\0" + new string('x', 131073) + "\r    ", Encoding.UTF8.GetString(output.ToArray()));
    }

    [Fact]
    public async Task CancellationAndInvalidWidthAreRejected()
    {
        using var source = new MemoryStream([9, 32, 120]);
        using var output = new MemoryStream();
        await Assert.ThrowsAsync<ArgumentOutOfRangeException>(() => IndentationConverter.CopyAsync(source, output, false, 0));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => IndentationConverter.CopyAsync(source, output, false, 4, new CancellationToken(true)));
        Assert.Empty(output.ToArray());
    }

    private sealed class ChunkStream(byte[] data, int chunk) : MemoryStream(data)
    {
        public override ValueTask<int> ReadAsync(Memory<byte> buffer, CancellationToken cancellationToken = default) =>
            base.ReadAsync(buffer[..Math.Min(chunk, buffer.Length)], cancellationToken);
    }
}
