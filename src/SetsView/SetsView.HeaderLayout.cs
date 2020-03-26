// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

namespace SetsView
{
    using System;
    using System.Linq;
    using Windows.UI.Xaml;

    /// <summary>
    /// SetsView methods related to calculating the width of the <see cref="SetsViewItem"/> Headers.
    /// </summary>
    public partial class SetsView
    {
        // Attached property for storing widths of sets if set by other means during layout.
        private static double GetOriginalWidth(SetsViewItem obj)
        {
            return (double)obj.GetValue(OriginalWidthProperty);
        }

        private static void SetOriginalWidth(SetsViewItem obj, double value)
        {
            obj.SetValue(OriginalWidthProperty, value);
        }

        // Using a DependencyProperty as the backing store for MyProperty.  This enables animation, styling, binding, etc...
        private static readonly DependencyProperty OriginalWidthProperty =
            DependencyProperty.RegisterAttached("OriginalWidth", typeof(double), typeof(SetsView), new PropertyMetadata(null));

        private static void OnLayoutEffectingPropertyChanged(DependencyObject sender, DependencyPropertyChangedEventArgs args)
        {
            if (sender is SetsView setsView && setsView._hasLoaded)
            {
                setsView.SetsView_SizeChanged(setsView, null);
            }
        }

        private void SetsView_SizeChanged(object sender, SizeChangedEventArgs e)
        {
            // We need to do this calculation here in Size Changed as the
            // Columns don't have their Actual Size calculated in Measure or Arrange.
            if (_hasLoaded && _setsViewContainer != null)
            {
                // Look for our special columns to calculate size of other 'stuff'
                var taken = _setsViewContainer.ColumnDefinitions.Sum(cd => GetIgnoreColumn(cd) ? 0 : cd.ActualWidth);

                // Get the column we want to work on for available space
                var setc = _setsViewContainer.ColumnDefinitions.FirstOrDefault(cd => GetConstrainColumn(cd));
                if (setc != null)
                {
                    var available = ActualWidth - taken;
                    var required = 0.0;
                    var minsetwidth = double.MaxValue;

                    if (SetsWidthBehavior == SetsWidthMode.Actual)
                    {
                        if (_setsScroller != null)
                        {
                            // If we have a scroll container, get its size.
                            required = _setsScroller.ExtentWidth;
                        }

                        // Restore original widths
                        foreach (var item in Items)
                        {
                            if (!(ContainerFromItem(item) is SetsViewItem set))
                            {
                                continue; // container not generated yet
                            }

                            if (set.ReadLocalValue(OriginalWidthProperty) != DependencyProperty.UnsetValue)
                            {
                                set.Width = GetOriginalWidth(set);
                            }
                        }
                    }
                    else if (available > 0)
                    {
                        // Calculate the width for each set from the provider and determine how much space they take.
                        foreach (var item in Items)
                        {
                            if (!(ContainerFromItem(item) is SetsViewItem set))
                            {
                                continue; // container not generated yet
                            }

                            minsetwidth = Math.Min(minsetwidth, set.MinWidth);

                            double width = double.NaN;

                            switch (SetsWidthBehavior)
                            {
                                case SetsWidthMode.Equal:
                                    width = ProvideEqualWidth(set, item, available);
                                    break;
                                case SetsWidthMode.Compact:
                                    width = ProvideCompactWidth(set, item, available);
                                    break;
                            }

                            if (set.ReadLocalValue(OriginalWidthProperty) == DependencyProperty.UnsetValue)
                            {
                                SetOriginalWidth(set, set.Width);
                            }

                            if (width > double.Epsilon)
                            {
                                set.Width = width;
                                required += Math.Max(Math.Min(width, set.MaxWidth), set.MinWidth);
                            }
                            else
                            {
                                set.Width = GetOriginalWidth(set);
                                required += set.ActualWidth;
                            }
                        }
                    }
                    else
                    {
                        // Fix negative bounds.
                        available = 0.0;

                        // Still need to determine a 'minimum' width (if available)
                        // TODO: Consolidate this logic with above better?
                        foreach (var item in Items)
                        {
                            if (!(ContainerFromItem(item) is SetsViewItem set))
                            {
                                continue; // container not generated yet
                            }

                            minsetwidth = Math.Min(minsetwidth, set.MinWidth);
                        }
                    }

                    if (!(minsetwidth < double.MaxValue))
                    {
                        minsetwidth = 0.0; // No Containers, no visual, 0 size.
                    }

                    if (available > minsetwidth)
                    {
                        // Constrain the column based on our required and available space
                        setc.MaxWidth = available;
                    }

                    //// TODO: If it's less, should we move the selected set to only be the one shown by default?

                    if (available <= minsetwidth || Math.Abs(available - minsetwidth) < double.Epsilon)
                    {
                        setc.Width = new GridLength(minsetwidth);
                    }
                    else if (required >= available)
                    {
                        // Fix size as we don't have enough space for all the sets.
                        setc.Width = new GridLength(available);
                    }
                    else
                    {
                        // We haven't filled up our space, so we want to expand to take as much as needed.
                        setc.Width = GridLength.Auto;
                    }
                }
                UpdateScrollViewerShadows();
                UpdateScrollViewerNavigateButtons();
                UpdateSetSeparators();
            }
        }

        private double ProvideEqualWidth(SetsViewItem set, object item, double availableWidth)
        {
            if (double.IsNaN(SelectedSetWidth))
            {
                if (Items.Count <= 1)
                {
                    return availableWidth;
                }

                return Math.Max(set.MinWidth, availableWidth / Items.Count);
            }
            else if (Items.Count() <= 1)
            {
                // Default case of a single set, make it full size.
                return Math.Min(SelectedSetWidth, availableWidth);
            }
            else
            {
                var width = (availableWidth - SelectedSetWidth) / (Items.Count - 1);

                // Constrain between Min and Selected (Max)
                if (width < set.MinWidth)
                {
                    width = set.MinWidth;
                }
                else if (width > SelectedSetWidth)
                {
                    width = SelectedSetWidth;
                }

                // If it's selected make it full size, otherwise whatever the size should be.
                return set.IsSelected
                    ? Math.Min(SelectedSetWidth, availableWidth)
                    : width;
            }
        }

        private double ProvideCompactWidth(SetsViewItem set, object item, double availableWidth)
        {
            // If we're selected and have a value for that, then just return that.
            if (set.IsSelected && !double.IsNaN(SelectedSetWidth))
            {
                return SelectedSetWidth;
            }

            // Otherwise use min size.
            return set.MinWidth;
        }
    }
}
