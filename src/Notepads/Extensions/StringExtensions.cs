namespace Notepads.Extensions
{
    using System;
    using Windows.UI;

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

        public static int IndexOfWholeWord(this string target, string value, int startIndex, StringComparison comparison)
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

        public static int LastIndexOfWholeWord(this string target, string value, int startIndex, StringComparison comparison)
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

        public static Uri ToAppxUri(this string path)
        {
            string prefix = $"ms-appx://{(path.StartsWith('/') ? string.Empty : "/")}";
            return new Uri($"{prefix}{path}");
        }

        public static Color ToColor(this string hexValue)
        {
            hexValue = hexValue.Replace("#", string.Empty);
            byte a = (byte)(Convert.ToUInt32(hexValue.Substring(0, 2), 16));
            byte r = (byte)(Convert.ToUInt32(hexValue.Substring(2, 2), 16));
            byte g = (byte)(Convert.ToUInt32(hexValue.Substring(4, 2), 16));
            byte b = (byte)(Convert.ToUInt32(hexValue.Substring(6, 2), 16));
            return Windows.UI.Color.FromArgb(a, r, g, b);
        }
    }
}