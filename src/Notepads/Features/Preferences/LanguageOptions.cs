// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Collections.Generic;
using System.Globalization;
using Notepads.Infrastructure.Localization;

namespace Notepads.Features.Preferences;

internal static class LanguageOptions
{
    public static string InitialLanguageId { get; } = SystemLanguages.PrimaryLanguageOverride;
    public static IReadOnlyList<string> SupportedLanguageIds => SystemLanguages.ManifestLanguages;
    public static string GetNativeName(string languageId) => new CultureInfo(languageId).NativeName;
}
