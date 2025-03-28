// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using System.Threading;
    using System.Threading.Tasks;
    using Windows.UI;
    using Windows.UI.Xaml.Media;
    using Notepads.Brushes;
    using Notepads.Extensions;
    using Notepads.Services;

    public static class BrushUtility
    {
        private static readonly SemaphoreSlim SemaphoreSlim = new SemaphoreSlim(1);

        public static async Task<Brush> GetHostBackdropAcrylicBrushAsync(Color color, float tintOpacity)
        {
            await SemaphoreSlim.WaitAsync();
            try
            {
                return new HostBackdropAcrylicBrush()
                {
                    FallbackColor = color,
                    LuminosityColor = color,
                    TintOpacity = tintOpacity,
                    NoiseTextureUri = "/Assets/noise_high.png".ToAppxUri(),
                };
            }
            catch (Exception ex)
            {
                AnalyticsService.TrackEvent("FailedToCreateAcrylicBrush", new Dictionary<string, string>
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