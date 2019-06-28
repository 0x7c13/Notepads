// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

namespace SetsView
{
    using System;

    /// <summary>
    /// A class used by the <see cref="SetsView"/> TabDraggedOutside Event
    /// </summary>
    public class SetDraggedOutsideEventArgs : EventArgs
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SetDraggedOutsideEventArgs"/> class.
        /// </summary>
        /// <param name="item">data context of element dragged</param>
        /// <param name="set"><see cref="SetsViewItem"/> container being dragged.</param>
        public SetDraggedOutsideEventArgs(object item, SetsViewItem set)
        {
            Item = item;
            Set = set;
        }

        /// <summary>
        /// Gets or sets the Item/Data Context of the item being dragged outside of the <see cref="SetsView"/>.
        /// </summary>
        public object Item { get; set; }

        /// <summary>
        /// Gets the Set being dragged outside of the <see cref="SetsView"/>.
        /// </summary>
        public SetsViewItem Set { get; private set; }
    }
}
