namespace Notepads.Extensions.DiffViewer
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Media;

    public class ScrollViewerSynchronizer
    {
        private readonly List<ScrollViewer> _scrollViewers;
        private readonly Dictionary<ScrollBar, ScrollViewer> _verticalScrollerViewers = new Dictionary<ScrollBar, ScrollViewer>();
        private readonly Dictionary<ScrollBar, ScrollViewer> _horizontalScrollerViewers = new Dictionary<ScrollBar, ScrollViewer>();
        private double _verticalScrollOffset;
        private double _horizontalScrollOffset;

        public ScrollViewerSynchronizer(List<ScrollViewer> scrollViewers)
        {
            _scrollViewers = scrollViewers;
            scrollViewers.ForEach(x => x.Loaded += Scroller_Loaded);
        }

        private void Scroller_Loaded(object sender, RoutedEventArgs e)
        {
            var scrollViewer = (ScrollViewer)sender;

            scrollViewer.ChangeView(_horizontalScrollOffset, _verticalScrollOffset, null, true);

            scrollViewer.Opacity = 1;
            if (_verticalScrollerViewers.Count > 0)
            {
                scrollViewer.ChangeView(null, _verticalScrollOffset, null, true);
            }
            scrollViewer.ApplyTemplate();

            var scrollViewerRoot = (FrameworkElement)VisualTreeHelper.GetChild(scrollViewer, 0);
            var horizontalScrollBar = (ScrollBar)scrollViewerRoot.FindName("HorizontalScrollBar");
            var verticalScrollBar = (ScrollBar)scrollViewerRoot.FindName("VerticalScrollBar");

            if (!_horizontalScrollerViewers.Keys.Contains(horizontalScrollBar))
            {
                _horizontalScrollerViewers.Add(horizontalScrollBar, scrollViewer);
            }

            if (!_verticalScrollerViewers.Keys.Contains(verticalScrollBar))
            {
                _verticalScrollerViewers.Add(verticalScrollBar, scrollViewer);
            }

            if (horizontalScrollBar != null)
            {
                horizontalScrollBar.Scroll += HorizontalScrollBar_Scroll;
                horizontalScrollBar.ValueChanged += HorizontalScrollBar_ValueChanged;
            }

            if (verticalScrollBar != null)
            {
                verticalScrollBar.Scroll += VerticalScrollBar_Scroll;
                verticalScrollBar.ValueChanged += VerticalScrollBar_ValueChanged;
            }
        }

        private void VerticalScrollBar_ValueChanged(object sender, RangeBaseValueChangedEventArgs e)
        {
            var changedScrollBar = sender as ScrollBar;
            var changedScrollViewer = _verticalScrollerViewers[changedScrollBar];
            Scroll(changedScrollViewer);
        }

        private void VerticalScrollBar_Scroll(object sender, ScrollEventArgs e)
        {
            var changedScrollBar = sender as ScrollBar;
            var changedScrollViewer = _verticalScrollerViewers[changedScrollBar];
            Scroll(changedScrollViewer);
        }

        private void HorizontalScrollBar_ValueChanged(object sender, RangeBaseValueChangedEventArgs e)
        {
            var changedScrollBar = sender as ScrollBar;
            var changedScrollViewer = _horizontalScrollerViewers[changedScrollBar];
            Scroll(changedScrollViewer);
        }

        private void HorizontalScrollBar_Scroll(object sender, ScrollEventArgs e)
        {
            var changedScrollBar = sender as ScrollBar;
            var changedScrollViewer = _horizontalScrollerViewers[changedScrollBar];
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
    }
}