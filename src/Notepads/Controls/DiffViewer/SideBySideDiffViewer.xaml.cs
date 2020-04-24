﻿namespace Notepads.Controls.DiffViewer
{
    using System;
    using System.Collections.Generic;
    using System.Threading;
    using System.Threading.Tasks;
    using Notepads.Commands;
    using Notepads.Extensions;
    using Notepads.Services;
    using Windows.System;
    using Windows.UI;
    using Windows.UI.Core;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    public sealed partial class SideBySideDiffViewer : UserControl, ISideBySideDiffViewer
    {
        public event EventHandler OnCloseEvent;

        private readonly RichTextBlockDiffRenderer _diffRenderer;

        private readonly ICommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;
        private readonly ICommandHandler<PointerRoutedEventArgs> _mouseCommandHandler;

        private CancellationTokenSource _cancellationTokenSource;

        private readonly ScrollBar _rightScrollViewerHorizontalScrollBar;
        private readonly ScrollBar _rightScrollViewerVerticalScrollBar;

        public SideBySideDiffViewer()
        {
            InitializeComponent();

            _diffRenderer = new RichTextBlockDiffRenderer();
            _keyboardCommandHandler = GetKeyboardCommandHandler();
            _mouseCommandHandler = GetMouseCommandHandler();

            RightScroller.ApplyTemplate();

            var scrollViewerRoot = (FrameworkElement)VisualTreeHelper.GetChild(RightScroller, 0);
            _rightScrollViewerHorizontalScrollBar = (ScrollBar)scrollViewerRoot.FindName("HorizontalScrollBar");
            _rightScrollViewerVerticalScrollBar = (ScrollBar)scrollViewerRoot.FindName("VerticalScrollBar");

            _rightScrollViewerHorizontalScrollBar.ValueChanged += HorizontalScrollBar_ValueChanged;
            _rightScrollViewerVerticalScrollBar.ValueChanged += VerticalScrollBar_ValueChanged;

            LeftTextBlock.SelectionHighlightColor = new SolidColorBrush(ThemeSettingsService.AppAccentColor);
            RightTextBlock.SelectionHighlightColor = new SolidColorBrush(ThemeSettingsService.AppAccentColor);

            LeftTextBlockBorder.KeyDown += LeftTextBlockBorder_KeyDown;
            RightTextBlockBorder.KeyDown += RightTextBlockBorder_KeyDown;
            LeftTextBlockBorder.PointerWheelChanged += LeftTextBlockBorder_PointerWheelChanged;
            RightTextBlockBorder.PointerWheelChanged += RightTextBlockBorder_PointerWheelChanged;

            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;

            DismissButton.Click += DismissButton_OnClick;
            LayoutRoot.KeyDown += OnKeyDown;
            KeyDown += OnKeyDown;
            LeftTextBlock.KeyDown += OnKeyDown;
            RightTextBlock.KeyDown += OnKeyDown;
            Loaded += SideBySideDiffViewer_Loaded;
        }

        public void Dispose()
        {
            StopRenderingAndClearCache();

            ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;

            DismissButton.Click -= DismissButton_OnClick;
            LayoutRoot.KeyDown -= OnKeyDown;
            KeyDown -= OnKeyDown;
            LeftTextBlock.KeyDown -= OnKeyDown;
            RightTextBlock.KeyDown -= OnKeyDown;
            Loaded -= SideBySideDiffViewer_Loaded;

            LeftTextBlockBorder.KeyDown -= LeftTextBlockBorder_KeyDown;
            RightTextBlockBorder.KeyDown -= RightTextBlockBorder_KeyDown;
            LeftTextBlockBorder.PointerWheelChanged -= LeftTextBlockBorder_PointerWheelChanged;
            RightTextBlockBorder.PointerWheelChanged -= RightTextBlockBorder_PointerWheelChanged;

            _rightScrollViewerHorizontalScrollBar.ValueChanged -= HorizontalScrollBar_ValueChanged;
            _rightScrollViewerVerticalScrollBar.ValueChanged -= VerticalScrollBar_ValueChanged;
        }

        private void HorizontalScrollBar_ValueChanged(object sender, RangeBaseValueChangedEventArgs args)
        {
            RightScroller.StartExpressionAnimation(LeftTextBlock, Axis.X);
        }

        private void VerticalScrollBar_ValueChanged(object sender, RangeBaseValueChangedEventArgs args)
        {
            RightScroller.StartExpressionAnimation(LeftTextBlock, Axis.Y);
        }

        private void ScrollUsingArrowKeys(KeyRoutedEventArgs e)
        {
            switch (e.Key)
            {
                case VirtualKey.Down:
                    RightScroller.ChangeView(null, RightScroller.VerticalOffset + 127, null, false);
                    break;
                case VirtualKey.Up:
                    RightScroller.ChangeView(null, RightScroller.VerticalOffset - 127, null, false);
                    break;
                case VirtualKey.Left:
                    RightScroller.ChangeView(RightScroller.HorizontalOffset - 90, null, null, false);
                    break;
                case VirtualKey.Right:
                    RightScroller.ChangeView(RightScroller.HorizontalOffset + 90, null, null, false);
                    break;
            }
        }

        private void LeftTextBlockBorder_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            ScrollUsingArrowKeys(e);
            e.Handled = true;
        }

        private void RightTextBlockBorder_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            ScrollUsingArrowKeys(e);
            e.Handled = true;
        }

        private void SideBySideDiffViewer_Loaded(object sender, RoutedEventArgs e)
        {
            Focus();
        }

        private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                LeftTextBlock.SelectionHighlightColor = new SolidColorBrush(color);
                RightTextBlock.SelectionHighlightColor = new SolidColorBrush(color);
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

        public void RenderDiff(string left, string right, ElementTheme theme)
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
    }
}