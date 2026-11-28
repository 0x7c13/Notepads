// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Features.WebSearch;

namespace Notepads.UnitTests;

public sealed class SearchUrlTests
{
    [Theory]
    [InlineData(SearchEngine.Bing, "https://www.bing.com/search?q={0}&form=NPCTXT")]
    [InlineData(SearchEngine.Google, "https://www.google.com/search?q={0}&oq={0}")]
    [InlineData(SearchEngine.DuckDuckGo, "https://duckduckgo.com/?q={0}&ia=web")]
    public void BuiltInEngineKeepsItsQuerySlotAndIgnoresCustomSetting(SearchEngine engine, string expected) =>
        Assert.Equal(expected, SearchUrls.GetSearchUrlBySearchEngine(engine, "https://custom.invalid/?q={0}"));

    [Fact]
    public void CustomEnginePreservesTheUserTemplate()
    {
        const string template = "https://custom.invalid/?query={0}&scope=中文";
        Assert.Equal(template, SearchUrls.GetSearchUrlBySearchEngine(SearchEngine.Custom, template));
    }
}
