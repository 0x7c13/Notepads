namespace Notepads.Utilities
{
    using Notepads.Services;
    using System.Collections.Generic;

    public enum SearchEngine
    {
        Bing,
        Google,
        DuckDuckGo,
        Custom
    }

    class SearchEngineUtility
    {
        private static Dictionary<SearchEngine, string> SearchEngineUrlDictionary = new Dictionary<SearchEngine, string>
        {
            {SearchEngine.Bing, "https://www.bing.com/search?q={0}&form=NPCTXT"},
            {SearchEngine.Google, "https://www.google.com/search?q={0}&oq={0}"},
            {SearchEngine.DuckDuckGo, "https://duckduckgo.com/?q={0}&ia=web"},
            {SearchEngine.Custom, ""}
        };

        public static string GetSearchUrlFromSearchEngine(SearchEngine searchEngine)
        {
            return searchEngine != SearchEngine.Custom ? SearchEngineUrlDictionary[searchEngine] : EditorSettingsService.EditorCustomMadeSearchUrl;
        }
    }
}
