namespace Notepads.Controls.GoTo
{
    using System;

    public class GoToEventArgs : EventArgs
    {
        public GoToEventArgs(string searchLine)
        {
            SearchLine = searchLine;
        }

        public string SearchLine { get; }
    }
}