namespace Notepads.Utilities
{
    using System.Collections.Generic;
    using System.Globalization;
    using System.Linq;
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
                Name = string.IsNullOrEmpty(value) ? "System Default" : new CultureInfo(value).NativeName;
            }
        }

        public string Name { get; private set; }
    }

    public static class LanguageUtility
    {
        public static readonly string CurrentLanguageID = ApplicationLanguages.PrimaryLanguageOverride;

        public static IReadOnlyCollection<LanguageItem> GetSupportedLanguageItems()
        {
            var supportedLanguageList = new List<LanguageItem>() { new LanguageItem() { ID = string.Empty } };
            supportedLanguageList.AddRange(ApplicationLanguages.ManifestLanguages
                .Select(languageId => new LanguageItem() { ID = languageId }));
            return supportedLanguageList;
        }
    }
}