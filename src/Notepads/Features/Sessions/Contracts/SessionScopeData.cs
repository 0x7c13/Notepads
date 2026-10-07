// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;

namespace Notepads.Features.Sessions.Contracts;

internal sealed class SessionScopeData
{
    public const string Primary = "Primary";
    public const string Secondary = "Secondary";

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

    public static Guid GetStableOwner()
    {
        using (var hash = SHA256.Create())
        {
            var bytes = hash.ComputeHash(Encoding.UTF8.GetBytes("Notepads.DocumentOwner/NotepadsSessionData.json"));
            var identity = new byte[16];
            Array.Copy(bytes, identity, identity.Length);
            return new Guid(identity);
        }
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
