// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.DiffViewer
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Media;

    public sealed class ScrollViewerSynchronizer : IDisposable
    {
        private readonly List<ScrollViewer> _scrollViewers;
        private readonly Dictionary<ScrollBar, ScrollViewer> _horizontalScrollBars = new Dictionary<ScrollBar, ScrollViewer>();
        private readonly Dictionary<ScrollBar, ScrollViewer> _verticalScrollBars = new Dictionary<ScrollBar, ScrollViewer>();
        private double _verticalScrollOffset = .0f;
        private double _horizontalScrollOffset = .0f;

        public ScrollViewerSynchronizer(List<ScrollViewer> scrollViewers)
        {
            _scrollViewers = scrollViewers;
            scrollViewers.ForEach(scrollViewer => scrollViewer.Loaded += ScrollViewerLoaded);
            scrollViewers.ForEach(scrollViewer => scrollViewer.Unloaded += ScrollViewerUnloaded);
        }

        private void ScrollViewerLoaded(object sender, RoutedEventArgs e)
        {
            if (!(sender is ScrollViewer scrollViewer)) return;

            scrollViewer.ChangeView(_horizontalScrollOffset, _verticalScrollOffset, null, true);

            scrollViewer.ApplyTemplate();

            var scrollViewerRoot = (FrameworkElement)VisualTreeHelper.GetChild(scrollViewer, 0);
            var horizontalScrollBar = (ScrollBar)scrollViewerRoot.FindName("HorizontalScrollBar");
            var verticalScrollBar = (ScrollBar)scrollViewerRoot.FindName("VerticalScrollBar");

            if (horizontalScrollBar != null)
            {
                if (!_horizontalScrollBars.Keys.Contains(horizontalScrollBar))
                {
                    _horizontalScrollBars.Add(horizontalScrollBar, scrollViewer);
                }

                horizontalScrollBar.Scroll += HorizontalScrollBar_Scroll;
                horizontalScrollBar.ValueChanged += HorizontalScrollBar_ValueChanged;
            }

            if (verticalScrollBar != null)
            {
                if (!_verticalScrollBars.Keys.Contains(verticalScrollBar))
                {
                    _verticalScrollBars.Add(verticalScrollBar, scrollViewer);
                }

                verticalScrollBar.Scroll += VerticalScrollBar_Scroll;
                verticalScrollBar.ValueChanged += VerticalScrollBar_ValueChanged;
            }
        }

        private void ScrollViewerUnloaded(object sender, RoutedEventArgs e)
        {
            if (!(sender is ScrollViewer scrollViewer)) return;

            scrollViewer.ApplyTemplate();

            var scrollViewerRoot = (FrameworkElement)VisualTreeHelper.GetChild(scrollViewer, 0);
            var horizontalScrollBar = (ScrollBar)scrollViewerRoot.FindName("HorizontalScrollBar");
            var verticalScrollBar = (ScrollBar)scrollViewerRoot.FindName("VerticalScrollBar");

            if (horizontalScrollBar != null)
            {
                horizontalScrollBar.Scroll -= HorizontalScrollBar_Scroll;
                horizontalScrollBar.ValueChanged -= HorizontalScrollBar_ValueChanged;

                if (_horizontalScrollBars.Keys.Contains(horizontalScrollBar))
                {
                    _horizontalScrollBars.Remove(horizontalScrollBar);
                }
            }

            if (verticalScrollBar != null)
            {
                verticalScrollBar.Scroll -= VerticalScrollBar_Scroll;
                verticalScrollBar.ValueChanged -= VerticalScrollBar_ValueChanged;

                if (_verticalScrollBars.Keys.Contains(verticalScrollBar))
                {
                    _verticalScrollBars.Remove(verticalScrollBar);
                }
            }
        }

        private void VerticalScrollBar_ValueChanged(object sender, RangeBaseValueChangedEventArgs e)
        {
            if (!(sender is ScrollBar changedScrollBar)) return;
            var changedScrollViewer = _verticalScrollBars[changedScrollBar];
            Scroll(changedScrollViewer);
        }

        private void VerticalScrollBar_Scroll(object sender, ScrollEventArgs e)
        {
            if (!(sender is ScrollBar changedScrollBar)) return;
            var changedScrollViewer = _verticalScrollBars[changedScrollBar];
            Scroll(changedScrollViewer);
        }

        private void HorizontalScrollBar_ValueChanged(object sender, RangeBaseValueChangedEventArgs e)
        {
            if (!(sender is ScrollBar changedScrollBar)) return;
            var changedScrollViewer = _horizontalScrollBars[changedScrollBar];
            Scroll(changedScrollViewer);
        }

        private void HorizontalScrollBar_Scroll(object sender, ScrollEventArgs e)
        {
            if (!(sender is ScrollBar changedScrollBar)) return;
            var changedScrollViewer = _horizontalScrollBars[changedScrollBar];
            Scroll(changedScrollViewer);
        }

        private void Scroll(ScrollViewer changedScrollViewer)
        {
            _verticalScrollOffset = changedScrollViewer.VerticalOffset;
            _horizontalScrollOffset = changedScrollViewer.HorizontalOffset;

            foreach (var scrollViewer in _scrollViewers.Where(s => s != changedScrollViewer))
            {
                if (Math.Abs(scrollViewer.VerticalOffset - changedScrollViewer.VerticalOffset) > 0.01)
                {
                    scrollViewer.ChangeView(null, changedScrollViewer.VerticalOffset, null, true);
                }

                if (Math.Abs(scrollViewer.HorizontalOffset - changedScrollViewer.HorizontalOffset) > 0.01)
                {
                    scrollViewer.ChangeView(changedScrollViewer.HorizontalOffset, null, null, true);
                }
            }
        }

        public void Dispose()
        {
            var horizontalScrollBars = new List<ScrollBar>(_horizontalScrollBars.Keys);
            horizontalScrollBars.ForEach(horizontalScrollBar =>
            {
                horizontalScrollBar.Scroll -= HorizontalScrollBar_Scroll;
                horizontalScrollBar.ValueChanged -= HorizontalScrollBar_ValueChanged;
            });
            horizontalScrollBars.Clear();

            var verticalScrollBars = new List<ScrollBar>(_verticalScrollBars.Keys);
            verticalScrollBars.ForEach(verticalScrollBar =>
            {
                verticalScrollBar.Scroll -= VerticalScrollBar_Scroll;
                verticalScrollBar.ValueChanged -= VerticalScrollBar_ValueChanged;
            });
            verticalScrollBars.Clear();

            _horizontalScrollBars.Clear();
            _verticalScrollBars.Clear();

            _scrollViewers?.ForEach(scrollViewer => scrollViewer.Loaded -= ScrollViewerLoaded);
            _scrollViewers?.ForEach(scrollViewer => scrollViewer.Unloaded -= ScrollViewerUnloaded);
            _scrollViewers?.Clear();
        }
    }
}