// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.DiffViewer
{
    using System.Collections.Generic;
    using Windows.UI;
    using Windows.UI.Xaml.Media;

    public sealed class BrushFactory
    {
        private readonly Dictionary<Color, SolidColorBrush> _brushes = new Dictionary<Color, SolidColorBrush>();

        public SolidColorBrush GetOrCreateSolidColorBrush(Color color)
        {
            if (_brushes.TryGetValue(color, out var brush))
            {
                return brush;
            }

            _brushes[color] = new SolidColorBrush(color);
            return _brushes[color];
        }
    }
}