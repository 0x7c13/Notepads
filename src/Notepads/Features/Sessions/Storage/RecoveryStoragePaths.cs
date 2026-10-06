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
    private readonly Func<string, FileAttributes> getAttributes;

    public RecoveryStoragePaths(string rootPath) : this(rootPath, File.GetAttributes)
    {
    }

    internal RecoveryStoragePaths(string rootPath, Func<string, FileAttributes> getAttributes)
    {
        if (string.IsNullOrWhiteSpace(rootPath)) throw new ArgumentException("A recovery root is required.", nameof(rootPath));
        ArgumentNullException.ThrowIfNull(getAttributes);
        this.getAttributes = getAttributes;
        RootPath = Path.TrimEndingDirectorySeparator(Path.GetFullPath(rootPath));
        EnsureSafePath(RootPath);
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

    public string EnsureSafePath(string path)
    {
        path = Path.GetFullPath(path);
        var comparison = OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
        if (!string.Equals(path, RootPath, comparison) && !path.StartsWith(RootPath + Path.DirectorySeparatorChar, comparison))
            throw new InvalidDataException("A recovery path escaped its configured root.");
        // The OS-supplied app-private root is our trust boundary. Inspect that root and
        // its descendants; a packaged process has no authority to inspect its ancestors.
        var current = path;
        while (current != null)
        {
            try
            {
                if ((getAttributes(current) & FileAttributes.ReparsePoint) != 0)
                    throw new InvalidDataException("Recovery paths cannot traverse reparse points.");
            }
            catch (FileNotFoundException) { }
            catch (DirectoryNotFoundException) { }
            if (string.Equals(current, RootPath, comparison)) break;
            current = Path.GetDirectoryName(current);
        }
        return path;
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
        address = null;
        copy = null;
        temporary = false;
        if (name == null || name.Length < 53 || name[20] != '-' ||
            !ulong.TryParse(name.AsSpan(0, 20), NumberStyles.None, CultureInfo.InvariantCulture, out var ordinal) || ordinal == 0 ||
            !Guid.TryParseExact(name.AsSpan(21, 32), "N", out var operation) || operation == Guid.Empty)
        {
            return false;
        }

        var suffix = name.Substring(53);
        if (suffix.StartsWith(".a.json", StringComparison.Ordinal)) { copy = "a"; suffix = suffix.Substring(7); }
        else if (suffix.StartsWith(".b.json", StringComparison.Ordinal)) { copy = "b"; suffix = suffix.Substring(7); }
        else if (suffix.StartsWith(".reset.intent", StringComparison.Ordinal)) { copy = "intent"; suffix = suffix.Substring(13); }
        else
        {
            return false;
        }

        if (suffix.Length != 0)
        {
            if (suffix.Length != 37 || suffix[0] != '.' || !suffix.EndsWith(".tmp", StringComparison.Ordinal) ||
                !Guid.TryParseExact(suffix.AsSpan(1, 32), "N", out _))
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
