namespace Notepads.Utilities
{
    using Notepads.Services;

    public enum SearchEngine
    {
        Bing,
        Google,
        DuckDuckGo,
        Custom
    }

    class SearchEngineUtility
    {
        private static string[] SearchUrls = new string[]
        {
            "https://www.bing.com/search?q={0}&form=NPCTXT",
            "https://www.google.com/search?q={0}&oq={0}",
            "https://duckduckgo.com/?q={0}&ia=web"
        };

        public static string GetSearchUrlFromSearchEngine(SearchEngine searchEngine)
        {
            switch(searchEngine)
            {
                case SearchEngine.Bing:
                    return SearchUrls[0];
                case SearchEngine.Google:
                    return SearchUrls[1];
                case SearchEngine.DuckDuckGo:
                    return SearchUrls[2];
                case SearchEngine.Custom:
                    return EditorSettingsService.EditorCustomMadeSearchUrl;
                default:
                    return SearchUrls[0];
            }
        }
    }
}
