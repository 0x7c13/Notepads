// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace NotepadsArchitectureTests;

internal static class LayerRules
{
    public static string? Module(string name)
    {
        if (name.StartsWith("Notepads.Presentation.")) return "Presentation";
        foreach (var feature in new[] { "Documents", "Sessions" })
        {
            var prefix = "Notepads.Features." + feature;
            if (name.StartsWith(prefix + "."))
            {
                foreach (var part in new[] { "Contracts", "Text", "Storage", "IO", "Operations", "FileTypes", "Validation", "Recovery", "Transfer", "Legacy" })
                    if (name.StartsWith(prefix + "." + part + ".")) return feature + "." + part;
                return name[(prefix.Length + 1)..].Contains('.') ? "UnknownApp" : feature;
            }
        }
        foreach (var feature in new[] { "Preferences", "WebSearch" })
            if (name.StartsWith("Notepads.Features." + feature + ".")) return feature;
        foreach (var part in new[] { "Diagnostics", "Storage", "Settings", "Threading", "Runtime", "Shell", "Fonts", "Localization", "Resources" })
            if (name.StartsWith("Notepads.Infrastructure." + part + ".")) return "Infrastructure." + part;
        if (name.StartsWith("WinUIEditor.")) return "Native";
        if (name.StartsWith("Notepads.Controls.")) return "ExternalControls";
        if (name.StartsWith("Windows.UI.Xaml.") || name.StartsWith("Microsoft.UI.Xaml.") ||
            name.StartsWith("CommunityToolkit.WinUI."))
        {
            return "XamlPlatform";
        }

        if (name.StartsWith("Notepads.Composition.") || name == "Notepads.App" || name.StartsWith("Notepads.App+") ||
            name == "Notepads.Program" || name.StartsWith("Notepads.Program+") || name.StartsWith("Notepads.Notepads_XamlTypeInfo."))
        {
            return "Composition";
        }

        return name.StartsWith("Notepads.") ? "UnknownApp" : null;
    }

    public static bool Allows(string sourceType, string targetType)
    {
        var source = Module(sourceType);
        var target = Module(targetType);
        if (source == "UnknownApp" || target == "UnknownApp") return false;
        if (target == "XamlPlatform") return source == "Composition" || source == "Presentation";
        if (source == null || target == null || source == target || source == "Composition") return true;
        if (target == "Composition") return false;
        if (target == "Native")
        {
            if (source == "Presentation")
            {
                return sourceType.StartsWith("Notepads.Presentation.Controls.TextEditor.") ||
                    sourceType.StartsWith("Notepads.Presentation.Workspace.") && targetType.Contains("EditorJournalCheckpoint");
            }

            if (source.StartsWith("Documents"))
                return targetType.Contains("EditorJournalCheckpoint") || targetType.Contains("EditorUtf8Reader");
            if (source.StartsWith("Sessions")) return targetType.Contains("EditorJournalCheckpoint");
            return false;
        }
        if (source == "Presentation") return true;
        if (target == "ExternalControls" || target == "Presentation") return false;
        if (source.StartsWith("Infrastructure."))
        {
            if (!target.StartsWith("Infrastructure.")) return false;
            if (source == "Infrastructure.Diagnostics" || source == "Infrastructure.Threading") return false;
            return target == "Infrastructure.Diagnostics" ||
                (source == "Infrastructure.Storage" || source == "Infrastructure.Runtime") && target == "Infrastructure.Threading";
        }
        if (target.StartsWith("Infrastructure.")) return true;
        if (source == "Preferences")
        {
            return target == "Documents.Contracts" || target == "WebSearch" && targetType == "Notepads.Features.WebSearch.SearchEngine" ||
                targetType == "Notepads.Features.Documents.Text.EncodingCatalog";
        }

        if (source == "WebSearch") return false;
        return source switch
        {
            "Documents.Contracts" or "Documents.Operations" => false,
            "Documents.Text" => target == "Documents.Contracts",
            "Documents.Storage" => target is "Documents.Contracts" or "Documents.Text",
            "Documents" => target is "Documents.Contracts" or "Documents.Text" or "Documents.Storage",
            "Documents.IO" => target is "Documents" or "Documents.Contracts" or "Documents.Text" or "Documents.Storage",
            "Documents.FileTypes" => target == "Documents.Contracts",
            "Sessions.Contracts" => target == "Documents.Contracts",
            "Sessions.Validation" => target is "Sessions.Contracts" or "Documents.Contracts",
            "Sessions.Storage" => target is "Sessions.Contracts" or "Sessions.Validation" || target.StartsWith("Documents"),
            "Sessions.Recovery" => target is "Sessions.Contracts" or "Sessions.Validation" or "Sessions.Storage" || target.StartsWith("Documents"),
            "Sessions.Transfer" => target is "Sessions.Contracts" or "Sessions.Validation" or "Sessions.Storage" or "Sessions.Recovery" || target.StartsWith("Documents"),
            "Sessions.Legacy" => target is "Sessions.Contracts" or "Sessions.Storage" or "Sessions.Recovery" || target.StartsWith("Documents"),
            "Sessions" => target.StartsWith("Sessions.") || target.StartsWith("Documents"),
            _ => false
        };
    }
}
