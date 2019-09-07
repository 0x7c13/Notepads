namespace Notepads.Controls.TextEditor
{
    public class TextEditorStateMetaData
    {
        public string FileNamePlaceholder { get; set; }

        public string LastSavedEncoding { get; set; }

        public string LastSavedLineEnding { get; set; }

        public long DateModifiedFileTime { get; set; }

        public string RequestedLineEnding { get; set; }

        public string RequestedEncoding { get; set; }

        public bool HasEditingFile { get; set; }

        public bool IsModified { get; set; }

        public int SelectionStartPosition { get; set; }

        public int SelectionEndPosition { get; set; }

        public bool WrapWord { get; set; }

        public double FontZoomFactor { get; set; }

        public double ScrollViewerHorizontalOffset { get; set; }

        public double ScrollViewerVerticalOffset { get; set; }

        public bool IsContentPreviewPanelOpened { get; set; }

        public bool IsInDiffPreviewMode { get; set; }
    }
}