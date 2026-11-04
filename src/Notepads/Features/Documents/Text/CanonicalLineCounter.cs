// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Buffers;
using System.IO;
using System.Threading;
using System.Threading.Tasks;

namespace Notepads.Features.Documents.Text;

internal static class CanonicalLineCounter
{
    // Mirrors the native MaximumDiffLines (NativeDiff.h); diff admission rejects any larger count.
    private const long MaximumDiffLines = 1_000_000;

    // Counted streaming admission; no strings or per-line allocations.
    internal static async Task<long> CountAsync(Stream stream, CancellationToken cancellationToken)
    {
        using var lease = MemoryPool<byte>.Shared.Rent(65536);
        var buffer = lease.Memory[..65536];
        long lines = 1;
        while (true)
        {
            var count = await stream.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (count == 0) return lines;
            lines += buffer.Span[..count].Count((byte)'\r');
            if (lines > MaximumDiffLines) return lines;
        }
    }
}
