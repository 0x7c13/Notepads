namespace Notepads.Controls.DiffViewer
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Threading;
    using System.Threading.Tasks;
    using Notepads.Commands;
    using Notepads.Extensions;
    using Notepads.Services;
    using Windows.System;
    using Windows.UI;
    using Windows.UI.Composition;
    using Windows.UI.Core;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    public sealed partial class SideBySideDiffComparer : UserControl, ISideBySideDiffViewer
    {
        public event EventHandler OnCloseEvent;

        private readonly RichTextBlockDiffRenderer _diffRenderer;

        private readonly ICommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;
        private readonly ICommandHandler<PointerRoutedEventArgs> _mouseCommandHandler;

        private CancellationTokenSource _cancellationTokenSource;

        private readonly ScrollViewerSynchronizer _scrollSynchronizer;

        public SideBySideDiffComparer()
        {
            InitializeComponent();

            _scrollSynchronizer = new ScrollViewerSynchronizer(new List<ScrollViewer> { LeftScroller, RightScroller });

            _diffRenderer = new RichTextBlockDiffRenderer();

            _mouseCommandHandler = GetMouseCommandHandler();

            LeftTextBlock.SelectionHighlightColor = new SolidColorBrush(ThemeSettingsService.AppAccentColor);
            RightTextBlock.SelectionHighlightColor = new SolidColorBrush(ThemeSettingsService.AppAccentColor);

            LeftTextBlockBorder.PointerWheelChanged += LeftTextBlockBorder_PointerWheelChanged;
            RightTextBlockBorder.PointerWheelChanged += RightTextBlockBorder_PointerWheelChanged;

            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;

            LayoutRoot.KeyDown += OnKeyDown;
            KeyDown += OnKeyDown;
            LeftTextBlock.KeyDown += OnKeyDown;
            RightTextBlock.KeyDown += OnKeyDown;
            Loaded += SideBySideDiffComparer_Loaded;
        }

        private void SideBySideDiffComparer_Loaded(object sender, RoutedEventArgs e)
        {
            Focus();
        }

        public void Dispose()
        {
            StopRenderingAndClearCache();

            ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;

            LayoutRoot.KeyDown -= OnKeyDown;
            KeyDown -= OnKeyDown;
            LeftTextBlock.KeyDown -= OnKeyDown;
            RightTextBlock.KeyDown -= OnKeyDown;
            Loaded -= SideBySideDiffComparer_Loaded;

            LeftTextBlockBorder.PointerWheelChanged -= LeftTextBlockBorder_PointerWheelChanged;
            RightTextBlockBorder.PointerWheelChanged -= RightTextBlockBorder_PointerWheelChanged;

            _scrollSynchronizer.Dispose();
        }

        private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                LeftTextBlock.SelectionHighlightColor = new SolidColorBrush(color);
                RightTextBlock.SelectionHighlightColor = new SolidColorBrush(color);
            });
        }

        private ICommandHandler<PointerRoutedEventArgs> GetMouseCommandHandler()
        {
            return new MouseCommandHandler(new List<IMouseCommand<PointerRoutedEventArgs>>()
            {
                new MouseCommand<PointerRoutedEventArgs>(false, false, false, ChangeVerticalScrollingBasedOnMouseInput),
                new MouseCommand<PointerRoutedEventArgs>(true, false, true, false, false, false, ChangeHorizontalScrollingBasedOnMouseInput),
                new MouseCommand<PointerRoutedEventArgs>(false, false, true, false, false, false, ChangeHorizontalScrollingBasedOnMouseInput),
                new MouseCommand<PointerRoutedEventArgs>(false, true, false, ChangeHorizontalScrollingBasedOnMouseInput)
            }, this);
        }

        private void ChangeVerticalScrollingBasedOnMouseInput(PointerRoutedEventArgs args)
        {
            var mouseWheelDelta = args.GetCurrentPoint(this).Properties.MouseWheelDelta;
            RightScroller.ChangeView(null, RightScroller.VerticalOffset + (-1 * mouseWheelDelta), null, false);
        }

        // Ctrl + Shift + Wheel -> horizontal scrolling
        private void ChangeHorizontalScrollingBasedOnMouseInput(PointerRoutedEventArgs args)
        {
            var mouseWheelDelta = args.GetCurrentPoint(this).Properties.MouseWheelDelta;
            RightScroller.ChangeView(RightScroller.HorizontalOffset + (-1 * mouseWheelDelta), null, null, false);
        }

        private void OnKeyDown(object sender, Windows.UI.Xaml.Input.KeyRoutedEventArgs args)
        {
            var result = _keyboardCommandHandler.Handle(args);
            if (result.ShouldHandle)
            {
                args.Handled = true;
            }
        }

        public void Focus()
        {
            RightTextBlock.Focus(FocusState.Programmatic);
        }

        public void StopRenderingAndClearCache()
        {
            if (_cancellationTokenSource != null && !_cancellationTokenSource.IsCancellationRequested)
            {
                _cancellationTokenSource.Cancel();
            }

            LeftTextBlock.TextHighlighters.Clear();
            LeftTextBlock.Blocks.Clear();
            RightTextBlock.TextHighlighters.Clear();
            RightTextBlock.Blocks.Clear();
        }

        public void RenderDiff(string left, string right, ElementTheme theme, string leftHeader = null, string rightHeader = null)
        {
            StopRenderingAndClearCache();

            var foregroundBrush = (theme == ElementTheme.Dark)
                ? new SolidColorBrush(Colors.White)
                : new SolidColorBrush(Colors.Black);

            var diffContext = _diffRenderer.GenerateDiffViewData(left, right, foregroundBrush);
            var leftContext = diffContext.Item1;
            var rightContext = diffContext.Item2;
            var leftHighlighters = leftContext.GetTextHighlighters();
            var rightHighlighters = rightContext.GetTextHighlighters();

            CancellationTokenSource cancellationTokenSource = new CancellationTokenSource();

            LoadContextBlocks(leftContext, rightContext, cancellationTokenSource, leftHeader, rightHeader);

            LoadHighlighters(leftContext, rightContext, cancellationTokenSource, leftHighlighters, rightHighlighters);

            _cancellationTokenSource = cancellationTokenSource;
        }

        public void LoadContextBlocks(RichTextBlockDiffContext leftContext, RichTextBlockDiffContext rightContext, CancellationTokenSource cancellationTokenSource, string leftHeader, string rightHeader)
        {
            Task.Factory.StartNew(async () =>
            {
                var leftCount = leftContext.Blocks.Count;
                var rightCount = rightContext.Blocks.Count;

                var leftStartIndex = 0;
                var rightStartIndex = 0;
                var threshold = 1;

                while (true)
                {
                    Thread.Sleep(10);
                    if (leftStartIndex < leftCount)
                    {
                        var end = leftStartIndex + threshold;
                        if (end >= leftCount) end = leftCount;
                        var start = leftStartIndex;

                        await Dispatcher.RunAsync(CoreDispatcherPriority.Low, () =>
                        {
                            for (int x = start; x < end; x++)
                            {
                                if (cancellationTokenSource.IsCancellationRequested) return;
                                if (leftHeader != null) DiffViewer_Header_LeftTextTittle.Text = leftHeader;
                                LeftTextBlock.Blocks.Add(leftContext.Blocks[x]);
                            }
                        });
                    }

                    if (rightStartIndex < rightCount)
                    {
                        var end = rightStartIndex + threshold;
                        if (end >= rightCount) end = rightCount;
                        var start = rightStartIndex;

                        await Dispatcher.RunAsync(CoreDispatcherPriority.Low, () =>
                        {
                            for (int x = start; x < end; x++)
                            {
                                if (cancellationTokenSource.IsCancellationRequested) return;
                                if (rightHeader != null) DiffViewer_Header_RightTextTittle.Text = rightHeader;
                                RightTextBlock.Blocks.Add(rightContext.Blocks[x]);
                            }
                        });
                    }

                    leftStartIndex += threshold;
                    rightStartIndex += threshold;
                    threshold *= 5;

                    if (leftStartIndex >= leftCount && rightStartIndex >= rightCount)
                    {
                        break;
                    }
                }
            }, cancellationTokenSource.Token);
        }

        public void LoadHighlighters(RichTextBlockDiffContext leftContext, RichTextBlockDiffContext rightContext, CancellationTokenSource cancellationTokenSource, IList<Windows.UI.Xaml.Documents.TextHighlighter> leftHighlighters, IList<Windows.UI.Xaml.Documents.TextHighlighter> rightHighlighters)
        {
            Task.Factory.StartNew(async () =>
            {
                var leftCount = leftHighlighters.Count;
                var rightCount = rightHighlighters.Count;

                var leftStartIndex = 0;
                var rightStartIndex = 0;
                var threshold = 5;

                while (true)
                {
                    Thread.Sleep(10);
                    if (leftStartIndex < leftCount)
                    {
                        var end = leftStartIndex + threshold;
                        if (end >= leftCount) end = leftCount;
                        var start = leftStartIndex;

                        await Dispatcher.RunAsync(CoreDispatcherPriority.Low, () =>
                        {
                            for (int x = start; x < end; x++)
                            {
                                if (cancellationTokenSource.IsCancellationRequested) return;
                                LeftTextBlock.TextHighlighters.Add(leftHighlighters[x]);
                            }
                        });
                    }

                    if (rightStartIndex < rightCount)
                    {
                        var end = rightStartIndex + threshold;
                        if (end >= rightCount) end = rightCount;
                        var start = rightStartIndex;

                        await Dispatcher.RunAsync(CoreDispatcherPriority.Low, () =>
                        {
                            for (int x = start; x < end; x++)
                            {
                                if (cancellationTokenSource.IsCancellationRequested) return;
                                RightTextBlock.TextHighlighters.Add(rightHighlighters[x]);
                            }
                        });
                    }

                    leftStartIndex += threshold;
                    rightStartIndex += threshold;
                    threshold *= 5;

                    if (leftStartIndex >= leftCount && rightStartIndex >= rightCount)
                    {
                        break;
                    }
                }
            }, cancellationTokenSource.Token);
        }


        private void LeftTextBlockBorder_PointerWheelChanged(object sender, PointerRoutedEventArgs e)
        {
            _mouseCommandHandler.Handle(e);

            // Always handle it so that left ScrollViewer won't pick up the event
            e.Handled = true;
        }

        private void RightTextBlockBorder_PointerWheelChanged(object sender, PointerRoutedEventArgs e)
        {
            _mouseCommandHandler.Handle(e);

            // Always handle it so that right ScrollViewer won't pick up the event
            e.Handled = true;
        }

        public void RenderDiff(string left, string right, ElementTheme theme)
        {
            throw new NotImplementedException();
        }
    }
}