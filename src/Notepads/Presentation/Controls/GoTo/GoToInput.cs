// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Linq;

namespace Notepads.Presentation.Controls.GoTo;

internal static class GoToInput
{
    public static bool ContainsAllowableCharactersOnly(this string str, params char[] allowableCharacters)
    {
        return str.All(allowableCharacters.Contains);
    }
}
