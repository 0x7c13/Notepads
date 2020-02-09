namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using System.Text;
    using System.Threading;
    using Microsoft.AppCenter.Analytics;
    using Notepads.Services;

    public static class EncodingUtility
    {
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
            //else
            //{
            //    try
            //    {
            //        encodingName = encoding.EncodingName;
            //        LoggingService.LogInfo($"Encoding name: {encodingName}");
            //    }
            //    catch (Exception ex)
            //    {
            //        LoggingService.LogException(ex);
            //    }
            //}

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
                    return GetSystemCurrentANSIEncoding() ?? new UTF8Encoding(false);
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

        public static Encoding GetSystemCurrentANSIEncoding()
        {
            try
            {
                Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);
                var ansiCodePage = Thread.CurrentThread.CurrentCulture.TextInfo.ANSICodePage;
                var ansiEncoding = Encoding.GetEncoding(Thread.CurrentThread.CurrentCulture.TextInfo.ANSICodePage);

                try
                {
                    LoggingService.LogInfo($"[GetSystemCurrentANSIEncoding] System ANSI encoding: [Name: {ansiEncoding.EncodingName} ANSICodePage: {ansiCodePage}]");
                }
                catch
                {
                    // ignored
                }

                return ansiEncoding;
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to get system current ANSI encoding: {ex.Message}");
                Analytics.TrackEvent("FailedToGetANSIEncoding", new Dictionary<string, string>
                {
                    {"ExceptionMessage", ex.Message}
                });
                return null;
            }
        }
    }
}