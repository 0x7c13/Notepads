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
            if (lineEnding == LineEnding.Cr)
            {
                text = text.Replace("\r\n", "\n").Replace("\r", "\n").Replace("\n", "\r");
            }
            else if (lineEnding == LineEnding.Crlf)
            {
                text = text.Replace("\r\n", "\n").Replace("\r", "\n").Replace("\n", "\r\n");
            }
            else // LF
            {
                text = text.Replace("\r\n", "\n").Replace("\r", "\n");
            }

            return text;
        }
    }
}