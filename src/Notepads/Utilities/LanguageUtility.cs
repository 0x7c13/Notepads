namespace Notepads.Utilities
{
    using System.Collections.Generic;
    using System.Globalization;
    using Windows.Globalization;
    using Windows.UI.Xaml;

    public class LanguageItem
    {
        private string _id;
        public string ID
        {
            get => _id;
            set
            {
                _id = value;
                _name = string.IsNullOrEmpty(value) ? "System Default" : new CultureInfo(value).NativeName;
            }
        }

        private string _name;
        public string Name { get => _name; }
    }

    public static class LanguageUtility
    {
        public static readonly string CurrentLanguageID = ApplicationLanguages.PrimaryLanguageOverride;
        public static Visibility RestrtPromptVisibility = Visibility.Collapsed;

        public static IReadOnlyCollection<LanguageItem> GetSupportedLanguageItems()
        {
            var supportedLanguageList = new List<LanguageItem>() { new LanguageItem() { ID = string.Empty } };
            foreach (var languageId in ApplicationLanguages.ManifestLanguages)
            {
                supportedLanguageList.Add(new LanguageItem() { ID = languageId });
            }
            return supportedLanguageList;
        }
    }
}
