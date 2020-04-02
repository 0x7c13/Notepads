namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using Windows.UI;
    using Windows.UI.Xaml.Media;
    using Microsoft.AppCenter.Analytics;

    public static class BrushUtility
    {
        public static Brush GetHostBackdropAcrylicBrush(Color color, float tintOpacity)
        {
            try
            {
                return new HostBackdropAcrylicBrush()
                {
                    FallbackColor = color,
                    LuminosityColor = color,
                    TintOpacity = tintOpacity,
                    TextureUri = ToAppxUri("/Assets/noise_low.png"),
                };
            }
            catch (Exception ex)
            {
                Analytics.TrackEvent("FailedToCreateAcrylicBrush", 
                    new Dictionary<string, string> {{ "Exception", ex.ToString() }});
                return new SolidColorBrush(color);
            }
        }    
        
        private static Uri ToAppxUri(string path)
        {
            string prefix = $"ms-appx://{(path.StartsWith('/') ? string.Empty : "/")}";
            return new Uri($"{prefix}{path}");
        }
    }
}