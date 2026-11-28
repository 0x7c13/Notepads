// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Text;
using System.Text.RegularExpressions;
using Windows.UI.Text;
using Windows.UI.Xaml;

namespace UnitTests.Markdown
{
    /// <summary>
    /// The base class for our unit tests.
    /// </summary>
    public abstract class TestBase
    {
        /// <summary>
        /// Removes a column of whitespace from the start of each line.
        /// </summary>
        /// <param name="input"> The text to process. </param>
        /// <returns> The input text, but with some whitespace removed. </returns>
        protected static string CollapseWhitespace(string input)
        {
            // Count the minimum number of spaces.
            int spacesToRemove = int.MaxValue;
            foreach (Match match in Regex.Matches(input, @"^[ ]*(?=[^\s])", RegexOptions.Multiline))
                spacesToRemove = Math.Min(spacesToRemove, match.Length);
            if (spacesToRemove < int.MaxValue)
            {
                // Remove that many spaces from every line, and convert other spaces to tabs.
                input = input.Replace("\r\n" + new string(' ', spacesToRemove), "\r\n");
            }

            // Remove first blank line.
            if (input.StartsWith(Environment.NewLine))
                input = input.Substring(Environment.NewLine.Length);
            return input;
        }

        private static ConcurrentDictionary<Type, Dictionary<string, object>> defaultValueCache =
            new ConcurrentDictionary<Type, Dictionary<string, object>>();

        /// <summary>
        /// Serializes the given object.
        /// </summary>
        /// <param name="result"> The StringBuilder to output to. </param>
        /// <param name="obj"> The object to serialize. </param>
        /// <param name="indentLevel"> The indent level. </param>
        protected static void SerializeElement(StringBuilder result, object obj, int indentLevel)
        {
            var type = obj.GetType();
            if (result.Length > 0)
            {
                result.Append("\r\n");
                for (int i = 0; i < indentLevel; i++)
                    result.Append("    ");
            }
            result.Append(type.Name);

            // Look up default values using the static dependency properties.
            Dictionary<string, object> defaultValues;
            if (!defaultValueCache.TryGetValue(type, out defaultValues))
            {
                defaultValues = new Dictionary<string, object>();

                Type baseType = type;
                while (baseType != null)
                {
                    foreach (var propertyInfo in baseType.GetProperties(BindingFlags.Public | BindingFlags.Static))
                    {
                        if (propertyInfo.Name.EndsWith("Property") && propertyInfo.PropertyType == typeof(DependencyProperty))
                        {
                            var dp = (DependencyProperty)propertyInfo.GetValue(obj);
                            defaultValues.Add(propertyInfo.Name.Substring(0, propertyInfo.Name.Length - "Property".Length), dp.GetMetadata(type).DefaultValue);
                        }
                    }
                    baseType = baseType.GetTypeInfo().BaseType;
                }

                // Override some defaults.
                if (defaultValues.ContainsKey("FontSize"))
                    defaultValues["FontSize"] = 15.0;
                if (defaultValues.ContainsKey("TextAlignment"))
                    defaultValues["TextAlignment"] = TextAlignment.Left;
                if (defaultValues.ContainsKey("Margin"))
                    defaultValues["Margin"] = new Thickness(0, 12, 0, 0);

                // Cache it.
                defaultValueCache.TryAdd(type, defaultValues);
            }

            // Now look up all the instance properties.
            var complexProperties = new List<PropertyInfo>();
            bool first = true;
            foreach (var propertyInfo in type.GetProperties(BindingFlags.Public | BindingFlags.Instance).OrderBy(pi => pi.Name))
            {
                // Only read-write properties...
                if (propertyInfo.CanRead == false || propertyInfo.CanWrite == false)
                {
                    // ...but read-only enumerables are okay.
                    if (!typeof(IEnumerable<object>).IsAssignableFrom(propertyInfo.PropertyType))
                        continue;
                }

                object value = propertyInfo.GetValue(obj);

                // Check if it's the same as the default value.
                object defaultValue;
                if (defaultValues.TryGetValue(propertyInfo.Name, out defaultValue) && object.Equals(value, defaultValue))
                    continue;

                if (value == null)
                {
                    result.AppendFormat("{0} {1}: null", first ? "" : ",", propertyInfo.Name);
                    first = false;
                }
                else if (value is int || value is double || value is bool || value is byte || value is Enum)
                {
                    result.AppendFormat("{0} {1}: {2}", first ? "" : ",", propertyInfo.Name, value.ToString());
                    first = false;
                }
                else if (value is string || value is Thickness)
                {
                    result.AppendFormat("{0} {1}: '{2}'", first ? "" : ",", propertyInfo.Name, value.ToString());
                    first = false;
                }
                else if (value is FontWeight)
                {
                    result.AppendFormat("{0} {1}: {2}", first ? "" : ",", propertyInfo.Name, ((FontWeight)value).Weight);
                    first = false;
                }
                else
                {
                    // Do these later.
                    complexProperties.Add(propertyInfo);
                }
            }

            // Do the complex properties that we skipped before.
            foreach (var propertyInfo in complexProperties)
            {
                object value = propertyInfo.GetValue(obj);
                if (typeof(IEnumerable<object>).IsAssignableFrom(propertyInfo.PropertyType))
                {
                    // Enumerable.
                    foreach (var child in (IEnumerable<object>)propertyInfo.GetValue(obj))
                        SerializeElement(result, child, indentLevel + 1);
                }
                else
                {
                    // Complex type.
                    SerializeElement(result, value, indentLevel + 1);
                }
            }
        }
    }
}