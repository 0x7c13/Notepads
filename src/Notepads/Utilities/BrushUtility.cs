namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using System.Threading;
    using System.Threading.Tasks;
    using Windows.UI;
    using Windows.UI.Xaml.Media;
    using Microsoft.AppCenter.Analytics;
    using Notepads.Brushes;
    using Notepads.Extensions;

    public static class BrushUtility
    {
        private static readonly SemaphoreSlim SemaphoreSlim = new SemaphoreSlim(1);

        public static async Task<Brush> GetHostBackdropAcrylicBrush(Color color, float tintOpacity)
        {
            await SemaphoreSlim.WaitAsync();
            try
            {
                return new HostBackdropAcrylicBrush()
                {
                    FallbackColor = color,
                    LuminosityColor = color,
                    TintOpacity = tintOpacity,
                    TextureUri = "/Assets/noise_high.png".ToAppxUri(),
                };
            }
            catch (Exception ex)
            {
                Analytics.TrackEvent("FailedToCreateAcrylicBrush", new Dictionary<string, string>
                {
                    { "Exception", ex.ToString() },
                    { "Message", ex.Message },
                });
                return new SolidColorBrush(color);
            }
            finally
            {
                SemaphoreSlim.Release();
            }
        }
    }
}