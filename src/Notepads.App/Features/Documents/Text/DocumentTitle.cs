// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Buffers;
using System.Globalization;
using System.Text;

namespace Notepads.Features.Documents.Text;

/// <summary>A bounded display label, independent of filenames and language detection.</summary>
public static class DocumentTitle
{
    public const int SampleBytes = 1024;
    public const int MaximumLength = 160;
    // UTF-8 decoding produces no more UTF-16 units than source bytes. Keep the
    // pure helper bounded too, including callers that supply a managed string.
    private const int MaximumPrefixChars = SampleBytes;
    private const int MaximumTextElements = 40;

    public static string FromPrefix(ReadOnlySpan<char> prefix, bool complete = true)
    {
        if (prefix.Length > MaximumPrefixChars)
        {
            prefix = prefix[..MaximumPrefixChars];
            complete = false;
        }
        Span<char> normalized = stackalloc char[MaximumPrefixChars];
        var length = 0;
        var space = false;
        var lineEnded = false;
        while (!prefix.IsEmpty)
        {
            if (Rune.DecodeFromUtf16(prefix, out var rune, out var consumed) != OperationStatus.Done)
            {
                rune = Rune.ReplacementChar;
                consumed = 1;
            }
            prefix = prefix[consumed..];
            if (rune.Value is '\r' or '\n' or 0x85 or 0x2028 or 0x2029)
            {
                if (length == 0) continue;
                lineEnded = true;
                break;
            }
            if (Rune.IsWhiteSpace(rune)) { space = length > 0; continue; }
            // Keep joiners/variation selectors for emoji and scripts, but do not
            // allow embedded controls or directional overrides to reorder the UI.
            if (Rune.IsControl(rune) || rune.Value is 0x061c or 0x200b or 0x200e or 0x200f or
                >= 0x202a and <= 0x202e or >= 0x2066 and <= 0x2069 or 0xfeff) continue;
            if (space) normalized[length++] = ' ';
            space = false;
            length += rune.EncodeToUtf16(normalized[length..]);
        }

        var text = normalized[..length];
        var end = 0;
        var previous = 0;
        var elements = 0;
        while (end < text.Length && elements < MaximumTextElements)
        {
            var next = end + StringInfo.GetNextTextElementLength(text[end..]);
            if (next > MaximumLength - 1) break;
            previous = end;
            end = next;
            elements++;
        }
        // A bounded native read may end inside a combining/ZWJ sequence. Its
        // final cluster is not a proven boundary until a delimiter or EOF.
        if (end == text.Length && !complete && !lineEnded && !space) end = previous;
        if (end == 0) return null;
        var truncated = end < text.Length || !complete && !lineEnded;
        var title = text[..end].TrimEnd();
        return truncated ? string.Concat(title, "…") : title.ToString();
    }
}
