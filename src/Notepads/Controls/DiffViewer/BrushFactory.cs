namespace Notepads.Controls.DiffViewer
{
    using System.Collections.Generic;
    using Windows.UI;
    using Windows.UI.Xaml.Media;

    public class BrushFactory
    {
        public Dictionary<Color, SolidColorBrush> Brushes = new Dictionary<Color, SolidColorBrush>();

        public SolidColorBrush GetOrCreateSolidColorBrush(Color color)
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