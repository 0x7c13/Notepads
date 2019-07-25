
namespace Notepads.Extensions.DiffViewer
{
    using Notepads.Commands;
    using Notepads.Services;
    using System;
    using System.Collections.Generic;
    using Windows.System;
    using Windows.UI;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    public sealed partial class SideBySideDiffViewer : UserControl, ISideBySideDiffViewer
    {
        private readonly RichTextBlockDiffRenderer _diffRenderer;
        private readonly ScrollViewerSynchronizer _scrollSynchronizer;
        private readonly IKeyboardCommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;

        public event EventHandler OnCloseEvent;

        public SideBySideDiffViewer()
        {
            InitializeComponent();
            _scrollSynchronizer = new ScrollViewerSynchronizer(new List<ScrollViewer> { LeftScroller, RightScroller });
            _diffRenderer = new RichTextBlockDiffRenderer();
            _keyboardCommandHandler = GetKeyboardCommandHandler();

            LeftBox.SelectionHighlightColor = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            RightBox.SelectionHighlightColor = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;

            ThemeSettingsService.OnAccentColorChanged += (sender, color) =>
            {
                LeftBox.SelectionHighlightColor =
                    Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
                RightBox.SelectionHighlightColor =
                    Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            };

            LayoutRoot.KeyDown += OnKeyDown;
            KeyDown += OnKeyDown;
            LeftBox.KeyDown += OnKeyDown;
            RightBox.KeyDown += OnKeyDown;
        }

        private KeyboardCommandHandler GetKeyboardCommandHandler()
        {
            return new KeyboardCommandHandler(new List<IKeyboardCommand<KeyRoutedEventArgs>>
            {
                new KeyboardShortcut<KeyRoutedEventArgs>(false, false, false, VirtualKey.Escape, (args) =>
                {
                    OnCloseEvent?.Invoke(this, EventArgs.Empty);
                }),
                new KeyboardShortcut<KeyRoutedEventArgs>(false, true, false, VirtualKey.D, (args) =>
                {
                    OnCloseEvent?.Invoke(this, EventArgs.Empty);
                }),
            });
        }

        private void OnKeyDown(object sender, Windows.UI.Xaml.Input.KeyRoutedEventArgs args)
        {
            _keyboardCommandHandler.Handle(args);
        }

        public void Focus()
        {
            RightBox.Focus(FocusState.Programmatic);
        }

        public void RenderDiff(string left, string right)
        {
            var foregroundBrush = (ThemeSettingsService.ThemeMode == ElementTheme.Dark)
                ? new SolidColorBrush(Colors.White)
                : new SolidColorBrush(Colors.Black);

            var diffData = _diffRenderer.GenerateDiffViewData(left, right, foregroundBrush);
            var leftData = diffData.Item1;
            var rightData = diffData.Item2;

            LeftBox.Blocks.Clear();
            LeftBox.TextHighlighters.Clear();
            RightBox.Blocks.Clear();
            RightBox.TextHighlighters.Clear();

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
