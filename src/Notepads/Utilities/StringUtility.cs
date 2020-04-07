namespace Notepads.Utilities
{
    using System;

    public static class StringUtility
    {
        public static string GetLeadingSpacesAndTabs(string str)
        {
            int i = 0;
            for (; i < str.Length; i++)
            {
                var ch = str[i];
                if (ch != ' ' && ch != '\t')
                {
                    break;
                }
            }
            return str.Substring(0, i);
        }

        public static int IndexOfWholeWord(string target, int startIndex, string value, StringComparison comparison)
        {
            int pos = startIndex;

            while (pos < target.Length && (pos = target.IndexOf(value, pos, comparison)) != -1)
            {
                bool startBoundary = true;
                if (pos > 0)
                    startBoundary = !char.IsLetterOrDigit(target[pos - 1]);

                bool endBoundary = true;
                if (pos + value.Length < target.Length)
                    endBoundary = !char.IsLetterOrDigit(target[pos + value.Length]);

                if (startBoundary && endBoundary)
                    return pos;

                pos++;
            }
            return -1;
        }

        public static int LastIndexOfWholeWord(string target, int startIndex, string value, StringComparison comparison)
        {
            int pos = startIndex;

            while (pos >= 0 && (pos = target.LastIndexOf(value, pos, comparison)) != -1)
            {
                bool startBoundary = true;
                if (pos > 0)
                    startBoundary = !char.IsLetterOrDigit(target[pos - 1]);

                bool endBoundary = true;
                if (pos + value.Length < target.Length)
                    endBoundary = !char.IsLetterOrDigit(target[pos + value.Length]);

                if (startBoundary && endBoundary)
                    return pos;

                pos--;
            }
            return -1;
        }
    }
}