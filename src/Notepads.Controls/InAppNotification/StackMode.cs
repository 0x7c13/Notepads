// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/InAppNotification

namespace Notepads.Controls
{
    /// <summary>
    /// The Stack mode of an in-app notification.
    /// </summary>
    public enum StackMode
    {
        /// <summary>
        /// Each notification will replace the previous one
        /// </summary>
        Replace,

        /// <summary>
        /// Opening a notification will display it immediately, remaining notifications will appear when a notification is dismissed
        /// </summary>
        StackInFront,

        /// <summary>
        /// Dismissing a notification will show the next one in the queue
        /// </summary>
        QueueBehind
    }
}