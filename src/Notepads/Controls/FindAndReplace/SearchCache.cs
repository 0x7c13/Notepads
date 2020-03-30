namespace Notepads.Controls.FindAndReplace
{
    public class SearchCache
    {
        public SearchCache(string searchText, bool matchCase = false, bool matchWholeWord = false, bool useRegex = false)
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
