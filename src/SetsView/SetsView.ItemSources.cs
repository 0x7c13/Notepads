// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

namespace SetsView
{
    using System.Reflection;
    using Windows.Foundation.Collections;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls.Primitives;

    /// <summary>
    /// SetsView methods related to tracking Items and ItemsSource changes.
    /// </summary>
    public partial class SetsView
    {
        // Temporary tracking of previous collections for removing events.
        private MethodInfo _removeItemsSourceMethod;

        /// <inheritdoc/>
        protected override void OnItemsChanged(object e)
        {
            IVectorChangedEventArgs args = (IVectorChangedEventArgs)e;

            base.OnItemsChanged(e);

            if (args.CollectionChange == CollectionChange.ItemRemoved && SelectedIndex == -1)
            {
                // If we remove the selected item we should select the previous item
                int startIndex = (int)args.Index + 1;
                if (startIndex > Items.Count)
                {
                    startIndex = 0;
                }

                SelectedIndex = FindNextSetIndex(startIndex, -1);
            }

            // Update Sizing (in case there are less items now)
            SetsView_SizeChanged(this, null);
        }

        private void ItemContainerGenerator_ItemsChanged(object sender, ItemsChangedEventArgs e)
        {
            var action = (CollectionChange)e.Action;
            if (action == CollectionChange.Reset)
            {
                // Reset collection to reload later.
                _hasLoaded = false;
            }
        }

        private void SetInitialSelection()
        {
            if (SelectedItem == null)
            {
                // If we have an index, but didn't get the selection, make the selection
                if (SelectedIndex >= 0 && SelectedIndex < Items.Count)
                {
                    SelectedItem = Items[SelectedIndex];
                }

                // Otherwise, select the first item by default
                else if (Items.Count >= 1)
                {
                    SelectedItem = Items[0];
                }
            }
        }

        // Finds the next visible & enabled set index.
        private int FindNextSetIndex(int startIndex, int direction)
        {
            int index = startIndex;
            if (direction != 0)
            {
                for (int i = 0; i < Items.Count; i++)
                {
                    index += direction;

                    if (index >= Items.Count)
                    {
                        index = 0;
                    }
                    else if (index < 0)
                    {
                        index = Items.Count - 1;
                    }

                    if (ContainerFromIndex(index) is SetsViewItem setItem && setItem.IsEnabled && setItem.Visibility == Visibility.Visible)
                    {
                        break;
                    }
                }
            }

            return index;
        }

        private void ItemsSource_PropertyChanged(DependencyObject sender, DependencyProperty dp)
        {
            // Use reflection to store a 'Remove' method of any possible collection in ItemsSource
            // Cache for efficiency later.
            if (ItemsSource != null)
            {
                _removeItemsSourceMethod = ItemsSource.GetType().GetMethod("Remove");
            }
            else
            {
                _removeItemsSourceMethod = null;
            }
        }
    }
}
