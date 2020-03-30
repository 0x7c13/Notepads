namespace Notepads.Controls.FindAndReplace
{
    using System;

    public enum FindAndReplaceMode
    {
        FindOnly,
        Replace,
        ReplaceAll
    }

    public enum SearchDirection
    {
        Previous,
        Next
    }

    public class FindAndReplaceEventArgs : EventArgs
    {
        public FindAndReplaceEventArgs(string searchText, string replaceText, bool matchCase, bool matchWholeWord, bool useRegex, FindAndReplaceMode findAndReplaceMode, SearchDirection searchDirection = SearchDirection.Next)
        {
            SearchText = searchText;
            MatchCase = matchCase;
            MatchWholeWord = matchWholeWord;
            ReplaceText = replaceText;
            FindAndReplaceMode = findAndReplaceMode;
            UseRegex = useRegex;
            SearchDirection = searchDirection;
        }

        public string SearchText { get; }

        public string ReplaceText { get; }

        public bool MatchCase { get; }

        public bool MatchWholeWord { get; }

        public bool UseRegex { get; }

        public FindAndReplaceMode FindAndReplaceMode { get; }

        public SearchDirection SearchDirection { get; }
    }
}