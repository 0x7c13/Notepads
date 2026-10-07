// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Globalization;
using System.IO;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.Features.Sessions.Storage;

/// <summary>Construct paths only from validated identities, never document-supplied relative paths.</summary>
internal sealed class RecoveryStoragePaths
{
    public RecoveryStoragePaths(string rootPath) : this(rootPath, File.GetAttributes)
    {
    }

    internal RecoveryStoragePaths(string rootPath, Func<string, FileAttributes> getAttributes)
    {
        if (string.IsNullOrWhiteSpace(rootPath)) throw new ArgumentException("A recovery root is required.", nameof(rootPath));
        ArgumentNullException.ThrowIfNull(getAttributes);
        RootPath = Path.TrimEndingDirectorySeparator(Path.GetFullPath(rootPath));
        // The OS-supplied app-private root is our trust boundary. Inspect that root once; scans refuse
        // reparse points below it, and a packaged process has no authority to inspect its ancestors.
        try
        {
            if ((getAttributes(RootPath) & FileAttributes.ReparsePoint) != 0)
                throw new InvalidDataException("Recovery paths cannot traverse reparse points.");
        }
        catch (FileNotFoundException) { }
        catch (DirectoryNotFoundException) { }
    }

    public string RootPath { get; }
    public string ScopesPath => UnderRoot("Scopes");
    public string ScopePath(Guid scopeId)
    {
        RequireIdentity(scopeId, nameof(scopeId));
        return UnderRoot("Scopes", scopeId.ToString("N"));
    }

    public string AreaPath(RecoveryArea area)
    {
        ValidateArea(area);
        if (area.AllowsForeignOwners)
            return area.RootId == Guid.Empty ? UnderRoot(area.Kind.ToString()) : UnderRoot(area.Kind.ToString(), area.RootId.ToString("N"));
        if (area.Kind is RecoveryAreaKind.Rescue or RecoveryAreaKind.Pending && area.DocumentId != Guid.Empty)
            return UnderRoot("Scopes", area.ScopeId.ToString("N"), area.Kind.ToString(), area.DocumentId.ToString("N"));
        return UnderRoot("Scopes", area.ScopeId.ToString("N"), area.Kind.ToString());
    }

    public string CopyPath(RecoveryAddress address, string copy)
    {
        ValidateAddress(address);
        if (copy is not ("a" or "b" or "intent")) throw new ArgumentException("Invalid recovery copy.", nameof(copy));
        var suffix = copy == "intent" ? ".reset.intent" : $".{copy}.json";
        return EnsureSafePath(Path.Combine(AreaPath(address.Area), address.Stem + suffix));
    }

    public string TemporaryPath(RecoveryAddress address, string copy) =>
        EnsureSafePath(CopyPath(address, copy) + $".{Guid.NewGuid():N}.tmp");

    public string UnderRoot(params string[] segments)
    {
        var path = RootPath;
        foreach (var segment in segments)
        {
            if (string.IsNullOrEmpty(segment) || segment is "." or ".." || segment.IndexOfAny(['/', '\\', ':']) >= 0)
                throw new ArgumentException("Invalid recovery path component.", nameof(segments));
            path = Path.Combine(path, segment);
        }
        return EnsureSafePath(path);
    }

    private string EnsureSafePath(string path)
    {
        path = Path.GetFullPath(path);
        var comparison = OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
        if (!string.Equals(path, RootPath, comparison) && !path.StartsWith(RootPath + Path.DirectorySeparatorChar, comparison))
            throw new InvalidDataException("A recovery path escaped its configured root.");
        return path;
    }

    /// <summary>Check an enumerated entry using the attributes its enumeration already returned.</summary>
    public void EnsureSafeEntry(string fullName, FileAttributes attributes)
    {
        EnsureSafePath(fullName);
        if ((attributes & FileAttributes.ReparsePoint) != 0)
            throw new InvalidDataException("Recovery paths cannot traverse reparse points.");
    }

    public static void ValidateArea(RecoveryArea area)
    {
        if (area == null || !Enum.IsDefined(area.Kind)) throw new ArgumentException("Invalid recovery area.", nameof(area));
        if (area.AllowsForeignOwners)
        {
            if (area.ScopeId != Guid.Empty || area.DocumentId != Guid.Empty)
                throw new ArgumentException("A global root cannot masquerade as a scope-local area.", nameof(area));
        }
        else if (area.ScopeId == Guid.Empty || area.RootId != Guid.Empty ||
            area.DocumentId != Guid.Empty && area.Kind is not (RecoveryAreaKind.Rescue or RecoveryAreaKind.Pending))
        {
            throw new ArgumentException("Invalid scope-local recovery area.", nameof(area));
        }
    }

    public static void ValidateAddress(RecoveryAddress address)
    {
        if (address == null || address.Ordinal == 0 || address.OperationId == Guid.Empty)
            throw new ArgumentException("Invalid recovery operation address.", nameof(address));
        ValidateArea(address.Area);
    }

    internal static bool TryParseFileName(RecoveryArea area, string name, out RecoveryAddress address, out string copy, out bool temporary)
    {
        // Names start with RecoveryAddress.Stem, "{Ordinal:D20}-{OperationId:N}".
        const int OrdinalDigits = 20, GuidDigits = 32, StemLength = OrdinalDigits + 1 + GuidDigits;
        address = null;
        copy = null;
        temporary = false;
        if (name == null || name.Length < StemLength || name[OrdinalDigits] != '-' ||
            !ulong.TryParse(name.AsSpan(0, OrdinalDigits), NumberStyles.None, CultureInfo.InvariantCulture, out var ordinal) || ordinal == 0 ||
            !Guid.TryParseExact(name.AsSpan(OrdinalDigits + 1, GuidDigits), "N", out var operation) || operation == Guid.Empty)
        {
            return false;
        }

        var suffix = name.Substring(StemLength);
        if (suffix.StartsWith(".a.json", StringComparison.Ordinal)) { copy = "a"; suffix = suffix.Substring(".a.json".Length); }
        else if (suffix.StartsWith(".b.json", StringComparison.Ordinal)) { copy = "b"; suffix = suffix.Substring(".b.json".Length); }
        else if (suffix.StartsWith(".reset.intent", StringComparison.Ordinal)) { copy = "intent"; suffix = suffix.Substring(".reset.intent".Length); }
        else
        {
            return false;
        }

        if (suffix.Length != 0)
        {
            // Temporary copies append ".{Guid:N}.tmp".
            if (suffix.Length != 1 + GuidDigits + ".tmp".Length || suffix[0] != '.' || !suffix.EndsWith(".tmp", StringComparison.Ordinal) ||
                !Guid.TryParseExact(suffix.AsSpan(1, GuidDigits), "N", out _))
            {
                return false;
            }

            temporary = true;
        }
        address = new RecoveryAddress(area, ordinal, operation);
        return true;
    }

    private static void RequireIdentity(Guid identity, string parameter)
    {
        if (identity == Guid.Empty) throw new ArgumentException("An identity is required.", parameter);
    }
}
