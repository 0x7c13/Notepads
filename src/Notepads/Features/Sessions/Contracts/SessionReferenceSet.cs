// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;

namespace Notepads.Features.Sessions.Contracts;

internal class SessionReferenceSet
{
    public HashSet<string> BaselineFileNames { get; } = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

    public HashSet<string> JournalFileNames { get; } = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

    public HashSet<string> LegacyBackupPaths { get; } = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

    public HashSet<string> FutureAccessTokens { get; } = new HashSet<string>(StringComparer.Ordinal);

    public void Include(SessionReferenceSet references)
    {
        BaselineFileNames.UnionWith(references.BaselineFileNames);
        JournalFileNames.UnionWith(references.JournalFileNames);
        LegacyBackupPaths.UnionWith(references.LegacyBackupPaths);
        FutureAccessTokens.UnionWith(references.FutureAccessTokens);
    }

}
