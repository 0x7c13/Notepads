﻿namespace Notepads.Utilities
{
    using System;
    using System.Linq;
    using System.Collections.Generic;
    using System.Text;
    using System.Threading;
    using Microsoft.AppCenter.Analytics;

    public static class EncodingUtility
    {
        private static Encoding[] _allSupportedANSIEncodings;

        private static Encoding _systemDefaultANSIEncoding;

        private static Encoding _currentCultureANSIEncoding;

        // https://docs.microsoft.com/en-us/windows/win32/intl/code-page-identifiers
        // Name is using format of "<CountryOrRegion> (<.NET Name>)"
        private static readonly Dictionary<int, string> ANSIEncodings = new Dictionary<int, string>()
        {
            { 1252,    "Western (windows-1252)" },
            { 28591,   "Western (iso-8859-1)" },
            { 28593,   "Western (iso-8859-3)" },
            { 28605,   "Western (iso-8859-15)" },
            { 10000,   "Western (macintosh)" },
            { 437,     "DOS (IBM437)" },
            { 1256,    "Arabic (windows-1256)" },
            { 28596,   "Arabic (iso-8859-6)" },
            { 1257,    "Baltic (windows-1257)" },
            { 28594,   "Baltic (iso-8859-4)" },
            { 1250,    "Central European (windows-1250)" },
            { 10029,   "Central European (x-mac-ce)" },
            { 28592,   "Central European (iso-8859-2)" },
            { 852,     "Central European (ibm852)" },
            { 1251,    "Cyrillic (windows-1251)" },
            { 10007,   "Cyrillic (x-mac-cyrillic)" },
            { 866,     "Cyrillic (cp866)" },
            { 855,     "Cyrillic (IBM855)" },
            { 28595,   "Cyrillic (iso-8859-5)" },
            { 20866,   "Cyrillic (koi8-r)" },
            { 21866,   "Cyrillic (koi8-u)" },
            { 28603,   "Estonian (iso-8859-13)" },
            { 1253,    "Greek (windows-1253)" },
            { 28597,   "Greek (iso-8859-7)" },
            { 1255,    "Hebrew (windows-1255)" },
            { 28598,   "Hebrew (iso-8859-8)" },
            { 932,     "Japanese (shift_jis)" },
            { 51932,   "Japanese (euc-jp)" },
            { 50220,   "Japanese (iso-2022-jp)" },
            { 51949,   "Korean (euc-kr)" },
            { 949,     "Korean (ks_c_5601-1987)" },
            { 50225,   "Korean (iso-2022-kr)" },
            { 865,     "Nordic DOS (IBM865)" },
            { 936,     "Simplified Chinese (gb2312)" },
            { 54936,   "Simplified Chinese (GB18030)" },
            { 874,     "Thai (windows-874)" },
            { 1254,    "Turkish (windows-1254)" },
            { 28599,   "Turkish (iso-8859-9)" },
            { 950,     "Traditional Chinese (big5)" },
            { 1258,    "Vietnamese (windows-1258)" },
            { 850,     "Western European DOS (ibm850)" }
        };

        public static string GetEncodingName(Encoding encoding)
        {
            string encodingName;

            switch (encoding)
            {
                case UTF7Encoding _:
                    encodingName = "UTF-7";
                    break;
                case UTF8Encoding _ when Equals(encoding, new UTF8Encoding(encoderShouldEmitUTF8Identifier: true)):
                    encodingName = "UTF-8-BOM";
                    break;
                case UTF8Encoding _ when Equals(encoding, new UTF8Encoding(encoderShouldEmitUTF8Identifier: false)):
                    encodingName = "UTF-8";
                    break;
                case UnicodeEncoding _ when Equals(encoding, new UnicodeEncoding(bigEndian: true, byteOrderMark: true)):
                    encodingName = "UTF-16 BE BOM";
                    break;
                case UnicodeEncoding _ when Equals(encoding, new UnicodeEncoding(bigEndian: false, byteOrderMark: true)):
                    encodingName = "UTF-16 LE BOM";
                    break;
                case UnicodeEncoding _ when Equals(encoding, new UnicodeEncoding(bigEndian: true, byteOrderMark: false)):
                    encodingName = "UTF-16 BE";
                    break;
                case UnicodeEncoding _ when Equals(encoding, new UnicodeEncoding(bigEndian: false, byteOrderMark: false)):
                    encodingName = "UTF-16 LE";
                    break;
                case UTF32Encoding _ when Equals(encoding, new UTF32Encoding(bigEndian: true, byteOrderMark: true)):
                    encodingName = "UTF-32 BE BOM";
                    break;
                case UTF32Encoding _ when Equals(encoding, new UTF32Encoding(bigEndian: false, byteOrderMark: true)):
                    encodingName = "UTF-32 LE BOM";
                    break;
                case UTF32Encoding _ when Equals(encoding, new UTF32Encoding(bigEndian: true, byteOrderMark: false)):
                    encodingName = "UTF-32 BE";
                    break;
                case UTF32Encoding _ when Equals(encoding, new UTF32Encoding(bigEndian: false, byteOrderMark: false)):
                    encodingName = "UTF-32 LE";
                    break;
                default:
                    encodingName = GetEncodingNameFallback(encoding);
                    break;
            }

            return encodingName;
        }

        private static string GetEncodingNameFallback(Encoding encoding)
        {
            string encodingName = "Unknown";

            if (ANSIEncodings.ContainsKey(encoding.CodePage))
            {
                encodingName = ANSIEncodings[encoding.CodePage];
            }
            else
            {
                try
                {
                    encodingName = encoding.WebName; // WebName is supported by Encoding.GetEncoding(WebName)
                    Analytics.TrackEvent("EncodingUtility_FoundUnlistedEncoding", new Dictionary<string, string>()
                    {
                        {"CodePage", encoding.CodePage.ToString()},
                        {"WebName", encoding.WebName}
                    });
                }
                catch (Exception ex)
                {
                    Analytics.TrackEvent("EncodingUtility_FailedToGetNameOfUnlistedEncoding", new Dictionary<string, string>()
                    {
                        {"Exception", ex.ToString()},
                        {"Message", ex.Message}
                    });
                }
            }

            return encodingName;
        }

        public static bool Equals(Encoding p, Encoding q)
        {
            if (p.CodePage == q.CodePage)
            {
                if (q is UTF7Encoding ||
                    q is UTF8Encoding ||
                    q is UnicodeEncoding ||
                    q is UTF32Encoding)
                {
                    return Encoding.Equals(p, q); // To make sure we compare bigEndian and byteOrderMark flags
                }
                else
                {
                    return true;
                }
            }

            return false;
        }

        public static Encoding GetEncodingByName(string name)
        {
            switch (name)
            {
                case "ANSI":
                    if (TryGetSystemDefaultANSIEncoding(out var systemDefaultANSIEncoding)) return systemDefaultANSIEncoding;
                    else return TryGetCurrentCultureANSIEncoding(out var currentCultureANSIEncoding) ? currentCultureANSIEncoding : new UTF8Encoding(false);
                case "UTF-7":
                    return new UTF7Encoding();
                case "UTF-8":
                    return new UTF8Encoding(false);
                case "UTF-8-BOM":
                    return new UTF8Encoding(true);
                case "UTF-16 BE BOM":
                    return new UnicodeEncoding(true, true);
                case "UTF-16 LE BOM":
                    return new UnicodeEncoding(false, true);
                case "UTF-16 BE":
                    return new UnicodeEncoding(true, false);
                case "UTF-16 LE":
                    return new UnicodeEncoding(false, false);
                case "UTF-32 BE BOM":
                    return new UTF32Encoding(true, true);
                case "UTF-32 LE BOM":
                    return new UTF32Encoding(false, true);
                case "UTF-32 BE":
                    return new UTF32Encoding(true, false);
                case "UTF-32 LE":
                    return new UTF32Encoding(false, false);
                default:
                    return GetEncodingByNameFallback(name);
            }
        }

        private static Encoding GetEncodingByNameFallback(string name)
        {
            foreach (var (codePage, encodingName) in ANSIEncodings)
            {
                if (string.Equals(encodingName, name, StringComparison.InvariantCultureIgnoreCase))
                {
                    return Encoding.GetEncoding(codePage);
                }
            }

            try
            {
                Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);
                return Encoding.GetEncoding(name);
            }
            catch (Exception ex)
            {
                Analytics.TrackEvent("EncodingUtility_FailedToGetEncoding", new Dictionary<string, string>()
                {
                    {"EncodingName", name},
                    {"Exception", ex.ToString()}
                });
            }

            return new UTF8Encoding(false);
        }

        public static bool TryGetSystemDefaultANSIEncoding(out Encoding encoding)
        {
            try
            {
                if (_systemDefaultANSIEncoding != null)
                {
                    encoding = _systemDefaultANSIEncoding;
                    return true;
                }
                Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);
                encoding = Encoding.GetEncoding(0);
                _systemDefaultANSIEncoding = encoding;
                return true;
            }
            catch (Exception ex)
            {
                Analytics.TrackEvent("EncodingUtility_FailedToGetSystemDefaultANSIEncoding", new Dictionary<string, string>()
                {
                    { "Message", ex.Message },
                    { "Exception", ex.ToString() },
                });
            }

            encoding = null;
            return false;
        }

        public static bool TryGetCurrentCultureANSIEncoding(out Encoding encoding)
        {
            try
            {
                if (_currentCultureANSIEncoding != null)
                {
                    encoding = _currentCultureANSIEncoding;
                    return true;
                }
                Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);
                encoding = Encoding.GetEncoding(Thread.CurrentThread.CurrentCulture.TextInfo.ANSICodePage);
                _currentCultureANSIEncoding = encoding;
                return true;
            }
            catch (Exception ex)
            {
                Analytics.TrackEvent("EncodingUtility_FailedToGetCurrentCultureANSIEncoding", new Dictionary<string, string>()
                {
                    { "Message", ex.Message },
                    { "Exception", ex.ToString() },
                });
            }

            encoding = null;
            return false;
        }

        public static Encoding[] GetAllSupportedANSIEncodings()
        {
            if (_allSupportedANSIEncodings != null)
            {
                return _allSupportedANSIEncodings;
            }

            Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);

            var encodings = new HashSet<Encoding>();

            foreach (var (codePage, encodingName) in ANSIEncodings)
            {
                try
                {
                    encodings.Add(Encoding.GetEncoding(codePage));
                }
                catch (Exception ex)
                {
                    Analytics.TrackEvent("EncodingUtility_FailedToGetANSIEncoding", new Dictionary<string, string>()
                    {
                        { "Message", ex.Message },
                        { "Exception", ex.ToString() },
                        { "CodePage", codePage.ToString() },
                        { "EncodingName", encodingName }
                    });
                }
            }

            _allSupportedANSIEncodings = encodings.ToArray();
            return _allSupportedANSIEncodings;
        }
    }
}