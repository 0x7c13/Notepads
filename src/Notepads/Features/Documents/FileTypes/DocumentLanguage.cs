// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Features.Documents.FileTypes;

public sealed class DocumentLanguage
{
    internal DocumentLanguage(string id, string displayName, params string[] extensions)
    {
        Id = id;
        DisplayName = displayName;
        Extensions = extensions;
    }

    public string Id { get; }
    public string DisplayName { get; }
    internal string[] Extensions { get; }
}
