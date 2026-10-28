// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Utilities
{
    public enum LineEnding
    {
        Crlf,
        Cr,
        Lf
    }

    public static class LineEndingUtility
    {
        public static LineEnding GetLineEndingTypeFromText(string text)
        {
            var containsCr = false;
            var containsLf = false;

            for (var i = 0; i < text.Length; i++)
            {
                if (text[i] == '\r')
                {
                    if (i + 1 < text.Length && text[i + 1] == '\n')
                    {
                        return LineEnding.Crlf;
                    }

                    containsCr = true;
                }
                else if (text[i] == '\n')
                {
                    containsLf = true;
                }
            }

            if (containsCr)
            {
                return LineEnding.Cr;
            }

            if (containsLf)
            {
                return LineEnding.Lf;
            }

            return LineEnding.Crlf;
        }

        public static string GetLineEndingDisplayText(LineEnding lineEnding)
        {
            switch (lineEnding)
            {
                case LineEnding.Crlf:
                    return "Windows (CRLF)";
                case LineEnding.Cr:
                    return "Macintosh (CR)";
                case LineEnding.Lf:
                    return "Unix (LF)";
                default:
                    return "Windows (CRLF)";
            }
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
            switch (lineEnding)
            {
                case LineEnding.Cr:
                    text = text.Replace("\r\n", "\n").Replace("\r", "\n").Replace("\n", "\r");
                    break;
                case LineEnding.Crlf:
                    text = text.Replace("\r\n", "\n").Replace("\r", "\n").Replace("\n", "\r\n");
                    break;
                case LineEnding.Lf:
                    text = text.Replace("\r\n", "\n").Replace("\r", "\n");
                    break;
            }

            return text;
        }
    }
}