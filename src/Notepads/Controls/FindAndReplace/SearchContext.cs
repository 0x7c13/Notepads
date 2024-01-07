// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.FindAndReplace
{
    public sealed class SearchContext
    {
        public SearchContext(
            string searchText,
            bool matchCase = false,
            bool matchWholeWord = false,
            bool useRegex = false)
        {
            SearchText = searchText;
            MatchCase = matchCase;
            MatchWholeWord = matchWholeWord;
            UseRegex = useRegex;
        }

        public string SearchText { get; }

        public bool MatchCase { get; }

        public bool MatchWholeWord { get; }

        public bool UseRegex { get; }
    }
}
