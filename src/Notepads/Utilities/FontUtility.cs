
namespace Notepads.Utilities
{
    using System;
    using Windows.Foundation;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;

    public static class FontUtility
    {
        public static bool IsMonospacedFont(FontFamily font)
        {
            TextBlock tb1 = new TextBlock { Text = "(!aiZ%#BIm,. ~`", FontFamily = font };
            tb1.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            TextBlock tb2 = new TextBlock { Text = "...............", FontFamily = font };
            tb2.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));

            double off = Math.Abs(tb1.DesiredSize.Width - tb2.DesiredSize.Width);
            return off < 0.01;
        }

        public static Size GetTextSize(FontFamily font, double fontSize, string text)
        {
            TextBlock tb = new TextBlock { Text = text, FontFamily = font, FontSize = fontSize };
            tb.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            return tb.DesiredSize;
        }
    }
}
