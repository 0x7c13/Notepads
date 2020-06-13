namespace Notepads.Utilities
{
    using System.Collections.Generic;
    using System.Globalization;
    using System.Linq;
    using Windows.ApplicationModel.Resources;
    using Windows.Globalization;

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
                    : new CultureInfo(value).NativeName;
            }
        }

        public string Name { get; private set; }
    }

    public static class LanguageUtility
    {
        private static readonly IReadOnlyList<string> _manifestLanguages = new List<string>()
        {
            "en-US",
            "bg",
            "cs", 
            "de-CH",
            "de-DE",
            "es-ES", 
            "fi",
            "fr-FR", 
            "hi", 
            "hr-HR", 
            "hu",
            "it-IT",
            "ja",
            "ka",
            "ko",
            "or",
            "pl",
            "pt-BR",
            "ru",
            "tr",
            "uk",
            "zh-Hans-CN",
            "zh-Hant-TW"
        };

        public static readonly string CurrentLanguageID = ApplicationLanguages.PrimaryLanguageOverride;

        public static IReadOnlyCollection<LanguageItem> GetSupportedLanguageItems()
        {
            var supportedLanguageList = new List<LanguageItem>() { new LanguageItem() { ID = string.Empty } };
            supportedLanguageList.AddRange(_manifestLanguages.Select(languageId => new LanguageItem() { ID = languageId }));
            return supportedLanguageList;
        }
    }
}