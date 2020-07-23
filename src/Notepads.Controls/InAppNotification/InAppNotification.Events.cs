// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/InAppNotification

namespace Notepads.Controls
{
    using System;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Automation;
    using Windows.UI.Xaml.Automation.Peers;

    /// <summary>
    /// In App Notification defines a control to show local notification in the app.
    /// </summary>
    public partial class InAppNotification
    {
        /// <summary>
        /// Event raised when the notification is opening
        /// </summary>
        public event InAppNotificationOpeningEventHandler Opening;

        /// <summary>
        /// Event raised when the notification is opened
        /// </summary>
        public event EventHandler Opened;

        /// <summary>
        /// Event raised when the notification is closing
        /// </summary>
        public event InAppNotificationClosingEventHandler Closing;

        /// <summary>
        /// Event raised when the notification is closed
        /// </summary>
        public event InAppNotificationClosedEventHandler Closed;

        private AutomationPeer peer;

        private void DismissButton_Click(object sender, RoutedEventArgs e)
        {
            Dismiss(InAppNotificationDismissKind.User);
        }

        private void DismissTimer_Tick(object sender, object e)
        {
            Dismiss(InAppNotificationDismissKind.Timeout);
        }

        private void OpenAnimationTimer_Tick(object sender, object e)
        {
            lock (_openAnimationTimer)
            {
                _openAnimationTimer.Stop();
                Opened?.Invoke(this, EventArgs.Empty);
                SetValue(AutomationProperties.NameProperty, "Notification");
                peer = FrameworkElementAutomationPeer.CreatePeerForElement(ContentTemplateRoot);
                if (Content?.GetType() == typeof(string))
                {
                    AutomateTextNotification(Content.ToString());
                }
            }
        }

        private void AutomateTextNotification(string message)
        {
            if (peer != null)
            {
                peer.SetFocus();
                peer.RaiseNotificationEvent(
                    AutomationNotificationKind.Other,
                    AutomationNotificationProcessing.ImportantMostRecent,
                    "New notification" + message,
                    Guid.NewGuid().ToString());
            }
        }

        private void ClosingAnimationTimer_Tick(object sender, object e)
        {
            lock (_closingAnimationTimer)
            {
                _closingAnimationTimer.Stop();
                Closed?.Invoke(this, new InAppNotificationClosedEventArgs(_lastDismissKind));
            }
        }
    }
}