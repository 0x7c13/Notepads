namespace Notepads.Utilities
{
    using System;
    using System.Linq;
    using System.Collections.Generic;
    using System.Text;
    using System.Threading;
    using Microsoft.AppCenter.Analytics;
    using Notepads.Services;

    public static class EncodingUtility
    {
        private static Encoding[] _allSupportedANSIEncodings;

        private static Encoding _systemDefaultANSIEncoding;

        private static Encoding _currentCultureANSIEncoding;

        public static string GetEncodingName(Encoding encoding)
        {
            var encodingName = "ANSI";

            if (encoding is UTF7Encoding)
            {
                encodingName = "UTF-7";
            }
            else if (encoding is UTF8Encoding)
            {
                if (Equals(encoding, new UTF8Encoding(true)))
                {
                    encodingName = "UTF-8-BOM";
                }
                else
                {
                    encodingName = "UTF-8";
                }
            }
            else if (encoding is UnicodeEncoding)
            {
                if (Equals(encoding, new UnicodeEncoding(bigEndian: true, byteOrderMark: true)))
                {
                    encodingName = "UTF-16 BE BOM";
                }
                else if (Equals(encoding, new UnicodeEncoding(bigEndian: false, byteOrderMark: true)))
                {
                    encodingName = "UTF-16 LE BOM";
                }
                else if (Equals(encoding, new UnicodeEncoding(bigEndian: true, byteOrderMark: false)))
                {
                    encodingName = "UTF-16 BE";
                }
                else if (Equals(encoding, new UnicodeEncoding(bigEndian: false, byteOrderMark: false)))
                {
                    encodingName = "UTF-16 LE";
                }
            }
            else if (encoding is UTF32Encoding)
            {
                if (Equals(encoding, new UTF32Encoding(bigEndian: true, byteOrderMark: true)))
                {
                    encodingName = "UTF-32 BE BOM";
                }
                if (Equals(encoding, new UTF32Encoding(bigEndian: false, byteOrderMark: true)))
                {
                    encodingName = "UTF-32 LE BOM";
                }
                else if (Equals(encoding, new UTF32Encoding(bigEndian: true, byteOrderMark: false)))
                {
                    encodingName = "UTF-32 BE";
                }
                else if (Equals(encoding, new UTF32Encoding(bigEndian: false, byteOrderMark: false)))
                {
                    encodingName = "UTF-32 LE";
                }
            }
            else
            {
                try
                {
                    encodingName = encoding.EncodingName;
                    LoggingService.LogInfo($"Encoding name: {encodingName}");
                }
                catch (Exception ex)
                {
                    LoggingService.LogException(ex);
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
                    return Encoding.Equals(p, q);
                }
                else
                {
                    return true;
                }
            }

            return false;
        }

        public static Encoding GetEncodingByName(string name, Encoding fallbackEncoding = null)
        {
            switch (name)
            {
                case "ANSI":
                {
                    if (TryGetSystemDefaultANSIEncoding(out var systemDefaultANSIEncoding))
                    {
                        return systemDefaultANSIEncoding;
                    }
                    else if (TryGetCurrentCultureANSIEncoding(out var currentCultureANSIEncoding))
                    {
                        return currentCultureANSIEncoding;
                    }
                    else
                    {
                        return new UTF8Encoding(false);
                    }
                }
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
                    return fallbackEncoding ?? new UTF8Encoding(false);
                    //    try
                    //    {
                    //        return Encoding.GetEncoding(name);
                    //    }
                    //    catch (Exception ex)
                    //    {
                    //        LoggingService.LogException(ex);
                    //        return fallbackEncoding ?? new UTF8Encoding(false);
                    //    }
            }
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
                Analytics.TrackEvent("EncodingUtility_FailedToGetSystemDefaultANSIEncoding", new Dictionary<string, string>() {
                    {
                        "Message", ex.Message
                    },
                    {
                        "Exception", ex.ToString()
                    },
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
                Analytics.TrackEvent("EncodingUtility_FailedToGetCurrentCultureANSIEncoding", new Dictionary<string, string>() {
                    {
                        "Message", ex.Message
                    },
                    {
                        "Exception", ex.ToString()
                    },
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

            // https://docs.microsoft.com/en-us/windows/win32/intl/code-page-identifiers
            var ANSIEncodings = new Dictionary<int, string>()
            {
                { 1252,    "Western (Windows 1252) windows1252" },
                { 28591,   "Western (ISO 8859-1) iso88591" },
                { 28593,   "Western (ISO 8859-3) iso88593" },
                { 28605,   "Western (ISO 8859-15) iso885915" },
                { 10000,   "Western (Mac Roman) macroman" },
                { 437,     "DOS (CP 437) cp437" },
                { 1256,    "Arabic (Windows 1256) windows1256" },
                { 28596,   "Arabic (ISO 8859-6) iso88596" },
                { 1257,    "Baltic (Windows 1257) winclows1257" },
                { 28594,   "Baltic (ISO 8859-4) iso88594" },
                { 1250,    "Central European (Windows 1250) windows1250" },
                { 28592,   "Central European (ISO 8859-2) iso88592" },
                { 852,     "Central European (CP 852) cp852" },
                { 1251,    "Cyrillic (Windows 1251) windows1251" },
                { 866,     "Cyrillic (CP 866) cp866" },
                { 28595,   "Cyrillic (ISO 8859-5) is088595" },
                { 20866,   "Cyrillic (K018-R) koi8r" },
                { 21866,   "Cyrillic (K018-U) koiSu" },
                //{ ,      "Cyrillic (K018-RU) koi8ru" },
                { 28603,   "Estonian (ISO 8859-13) iso885913" },
                { 1253,    "Greek (Windows 1253) windows1253" },
                { 28597,   "Greek (ISO 8859-7) iso88597" },
                { 1255,    "Hebrew (Windows 1255) windows1255" },
                { 28598,   "Hebrew (ISO 8859-8) iso88598" },
                { 932,     "Japanese (Shift JIS) shiftjis" },
                { 51932,   "Japanese (EUC-JP) eucjp" },
                { 51949,   "Korean (EUC-KR) euckr" },
                { 865,     "Nordic DOS (CP 865) cp865" },
                //{ ,      "Nordic (ISO 8859-10) iso885910" },
                //{ ,      "Romanian (ISO 8859-16) iso885916" },
                //{ ,      "Simplified Chinese (GBK) gbk" },
                { 936,     "Simplified Chinese (GB 2312) gb2312" },
                { 54936,   "Simplified Chinese (6818030) gb18030" },
                //{ ,      "Tajik (K018-T) koi8t" },
                { 874,     "Thai (Windows 874) windows874" },
                { 1254,    "Turkish (Windows 1254) windows1254" },
                { 28599,   "Turkish (ISO 8859-9) iso88599" },
                { 950,     "Traditional Chinese (Big5) cp950" },
                //{ ,      "Traditional Chinese (Big5-HKSCS) big5hkscs" },
                { 1258,    "Vietnamese (Windows 1258) windows1258" },
                { 850,     "Western European DOS (CP 850) cp850" }
            };

            Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);

            var encodings = new HashSet<Encoding>();

            foreach (var encoding in ANSIEncodings)
            {
                try
                {
                    encodings.Add(Encoding.GetEncoding(encoding.Key));
                }
                catch (Exception ex)
                {
                    Analytics.TrackEvent("EncodingUtility_FailedToGetANSIEncoding", new Dictionary<string, string>() {
                        {
                            "Message", ex.Message
                        },
                        {
                            "Exception", ex.ToString()
                        },
                        {
                            "CodePage", encoding.Key.ToString()
                        },
                        {
                            "EncodingName", encoding.Value
                        }
                    });
                }
            }

            _allSupportedANSIEncodings = encodings.ToArray();
            return _allSupportedANSIEncodings;
        }
    }
}