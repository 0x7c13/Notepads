// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Collections.Generic;

namespace Notepads.Features.WebSearch;

public enum SearchEngine
{
    Bing,
    Google,
    DuckDuckGo,
    Custom
}

public static class SearchUrls
{
    private static readonly Dictionary<SearchEngine, string> SearchEngineUrlDictionary = new()
    {
        {SearchEngine.Bing, "https://www.bing.com/search?q={0}&form=NPCTXT"},
        {SearchEngine.Google, "https://www.google.com/search?q={0}&oq={0}"},
        {SearchEngine.DuckDuckGo, "https://duckduckgo.com/?q={0}&ia=web"},
        {SearchEngine.Custom, ""}
    };

    public static string GetSearchUrlBySearchEngine(SearchEngine searchEngine, string customTemplate)
    {
        return searchEngine != SearchEngine.Custom ? SearchEngineUrlDictionary[searchEngine] : customTemplate;
    }
}
