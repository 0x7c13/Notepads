
namespace Notepads.Extensions.DiffViewer
{
    using System.Collections.Generic;
    using Windows.UI.Xaml.Controls;

    public sealed partial class SideBySideDiffViewer : Page, ISideBySideDiffViewer
    {
        private readonly TextBoxDiffRenderer diffRenderer;
        private readonly ScrollViewerSynchronizer scrollSynchronizer;

        public SideBySideDiffViewer()
        {
            InitializeComponent();
            scrollSynchronizer = new ScrollViewerSynchronizer(new List<ScrollViewer> { LeftScroller, RightScroller });
            diffRenderer = new TextBoxDiffRenderer();
        }

        public void RenderDiff(string left, string right)
        {
            var diffData = diffRenderer.GenerateDiffViewData(left, right);
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
