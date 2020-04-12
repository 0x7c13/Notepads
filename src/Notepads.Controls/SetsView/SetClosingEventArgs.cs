// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

namespace Notepads.Controls
{
    using System;

    /// <summary>
    /// Event arguments for <see cref="SetsView.SetClosing"/> event.
    /// </summary>
    public class SetClosingEventArgs : EventArgs
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SetClosingEventArgs"/> class.
        /// </summary>
        /// <param name="item">Item being closed.</param>
        /// <param name="set"><see cref="SetsViewItem"/> container being closed.</param>
        public SetClosingEventArgs(object item, SetsViewItem set)
        {
            Item = item;
            Set = set;
        }

        /// <summary>
        /// Gets the Item being closed.
        /// </summary>
        public object Item { get; }

        /// <summary>
        /// Gets the Set being closed.
        /// </summary>
        public SetsViewItem Set { get; }

        /// <summary>
        /// Gets or sets a value indicating whether the notification should be closed.
        /// </summary>
        public bool Cancel { get; set; }
    }
}