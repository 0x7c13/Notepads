
namespace Notepads.EventArgs
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

        public string SearchText { get; private set; }

        public string ReplaceText { get; private set; }

        public bool MatchCase { get; private set; }

        public bool MatchWholeWord { get; private set; }

        public FindAndReplaceMode FindAndReplaceMode  { get; private set; }
    }

}
