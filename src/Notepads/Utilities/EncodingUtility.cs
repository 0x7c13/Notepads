
namespace Notepads.Utilities
{
    using Microsoft.AppCenter.Analytics;
    using Notepads.Services;
    using System;
    using System.Collections.Generic;
    using System.Text;
    using System.Threading;

    public static class EncodingUtility
    {
        public static string GetEncodingBodyName(Encoding encoding)
        {
            var encodingBodyName = "ANSI";

            if (encoding is UTF7Encoding)
            {
                encodingBodyName = "UTF-7";
            }
            else if (encoding is UTF8Encoding)
            {
                if (Equals(encoding, new UTF8Encoding(true)))
                {
                    encodingBodyName = "UTF-8-BOM";
                }
                else
                {
                    encodingBodyName = "UTF-8";
                }
            }
            else if (encoding is UnicodeEncoding)
            {
                if (encoding.CodePage == Encoding.Unicode.CodePage)
                {
                    encodingBodyName = "UTF-16 LE BOM";
                }
                else if (encoding.CodePage == Encoding.BigEndianUnicode.CodePage)
                {
                    encodingBodyName = "UTF-16 BE BOM";
                }
            }
            else
            {
                try
                {
                    encodingBodyName = encoding.BodyName.ToUpper();
                }
                catch (Exception)
                {
                    // Ignore
                }
            }

            return encodingBodyName;
        }

        public static bool Equals(Encoding p, Encoding q)
        {
            if (p.CodePage == q.CodePage)
            {
                if (q is UTF8Encoding)
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

        public static Encoding GetEncodingByName(string name)
        {
            switch (name)
            {
                case "ANSI":
                    return GetSystemCurrentANSIEncoding();
                case "UTF-8":
                    return new UTF8Encoding(false);
                case "UTF-8-BOM":
                    return new UTF8Encoding(true);
                case "UTF-16 LE BOM":
                    return new UnicodeEncoding(false, true);
                case "UTF-16 BE BOM":
                    return new UnicodeEncoding(true, true);
                default:
                    return Encoding.GetEncoding(name);
            }
        }

        public static Encoding GetSystemCurrentANSIEncoding()
        {
            try
            {
                Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);
                return Encoding.GetEncoding(Thread.CurrentThread.CurrentCulture.TextInfo.ANSICodePage);
            }
            catch (Exception ex)
            {
                LoggingService.LogException(ex);
                Analytics.TrackEvent("FailedToGetANSIEncoding", new Dictionary<string, string>
                {
                    {"ExceptionMessage", ex.Message}
                });
                return new UTF8Encoding(false);
            }
        }
    }
}
