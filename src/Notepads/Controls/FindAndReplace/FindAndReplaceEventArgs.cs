namespace Notepads.Controls.FindAndReplace
{
    using System;

    public enum FindAndReplaceMode
    {
        FindOnly,
        Replace,
        ReplaceAll
    }

    public class FindAndReplaceEventArgs : EventArgs
    {
        public FindAndReplaceEventArgs(string searchText, string replaceText, bool matchCase, bool matchWholeWord, FindAndReplaceMode findAndReplaceMode)
        {
            SearchText = searchText;
            MatchCase = matchCase;
            MatchWholeWord = matchWholeWord;
            ReplaceText = replaceText;
            FindAndReplaceMode = findAndReplaceMode;
        }

        public string SearchText { get; }

        public string ReplaceText { get; }

        public bool MatchCase { get; }

        public bool MatchWholeWord { get; }

        public FindAndReplaceMode FindAndReplaceMode { get; }
    }
}