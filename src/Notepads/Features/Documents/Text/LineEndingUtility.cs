// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;

namespace Notepads.Features.Documents.Text;

public static class LineEndingUtility
{
    public static LineEnding GetLineEndingTypeFromText(string text)
    {
        if (text.Contains("\r\n"))
        {
            return LineEnding.Crlf;
        }
        else if (text.Contains("\r"))
        {
            return LineEnding.Cr;
        }
        else if (text.Contains("\n"))
        {
            return LineEnding.Lf;
        }
        else
        {
            return LineEnding.Crlf;
        }
    }

    public static string GetLineEndingDisplayText(LineEnding lineEnding)
    {
        return lineEnding switch
        {
            LineEnding.Crlf => "Windows (CRLF)",
            LineEnding.Cr => "Macintosh (CR)",
            LineEnding.Lf => "Unix (LF)",
            _ => "Windows (CRLF)",
        };
    }

    public static string GetLineEndingName(LineEnding lineEnding)
    {
        string lineEndingName = "CRLF";

        switch (lineEnding)
        {
            case LineEnding.Crlf:
                lineEndingName = "CRLF";
                break;
            case LineEnding.Cr:
                lineEndingName = "CR";
                break;
            case LineEnding.Lf:
                lineEndingName = "LF";
                break;
        }

        return lineEndingName;
    }

    public static LineEnding GetLineEndingByName(string name)
    {
        LineEnding lineEnding = LineEnding.Crlf;

        switch (name.ToUpper())
        {
            case "CRLF":
                lineEnding = LineEnding.Crlf;
                break;
            case "CR":
                lineEnding = LineEnding.Cr;
                break;
            case "LF":
                lineEnding = LineEnding.Lf;
                break;
        }

        return lineEnding;
    }

    public static string ApplyLineEnding(string text, LineEnding lineEnding)
    {
        var ending = GetSequence(lineEnding);
        StringBuilder builder = null;
        var segmentStart = 0;
        for (var i = 0; i < text.Length; i++)
        {
            if (text[i] != '\r' && text[i] != '\n') continue;
            var length = text[i] == '\r' && i + 1 < text.Length && text[i + 1] == '\n' ? 2 : 1;
            var matches = length == ending.Length && text[i] == ending[0];
            if (!matches || builder != null)
            {
                if (builder == null) builder = new StringBuilder(text.Length);
                builder.Append(text, segmentStart, i - segmentStart);
                builder.Append(ending);
                segmentStart = i + length;
            }
            i += length - 1;
        }
        if (builder == null) return text;
        builder.Append(text, segmentStart, text.Length - segmentStart);
        return builder.ToString();
    }

    private static string GetSequence(LineEnding lineEnding) => lineEnding == LineEnding.Cr ? "\r" : lineEnding == LineEnding.Lf ? "\n" : "\r\n";

    /// <summary>Encode/save with bounded temporary memory, including mixed EOLs.</summary>
    public static async Task WriteAsync(TextWriter writer, string text, LineEnding? lineEnding = null)
    {
        var buffer = new char[16384];
        var count = 0;
        var ending = lineEnding.HasValue ? GetSequence(lineEnding.Value) : null;
        for (var i = 0; i < text.Length; i++)
        {
            var character = text[i];
            if (ending != null && (character == '\r' || character == '\n'))
            {
                if (character == '\r' && i + 1 < text.Length && text[i + 1] == '\n') i++;
                foreach (var endCharacter in ending) buffer[count++] = endCharacter;
            }
            else
            {
                buffer[count++] = character;
            }
            // Leave room for the two-character CRLF sequence. StreamWriter's
            // encoder retains a high surrogate across WriteAsync calls.
            if (count >= buffer.Length - 1)
            {
                await writer.WriteAsync(buffer, 0, count).ConfigureAwait(false);
                count = 0;
            }
        }
        if (count > 0) await writer.WriteAsync(buffer, 0, count).ConfigureAwait(false);
    }
}
