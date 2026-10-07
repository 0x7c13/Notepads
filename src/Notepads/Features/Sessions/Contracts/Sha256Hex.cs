// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Features.Sessions.Contracts;

internal static class Sha256Hex
{
    internal static bool IsValid(string value)
    {
        if (value?.Length != 64) return false;
        foreach (var character in value)
            if (!char.IsAsciiHexDigit(character)) return false;
        return true;
    }
}
