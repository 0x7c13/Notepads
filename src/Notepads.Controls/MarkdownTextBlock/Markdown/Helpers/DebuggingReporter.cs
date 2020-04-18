// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Helpers

namespace Notepads.Controls.Markdown
{
    using System.Diagnostics;

    /// <summary>
    /// Reports an error during debugging.
    /// </summary>
    internal class DebuggingReporter
    {
        /// <summary>
        /// Reports a critical error.
        /// </summary>
        public static void ReportCriticalError(string errorText)
        {
            Debug.WriteLine(errorText);
            if (Debugger.IsAttached)
            {
                Debugger.Break();
            }
        }
    }
}