// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;

namespace Notepads.Presentation.Controls.Markdown;

internal static class MarkdownDataImage
{
    internal const ulong MaximumEncodedBytes = 16 * 1024 * 1024;
    private const string PngPrefix = "data:image/png;base64,";
    private const string JpegPrefix = "data:image/jpeg;base64,";

    // Only base64 PNG/JPEG are supported. WIC remains responsible for validating
    // the image structure, dimensions and pixel budget after this host handoff.
    internal static byte[] Decode(string destination, ulong maximumBytes, CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        bool png = destination?.StartsWith(PngPrefix, StringComparison.OrdinalIgnoreCase) == true;
        bool jpeg = destination?.StartsWith(JpegPrefix, StringComparison.OrdinalIgnoreCase) == true;
        if (!png && !jpeg) return null;

        ReadOnlySpan<byte> signature = png ? [137, 80, 78, 71, 13, 10, 26, 10] : [255, 216, 255];
        ReadOnlySpan<char> payload = destination.AsSpan(png ? PngPrefix.Length : JpegPrefix.Length);
        maximumBytes = Math.Min(maximumBytes, MaximumEncodedBytes);
        if (maximumBytes < (ulong)signature.Length || payload.Length == 0 || payload.Length % 4 != 0)
            return null;

        int padding = payload[^1] == '=' ? payload[^2] == '=' ? 2 : 1 : 0;
        ulong decodedLength = (ulong)(payload.Length / 4) * 3 - (uint)padding;
        // Check the exact decoded length before allocating. No substring or
        // temporary full-size copy of the encoded URI is created.
        if (decodedLength < (ulong)signature.Length || decodedLength > maximumBytes)
            return null;

        byte[] bytes = new byte[(int)decodedLength];
        int written = 0;
        const int charactersPerChunk = 16 * 1024; // Complete groups of four.
        for (int start = 0; start < payload.Length; start += charactersPerChunk)
        {
            cancellation.ThrowIfCancellationRequested();
            ReadOnlySpan<char> chunk = payload.Slice(start, Math.Min(charactersPerChunk, payload.Length - start));
            for (int index = 0; index < chunk.Length; ++index)
            {
                char value = chunk[index];
                if (value is >= 'A' and <= 'Z' or >= 'a' and <= 'z' or >= '0' and <= '9' or '+' or '/')
                    continue;
                if (value != '=' || start + index < payload.Length - padding)
                    return null;
            }
            if (!Convert.TryFromBase64Chars(chunk, bytes.AsSpan(written), out int count))
                return null;
            written += count;
        }

        cancellation.ThrowIfCancellationRequested();
        return written == bytes.Length && bytes.AsSpan(0, signature.Length).SequenceEqual(signature) ? bytes : null;
    }
}
