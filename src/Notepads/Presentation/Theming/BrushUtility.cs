// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Resources;
using Windows.UI;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Theming;

public static class BrushUtility
{
    private static readonly SemaphoreSlim SemaphoreSlim = new(1);

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
