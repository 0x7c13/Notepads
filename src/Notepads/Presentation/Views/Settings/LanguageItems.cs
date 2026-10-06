// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Notepads.Features.Preferences;
using Windows.ApplicationModel.Resources;
using Windows.Globalization;

namespace Notepads.Presentation.Views.Settings;

public class LanguageItem
{
    private string _id;

    public string ID
    {
        get => _id;
        set
        {
            _id = value;
            Name = string.IsNullOrEmpty(value)
                ? ResourceLoader.GetForCurrentView().GetString("/Settings/AdvancedPage_LanguagePreferenceSettings_SystemDefaultText")
                : LanguageOptions.GetNativeName(value);
        }
    }

    public string Name { get; private set; }
}

public static class LanguageUtility
{
    public static readonly string CurrentLanguageID = LanguageOptions.InitialLanguageId;

    public static IReadOnlyCollection<LanguageItem> GetSupportedLanguageItems()
    {
        var supportedLanguageList = new List<LanguageItem>() { new() { ID = string.Empty } };
        supportedLanguageList.AddRange(LanguageOptions.SupportedLanguageIds
            .Select(languageId => new LanguageItem() { ID = languageId }));
        return supportedLanguageList;
    }
}
