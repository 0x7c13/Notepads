
namespace Notepads.Extensions.DiffViewer
{
    using System;
    using System.Collections.Generic;
    using System.Threading;
    using Notepads.Controls.TextEditor;
    using Windows.System;
    using Windows.UI.Core;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;

    public sealed partial class DiffViewer : Page, IContentPreviewExtension
    {
        private readonly TextBoxDiffRenderer diffRenderer;
        private readonly ScrollViewerSynchronizer scrollSynchronizer;

        private TextEditor _editor;

        public DiffViewer()
        {
            InitializeComponent();
            scrollSynchronizer = new ScrollViewerSynchronizer(new List<ScrollViewer> { LeftScroller, RightScroller });
            diffRenderer = new TextBoxDiffRenderer(LeftBox, RightBox);
        }

        public void Bind(TextEditor editor)
        {
            _editor = editor;
            diffRenderer.GenerateDiffView(editor.OriginalContent, editor.TextEditorCore.GetText());
        }


        private bool _isExtensionEnabled;

        public bool IsExtensionEnabled
        {
            get => _isExtensionEnabled;
            set
            {
                if (value)
                {
                    diffRenderer.GenerateDiffView(_editor.OriginalContent, _editor.TextEditorCore.GetText());
                }
                _isExtensionEnabled = value;
            }
        }
    }
}
