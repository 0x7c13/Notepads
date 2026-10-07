// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Text.Json.Serialization;
using Notepads.Features.Sessions.Contracts.Legacy;

namespace Notepads.Features.Sessions.Contracts;

[JsonSerializable(typeof(NotepadsSessionDataV1))]
[JsonSerializable(typeof(NotepadsSessionDataV2))]
[JsonSerializable(typeof(TextEditorSessionDataV2))]
[JsonSerializable(typeof(TransferToken))]
[JsonSerializable(typeof(RecoveryRecord))]
internal sealed partial class SessionJsonContext : JsonSerializerContext
{
}
