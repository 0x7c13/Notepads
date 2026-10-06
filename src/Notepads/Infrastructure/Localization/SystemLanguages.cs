// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Collections.Generic;
using Windows.Globalization;

namespace Notepads.Infrastructure.Localization;

internal static class SystemLanguages
{
    public static IReadOnlyList<string> ManifestLanguages => ApplicationLanguages.ManifestLanguages;
    public static string PrimaryLanguageOverride => ApplicationLanguages.PrimaryLanguageOverride;
}
