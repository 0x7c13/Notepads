namespace Notepads.Extensions
{
    using System;
    using System.Linq;

    public static class StringExtensions
    {
        public static string LeadingSpacesAndTabs(this string str)
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

        public static int IndexOfWholeWord(this string str, string value, int startIndex, StringComparison comparison)
        {
            int pos = startIndex;

            while (pos < str.Length && (pos = str.IndexOf(value, pos, comparison)) != -1)
            {
                bool startBoundary = true;
                if (pos > 0)
                    startBoundary = !char.IsLetterOrDigit(str[pos - 1]);

                bool endBoundary = true;
                if (pos + value.Length < str.Length)
                    endBoundary = !char.IsLetterOrDigit(str[pos + value.Length]);

                if (startBoundary && endBoundary)
                    return pos;

                pos++;
            }
            return -1;
        }

        public static int LastIndexOfWholeWord(this string str, string value, int startIndex, StringComparison comparison)
        {
            int pos = startIndex;

            while (pos >= 0 && (pos = str.LastIndexOf(value, pos, comparison)) != -1)
            {
                bool startBoundary = true;
                if (pos > 0)
                    startBoundary = !char.IsLetterOrDigit(str[pos - 1]);

                bool endBoundary = true;
                if (pos + value.Length < str.Length)
                    endBoundary = !char.IsLetterOrDigit(str[pos + value.Length]);

                if (startBoundary && endBoundary)
                    return pos;

                pos--;
            }
            return -1;
        }

        public static Uri ToAppxUri(this string path)
        {
            string prefix = $"ms-appx://{(path.StartsWith('/') ? string.Empty : "/")}";
            return new Uri($"{prefix}{path}");
        }

        public static bool ContainsAllowableCharactersOnly(this string str, params char[] allowableCharacters)
        {
            return str.All(allowableCharacters.Contains);
        }
    }
}