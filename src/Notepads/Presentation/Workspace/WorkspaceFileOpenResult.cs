// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;

namespace Notepads.Presentation.Workspace;

public enum WorkspaceFileOpenKind { Created, Existing, Failed, Cancelled }

public sealed record WorkspaceFileOpenResult(
    WorkspaceFileOpenKind Kind, WorkspaceTab Tab = null, Exception Error = null);
