// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Infrastructure.Settings;

namespace Notepads.Features.Preferences;

public enum ThemeChoice { Default, Light, Dark }

internal static class AppearancePreferences
{
    public static bool? UseWindowsTheme
    {
        get => ApplicationSettingsStore.Read(SettingsKey.UseWindowsThemeBool) as bool?;
        set => ApplicationSettingsStore.Write(SettingsKey.UseWindowsThemeBool, value);
    }

    public static bool? UseWindowsAccentColor
    {
        get => ApplicationSettingsStore.Read(SettingsKey.UseWindowsAccentColorBool) as bool?;
        set => ApplicationSettingsStore.Write(SettingsKey.UseWindowsAccentColorBool, value);
    }

    public static double? BackgroundTintOpacity
    {
        get => ApplicationSettingsStore.Read(SettingsKey.AppBackgroundTintOpacityDouble) as double?;
        set => ApplicationSettingsStore.Write(SettingsKey.AppBackgroundTintOpacityDouble, value);
    }

    public static string AccentColorHex
    {
        get => ApplicationSettingsStore.Read(SettingsKey.AppAccentColorHexStr) as string;
        set => ApplicationSettingsStore.Write(SettingsKey.AppAccentColorHexStr, value);
    }

    public static string CustomAccentColorHex
    {
        get => ApplicationSettingsStore.Read(SettingsKey.CustomAccentColorHexStr) as string;
        set => ApplicationSettingsStore.Write(SettingsKey.CustomAccentColorHexStr, value);
    }

    public static ThemeChoice? RequestedTheme
    {
        get => Enum.TryParse(ApplicationSettingsStore.Read(SettingsKey.RequestedThemeStr) as string, out ThemeChoice choice)
            ? choice : (ThemeChoice?)null;
        set => ApplicationSettingsStore.Write(SettingsKey.RequestedThemeStr, value?.ToString());
    }
}
