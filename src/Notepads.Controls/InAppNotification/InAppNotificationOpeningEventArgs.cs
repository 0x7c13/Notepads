// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/InAppNotification

namespace Notepads.Controls
{
    using System;

    /// <summary>
    /// A delegate for <see cref="InAppNotification"/> opening.
    /// </summary>
    /// <param name="sender">The sender.</param>
    /// <param name="e">The event arguments.</param>
    public delegate void InAppNotificationOpeningEventHandler(object sender, InAppNotificationOpeningEventArgs e);

    /// <summary>
    /// Provides data for the <see cref="InAppNotification"/> Dismissing event.
    /// </summary>
    public class InAppNotificationOpeningEventArgs : EventArgs
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="InAppNotificationOpeningEventArgs"/> class.
        /// </summary>
        public InAppNotificationOpeningEventArgs()
        {
        }

        /// <summary>
        /// Gets or sets a value indicating whether the notification should be opened.
        /// </summary>
        public bool Cancel { get; set; }
    }
}