// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using Notepads.Infrastructure.Diagnostics;
using Windows.Globalization;

namespace Notepads.Infrastructure.Fonts;

internal static class SystemFontCatalog
{
    public static bool TryGetFamilyNames(out string[] fonts)
    {
        try
        {
            fonts = Microsoft.Graphics.Canvas.Text.CanvasTextFormat.GetSystemFontFamilies(ApplicationLanguages.Languages);
            return true;
        }
        catch (Exception ex)
        {
            AnalyticsService.TrackEvent("FailedToGetSystemFontFamilies", new Dictionary<string, string> { { "Exception", ex.ToString() } });
            fonts = null;
            return false;
        }
    }
}
