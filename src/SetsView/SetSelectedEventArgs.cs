// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

namespace SetsView
{
    using System;

    /// <summary>
    /// Event arguments for <see cref="SetsView.SetSelected"/> event.
    /// </summary>
    public class SetSelectedEventArgs : EventArgs
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SetSelectedEventArgs"/> class.
        /// </summary>
        /// <param name="item">Selected item.</param>
        /// <param name="set"><see cref="SetsViewItem"/> Selected set.</param>
        public SetSelectedEventArgs(object item, SetsViewItem set)
        {
            Item = item;
            Set = set;
        }

        /// <summary>
        /// Gets the Selected Item.
        /// </summary>
        public object Item { get; private set; }

        /// <summary>
        /// Gets the Selected Set.
        /// </summary>
        public SetsViewItem Set { get; private set; }
    }
}