// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;

namespace Notepads.Infrastructure.Resources;

public static class AppResourceUri
{
    public static Uri ToAppxUri(this string path)
    {
        string prefix = $"ms-appx://{(path.StartsWith('/') ? string.Empty : "/")}";
        return new Uri($"{prefix}{path}");
    }

}
