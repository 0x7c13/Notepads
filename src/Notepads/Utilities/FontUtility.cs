namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using Windows.Foundation;
    using Windows.Globalization;
    using Windows.UI.Text;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;
    using Microsoft.AppCenter.Analytics;

    public static class FontUtility
    {
        /// <summary>
        /// The collection of symbol fonts that need to be skipped from the available fonts, as they don't produce readable text
        /// </summary>
        private static readonly IReadOnlyCollection<string> SymbolFonts = new HashSet<string>(new[]
        {
            "Segoe MDL2 Assets",
            "Webdings",
            "Wingdings",
            "HoloLens MDL2 Assets",
            "Bookshelf Symbol 7",
            "MT Extra",
            "MS Outlook",
            "MS Reference Specialty",
            "Wingdings 2",
            "Wingdings 3",
            "Marlett"
        });

        /// <summary>
        /// The fallback collection of fonts available in all Windows 10 versions in case GetSystemFontFamilies API failed
        /// https://docs.microsoft.com/en-us/typography/fonts/windows_10_font_list
        /// </summary>
        private static readonly IReadOnlyCollection<string> DefaultFonts = new HashSet<string>(new[]
        {
            "Arial",
            "Arial Black",
            "Calibri",
            "Cambria",
            "Cambria Math",
            "Comic Sans MS",
            "Consolas",
            "Constantia",
            "Courier New",
            "Ebrima",
            "Franklin Gothic Medium",
            "Gabriola",
            "Gadugi",
            "Georgia",
            "Impact",
            "Javanese Text",
            "Leelawadee UI",
            "Lucida Console",
            "Lucida Sans Unicode",
            "Malgun Gothic",
            "Marlett",
            "Microsoft Himalaya",
            "Microsoft JhengHei",
            "Microsoft New Tai Lue",
            "Microsoft PhagsPa",
            "Microsoft Sans Serif",
            "Microsoft Tai Le",
            "Microsoft YaHei",
            "Microsoft Yi Baiti",
            "MingLiU-ExtB",
            "Mongolian Baiti",
            "MS Gothic",
            "MV Boli",
            "Myanmar Text",
            "Nirmala UI",
            "Palatino Linotype",
            "Segoe Print",
            "Segoe Script",
            "Segoe UI",
            "SimSun",
            "Sitka",
            "Sylfaen",
            "Tahoma",
            "Times New Roman",
            "Trebuchet MS",
            "Verdana",
            "Yu Gothic"
        });

        public static readonly int[] PredefinedFontSizes =
        {
            8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 22, 24, 26, 28, 36, 48, 72
        };

        public static readonly Dictionary<string, FontStyle> PredefinedFontStylesMap = new Dictionary<string, FontStyle>()
        {
            {nameof(FontStyle.Normal),  FontStyle.Normal},
            {nameof(FontStyle.Italic),  FontStyle.Italic},
            {nameof(FontStyle.Oblique), FontStyle.Oblique}
        };

        public static readonly Dictionary<string, ushort> PredefinedFontWeightsMap = new Dictionary<string, ushort>()
        {
            {nameof(FontWeights.Normal),     FontWeights.Normal.Weight},
            {nameof(FontWeights.Thin),       FontWeights.Thin.Weight},
            {nameof(FontWeights.ExtraLight), FontWeights.ExtraLight.Weight},
            {nameof(FontWeights.Light),      FontWeights.Light.Weight},
            {nameof(FontWeights.SemiLight),  FontWeights.SemiLight.Weight},
            {nameof(FontWeights.Medium),     FontWeights.Medium.Weight},
            {nameof(FontWeights.SemiBold),   FontWeights.SemiBold.Weight},
            {nameof(FontWeights.Bold),       FontWeights.Bold.Weight},
            {nameof(FontWeights.ExtraBold),  FontWeights.ExtraBold.Weight},
            {nameof(FontWeights.Black),      FontWeights.Black.Weight},
            {nameof(FontWeights.ExtraBlack), FontWeights.ExtraBlack.Weight}
        };

        public static bool IsMonospacedFont(FontFamily font)
        {
            var tb1 = new TextBlock { Text = "(!aiZ%#BIm,. ~`", FontFamily = font };
            tb1.Measure(new Size(Double.PositiveInfinity, Double.PositiveInfinity));
            var tb2 = new TextBlock { Text = "...............", FontFamily = font };
            tb2.Measure(new Size(Double.PositiveInfinity, Double.PositiveInfinity));

            var off = Math.Abs(tb1.DesiredSize.Width - tb2.DesiredSize.Width);
            return off < 0.01;
        }

        public static Size GetTextSize(FontFamily font, double fontSize, string text)
        {
            var tb = new TextBlock { Text = text, FontFamily = font, FontSize = fontSize };
            tb.Measure(new Size(Double.PositiveInfinity, Double.PositiveInfinity));
            return tb.DesiredSize;
        }

        public static string[] GetSystemFontFamilies()
        {
            try
            {
                var systemFonts = Microsoft.Graphics.Canvas.Text.CanvasTextFormat.GetSystemFontFamilies(ApplicationLanguages.Languages);
                return systemFonts.Where(font => !SymbolFonts.Contains(font)).OrderBy(font => font).ToArray();
            }
            catch (Exception ex)
            {
                Analytics.TrackEvent("FailedToGetSystemFontFamilies", new Dictionary<string, string>()
                {
                    { "Exception", ex.ToString() }
                });
                return DefaultFonts.Where(font => !SymbolFonts.Contains(font)).OrderBy(font => font).ToArray();
            }
        }
    }
}