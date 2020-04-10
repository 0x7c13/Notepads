namespace Notepads.Extensions.DiffViewer
{
    using System;
    using System.Collections.Generic;
    using System.Threading;
    using System.Threading.Tasks;
    using Notepads.Commands;
    using Notepads.Services;
    using Notepads.Utilities;
    using Windows.System;
    using Windows.UI;
    using Windows.UI.Core;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    public sealed partial class SideBySideDiffViewer : UserControl, ISideBySideDiffViewer
    {
        private readonly RichTextBlockDiffRenderer _diffRenderer;

        private readonly ScrollViewerSynchronizer _scrollSynchronizer;

        private readonly ICommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;

        private CancellationTokenSource _cancellationTokenSource;

        public event EventHandler OnCloseEvent;

        public SideBySideDiffViewer()
        {
            InitializeComponent();
            _scrollSynchronizer = new ScrollViewerSynchronizer(new List<ScrollViewer> { LeftScroller, RightScroller });
            _diffRenderer = new RichTextBlockDiffRenderer();
            _keyboardCommandHandler = GetKeyboardCommandHandler();

            LeftBox.SelectionHighlightColor = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            RightBox.SelectionHighlightColor = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;

            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;

            DismissButton.Click += DismissButton_OnClick;
            LayoutRoot.KeyDown += OnKeyDown;
            KeyDown += OnKeyDown;
            LeftBox.KeyDown += OnKeyDown;
            RightBox.KeyDown += OnKeyDown;
            Loaded += SideBySideDiffViewer_Loaded;
        }

        public void Dispose()
        {
            StopRenderingAndClearCache();

            ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;

            DismissButton.Click -= DismissButton_OnClick;
            LayoutRoot.KeyDown -= OnKeyDown;
            KeyDown -= OnKeyDown;
            LeftBox.KeyDown -= OnKeyDown;
            RightBox.KeyDown -= OnKeyDown;
            Loaded -= SideBySideDiffViewer_Loaded;
        }

        private void SideBySideDiffViewer_Loaded(object sender, RoutedEventArgs e)
        {
            Focus();
        }

        private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                LeftBox.SelectionHighlightColor = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
                RightBox.SelectionHighlightColor = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            });
        }

        private KeyboardCommandHandler GetKeyboardCommandHandler()
        {
            return new KeyboardCommandHandler(new List<IKeyboardCommand<KeyRoutedEventArgs>>
            {
                new KeyboardCommand<KeyRoutedEventArgs>(VirtualKey.Escape, (args) =>
                {
                    DismissButton_OnClick(this, new RoutedEventArgs());
                }),
                new KeyboardCommand<KeyRoutedEventArgs>(false, true, false, VirtualKey.D, (args) =>
                {
                    DismissButton_OnClick(this, new RoutedEventArgs());
                }),
            });
        }

        private void OnKeyDown(object sender, Windows.UI.Xaml.Input.KeyRoutedEventArgs e)
        {
            var result = _keyboardCommandHandler.Handle(e);
            if (result.ShouldHandle)
            {
                e.Handled = true;
            }
        }

        public void Focus()
        {
            RightBox.Focus(FocusState.Programmatic);
        }

        public void StopRenderingAndClearCache()
        {
            if (_cancellationTokenSource != null && !_cancellationTokenSource.IsCancellationRequested)
            {
                _cancellationTokenSource.Cancel();
            }

            LeftBox.TextHighlighters.Clear();
            LeftBox.Blocks.Clear();
            RightBox.TextHighlighters.Clear();
            RightBox.Blocks.Clear();
        }

        public void RenderDiff(string left, string right)
        {
            StopRenderingAndClearCache();

            var foregroundBrush = (ThemeSettingsService.ThemeMode == ElementTheme.Dark)
                ? new SolidColorBrush(Colors.White)
                : new SolidColorBrush(Colors.Black);

            var diffContext = _diffRenderer.GenerateDiffViewData(left, right, foregroundBrush);
            var leftContext = diffContext.Item1;
            var rightContext = diffContext.Item2;
            var leftHighlighters = leftContext.GetTextHighlighters();
            var rightHighlighters = rightContext.GetTextHighlighters();

            CancellationTokenSource cancellationTokenSource = new CancellationTokenSource();

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
                                LeftBox.Blocks.Add(leftContext.Blocks[x]);
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
                                RightBox.Blocks.Add(rightContext.Blocks[x]);
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
                                LeftBox.TextHighlighters.Add(leftHighlighters[x]);
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
                                RightBox.TextHighlighters.Add(rightHighlighters[x]);
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

            _cancellationTokenSource = cancellationTokenSource;

            //Task.Factory.StartNew(async () =>
            //{
            //    var count = rightCount > leftCount ? rightCount : leftCount;

            //    for (int i = 0; i < count; i++)
            //    {
            //        if (i < leftCount)
            //        {
            //            var j = i;
            //            await Dispatcher.RunAsync(CoreDispatcherPriority.Low, () =>
            //            {
            //                Thread.Sleep(20);
            //                LeftBox.Blocks.Add(leftContext.Blocks[j]);
            //            });
            //        }
            //        if (i < rightCount)
            //        {
            //            var j = i;
            //            await Dispatcher.RunAsync(CoreDispatcherPriority.Low, () =>
            //            {
            //                Thread.Sleep(20);
            //                RightBox.Blocks.Add(rightContext.Blocks[j]);
            //            });
            //        }
            //    }
            //});

            //Task.Factory.StartNew(async () =>
            //{
            //    var leftCount = leftHighlighters.Count;
            //    var rightCount = rightHighlighters.Count;

            //    var count = rightCount > leftCount ? rightCount : leftCount;

            //    for (int i = 0; i < count; i++)
            //    {
            //        if (i < leftCount)
            //        {
            //            var j = i;
            //            await Dispatcher.RunAsync(CoreDispatcherPriority.Low,
            //                () => LeftBox.TextHighlighters.Add(leftHighlighters[j]));
            //        }
            //        if (i < rightCount)
            //        {
            //            var j = i;
            //            await Dispatcher.RunAsync(CoreDispatcherPriority.Low,
            //                () => RightBox.TextHighlighters.Add(rightHighlighters[j]));
            //        }
            //    }
            //});
        }

        private void DismissButton_OnClick(object sender, RoutedEventArgs e)
        {
            StopRenderingAndClearCache();
            OnCloseEvent?.Invoke(this, EventArgs.Empty);
        }
    }
}