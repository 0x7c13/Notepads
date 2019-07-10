
namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;
    using System.Threading.Tasks;

    public static class EncodingUtility
    {
        public static string GetEncodingBodyName(Encoding encoding)
        {
            var encodingBodyName = encoding.BodyName.ToUpper();

            if (encoding is UTF8Encoding)
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
                encodingBodyName = encoding.BodyName;
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
    }
}
