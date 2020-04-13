// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/blob/master/Microsoft.Toolkit.Parsers/Core

namespace Notepads.Controls.Markdown
{
    using System;

    /// <summary>
    /// The StringValue attribute is used as a helper to decorate enum values with string representations.
    /// </summary>
    [AttributeUsage(AttributeTargets.Field)]
    public sealed class StringValueAttribute : Attribute
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="StringValueAttribute"/> class.
        /// Constructor accepting string value.
        /// </summary>
        /// <param name="value">String value</param>
        public StringValueAttribute(string value)
        {
            Value = value;
        }

        /// <summary>
        /// Gets property for string value.
        /// </summary>
        public string Value { get; }
    }
}