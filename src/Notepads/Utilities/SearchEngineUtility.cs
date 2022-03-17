namespace Notepads.Utilities
{
    using Notepads.Services;
    using System.Collections.Generic;

    public enum SearchEngine
    {
        Bing,
        Google,
        DuckDuckGo,
		Brave,
		Qwant,
        Custom
    }

    public static class SearchEngineUtility
    {
        private static readonly Dictionary<SearchEngine, string> SearchEngineUrlDictionary = new Dictionary<SearchEngine, string>
        {
            {SearchEngine.Bing, "https://www.bing.com/search?q={0}&form=NPCTXT"},
            {SearchEngine.Google, "https://www.google.com/search?q={0}&oq={0}"},
            {SearchEngine.DuckDuckGo, "https://duckduckgo.com/?q={0}&ia=web"},
            {SearchEngine.Brave, "https://search.brave.com/search?q={0}&source=web"},
            {SearchEngine.Qwant, "https://www.qwant.com/?q={0}&t=web"},
            {SearchEngine.Custom, ""}
        };

        public static string GetSearchUrlBySearchEngine(SearchEngine searchEngine)
        {
            return searchEngine != SearchEngine.Custom ? SearchEngineUrlDictionary[searchEngine] : AppSettingsService.EditorCustomMadeSearchUrl;
        }
    }
}
