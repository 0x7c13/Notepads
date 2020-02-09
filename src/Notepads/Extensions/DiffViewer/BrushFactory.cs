namespace Notepads.Extensions.DiffViewer
{
    using System.Collections.Generic;
    using Windows.UI;
    using Windows.UI.Xaml.Media;

    public static class BrushFactory
    {
        public static Dictionary<Color, SolidColorBrush> Brushes = new Dictionary<Color, SolidColorBrush>();

        public static SolidColorBrush GetSolidColorBrush(Color color)
        {
            if (Brushes.ContainsKey(color))
            {
                return Brushes[color];
            }
            else
            {
                Brushes[color] = new SolidColorBrush(color);
                return Brushes[color];
            }
        }
    }
}