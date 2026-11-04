// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Security.Cryptography;

namespace Notepads.Features.Sessions.Contracts;

internal sealed class SessionScopeData
{
    public const string Primary = "Primary";
    public const string Secondary = "Secondary";

    private static readonly Guid StableOwner = CreateStableOwner();

    public Guid OwnerId { get; set; }

    public Guid InstanceId { get; set; }

    public string Kind { get; set; }

    public void Validate()
    {
        if (OwnerId == Guid.Empty || InstanceId == Guid.Empty ||
            Kind != Primary && Kind != Secondary ||
            Kind == Secondary && (OwnerId != InstanceId || IsReservedOwner(OwnerId)))
        {
            throw new InvalidDataException("Invalid recovery session scope.");
        }
    }

    public static Guid GetStableOwner() => StableOwner;

    private static Guid CreateStableOwner()
    {
        Span<byte> hash = stackalloc byte[SHA256.HashSizeInBytes];
        SHA256.HashData("Notepads.DocumentOwner/NotepadsSessionData.json"u8, hash);
        return new Guid(hash[..16]);
    }

    public static bool IsReservedOwner(Guid ownerId) => ownerId == GetStableOwner();

    public static string GetManifestFileName(Guid ownerId) => ownerId == GetStableOwner() ? "NotepadsSessionData.json" :
        GetSecondaryPrefix(ownerId) + "NotepadsSessionData.json";

    public static string GetSecondaryPrefix(Guid ownerId)
    {
        if (ownerId == Guid.Empty) throw new ArgumentException("A secondary session requires an owner.", nameof(ownerId));
        return "Secondary-" + ownerId.ToString("N") + "-";
    }
}
