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
        public FindAndReplaceEventArgs(
            SearchContext searchContext,
            string replaceText,
            FindAndReplaceMode findAndReplaceMode,
            SearchDirection searchDirection = SearchDirection.Next)
        {
            SearchContext = searchContext;
            ReplaceText = replaceText;
            FindAndReplaceMode = findAndReplaceMode;
            SearchDirection = searchDirection;
        }

        public SearchContext SearchContext { get; }

        public string ReplaceText { get; }

        public FindAndReplaceMode FindAndReplaceMode { get; }

        public SearchDirection SearchDirection { get; }
    }
}