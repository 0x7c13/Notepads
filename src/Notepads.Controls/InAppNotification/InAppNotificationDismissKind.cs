// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/InAppNotification

namespace Notepads.Controls
{
    /// <summary>
    /// Enumeration to describe how an InAppNotification was dismissed
    /// </summary>
    public enum InAppNotificationDismissKind
    {
        /// <summary>
        /// When the system dismissed the notification.
        /// </summary>
        Programmatic,

        /// <summary>
        /// When user explicitly dismissed the notification.
        /// </summary>
        User,

        /// <summary>
        /// When the system dismissed the notification after timeout.
        /// </summary>
        Timeout
    }
}