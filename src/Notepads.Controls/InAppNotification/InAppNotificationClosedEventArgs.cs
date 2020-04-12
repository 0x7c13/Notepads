// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/InAppNotification

namespace Notepads.Controls
{
    using System;

    /// <summary>
    /// A delegate for <see cref="InAppNotification"/> dismissing.
    /// </summary>
    /// <param name="sender">The sender.</param>
    /// <param name="e">The event arguments.</param>
    public delegate void InAppNotificationClosedEventHandler(object sender, InAppNotificationClosedEventArgs e);

    /// <summary>
    /// Provides data for the <see cref="InAppNotification"/> Dismissing event.
    /// </summary>
    public class InAppNotificationClosedEventArgs : EventArgs
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="InAppNotificationClosedEventArgs"/> class.
        /// </summary>
        /// <param name="dismissKind">Dismiss kind that triggered the closing event</param>
        public InAppNotificationClosedEventArgs(InAppNotificationDismissKind dismissKind)
        {
            DismissKind = dismissKind;
        }

        /// <summary>
        /// Gets the kind of action for the closing event.
        /// </summary>
        public InAppNotificationDismissKind DismissKind { get; private set; }
    }
}