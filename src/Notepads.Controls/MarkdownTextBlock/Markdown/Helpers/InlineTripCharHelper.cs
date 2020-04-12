// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Helpers

namespace Notepads.Controls.Markdown
{
    /// <summary>
    /// A helper class for the trip chars. This is an optimization. If we ask each class to go
    /// through the rage and look for itself we end up looping through the range n times, once
    /// for each inline. This class represent a character that an inline needs to have a
    /// possible match. We will go through the range once and look for everyone's trip chars,
    /// and if they can make a match from the trip char then we will commit to them.
    /// </summary>
    internal class InlineTripCharHelper
    {
        // Note! Everything in first char and suffix should be lower case!
        public char FirstChar { get; set; }

        public InlineParseMethod Method { get; set; }
    }
}