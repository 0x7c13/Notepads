// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Windows.Foundation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Helpers;

public static class FontMeasurement
{
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

}
