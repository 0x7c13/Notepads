namespace Notepads.Extensions
{
    using System;
    using Windows.UI;

    public static class ColorExtensions
    {
        /// <summary>
        /// Convert the color hexadecimal string to color.
        /// <para>
        /// Support:
        /// </para>
        /// <para>- #FFDFD991</para>
        /// <para>- #DFD991</para>
        /// <para>- #FD92</para>
        /// <para>- #DAC</para>
        /// <para>The # character can be omitted</para>
        /// </summary>
        /// <param name="hexColorString"></param>
        /// <returns></returns>
        public static Color ToColor(string hexColorString)
        {
            if (string.IsNullOrEmpty(hexColorString)) throw new ArgumentNullException(nameof(hexColorString));

            var hex = hexColorString;
            hex = hex.Replace("#", string.Empty);

            //#FFDFD991
            //#DFD991
            //#FD92
            //#DAC

            bool existAlpha = hex.Length == 8 || hex.Length == 4;
            bool isDoubleHex = hex.Length == 8 || hex.Length == 6;

            if (!existAlpha && hex.Length != 6 && hex.Length != 3)
            {
                throw new ArgumentException($@"Input string {hexColorString} is invalid color.
The supported formats are:
- #FFDFD991 / FFDFD991
- #DFD991 / DFD991
- #FD92 / FD92
- #DAC / DAC");
            }

            int n = 0;
            byte a;
            int hexCount = isDoubleHex ? 2 : 1;
            if (existAlpha)
            {
                n = hexCount;
                a = (byte) ConvertHexToByte(hex, 0, hexCount);
                if (!isDoubleHex)
                {
                    //#FD92 = #FFDD9922
                    //Duplicate characters
                    a = (byte) (a * 16 + a);
                }
            }
            else
            {
                a = 0xFF;
            }

            var r = (byte) ConvertHexToByte(hex, n, hexCount);
            var g = (byte) ConvertHexToByte(hex, n + hexCount, hexCount);
            var b = (byte) ConvertHexToByte(hex, n + 2 * hexCount, hexCount);
            if (!isDoubleHex)
            {
                //#FD92 = #FFDD9922

                r = (byte) (r * 16 + r);
                g = (byte) (g * 16 + g);
                b = (byte) (b * 16 + b);
            }

            return Windows.UI.Color.FromArgb(a, r, g, b);
        }

        private static uint ConvertHexToByte(string hex, int n, int count = 2)
        {
            return Convert.ToUInt32(hex.Substring(n, count), 16);
        }
    }
}
