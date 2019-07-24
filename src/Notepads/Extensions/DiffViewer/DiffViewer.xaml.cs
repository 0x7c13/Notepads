
namespace Notepads.Extensions.DiffViewer
{
    using System.Collections.Generic;
    using Windows.UI.Xaml.Controls;

    public sealed partial class DiffViewer : Page, ISideBySideDiffViewer
    {
        private readonly TextBoxDiffRenderer diffRenderer;
        private readonly ScrollViewerSynchronizer scrollSynchronizer;

        public DiffViewer()
        {
            InitializeComponent();
            scrollSynchronizer = new ScrollViewerSynchronizer(new List<ScrollViewer> { LeftScroller, RightScroller });
            diffRenderer = new TextBoxDiffRenderer();
        }

        public void RenderDiff(string originalContent, string newContent)
        {
            var diffData = diffRenderer.GenerateDiffViewData(originalContent, newContent);
            var leftData = diffData.Item1;
            var rightData = diffData.Item2;

            foreach (var block in leftData.Blocks)
            {
                LeftBox.Blocks.Add(block);
            }

            foreach (var textHighlighter in leftData.TextHighlighters)
            {
                LeftBox.TextHighlighters.Add(textHighlighter);
            }

            foreach (var block in rightData.Blocks)
            {
                RightBox.Blocks.Add(block);
            }

            foreach (var textHighlighter in rightData.TextHighlighters)
            {
                RightBox.TextHighlighters.Add(textHighlighter);
            }
        }
    }
}
