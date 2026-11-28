// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Settings;

namespace NotepadsIntegrationTests;

internal static class MarkdownYamlFixture
{
    internal const string BodyHeading = "Body after front matter";
    internal const string FrontMatter = "---\rtitle: YAML fixture\rpublished: true\rtags: [native, preview]\r" +
        "description: |\r  A multiline value with Ω and 中文.\r  The source stays unchanged.\r" +
        "literal: '[link](https://example.test) <b>text</b> $x$'\r---\r\r";
    internal const string InvalidFrontMatter = "---\rtitle: [unterminated\r---\r\r";
    internal const string Body = "# " + BodyHeading + "\r\rAn ordinary **body** paragraph and $x^2$ formula.\r\r" +
        "<div>HTML body remains visible.</div>\r\r" +
        "~~~yaml\rservice:\r  enabled: true\r  count: 42\r  label: \"YAML fence\"\r~~~\r\r" +
        "~~~yml\r- native\r- preview\r~~~\r\r## Tail heading\r\rThe final body paragraph.\r";
    internal static string Source(bool invalid = false) => (invalid ? InvalidFrontMatter : FrontMatter) + Body;
}

internal sealed partial class MarkdownKeyboardFixture
{
    private bool _yamlPreferenceSaved;
    private object _yamlStoredPreference;

    internal async Task PrepareYamlAsync(string scenario)
    {
        if (scenario is not ("table" or "codeBlock" or "hide" or "invalid"))
            throw new ArgumentException("Declared YAML fixture modes only.");
        if (!_yamlPreferenceSaved)
        {
            _yamlStoredPreference = ApplicationSettingsStore.Read(ApplicationPreferences.MarkdownFrontMatterStyleKey);
            _yamlPreferenceSaved = true;
        }
        ApplicationPreferences.MarkdownFrontMatterView = scenario switch
        {
            "codeBlock" => MarkdownFrontMatterStyle.CodeBlock,
            "hide" => MarkdownFrontMatterStyle.Hide,
            _ => MarkdownFrontMatterStyle.Table
        };
        _editor.TestNativeCore.SetText(MarkdownYamlFixture.Source(scenario == "invalid"));
        _editor.TestNativeCore.TestFeatureNative.SetEmptySelection(0);
        if (!_editor.TestMarkdownPanelOpen) _editor.ShowHideContentPreview();
        var extension = await MarkdownFeatureTests.CurrentAsync(_editor);
        extension.FollowSource = false;
        extension.PreviewView.TrySetVerticalScrollProgress(0);
    }

    private void RestoreYamlPreference()
    {
        if (!_yamlPreferenceSaved) return;
        if (_yamlStoredPreference == null) ApplicationSettingsStore.Remove(ApplicationPreferences.MarkdownFrontMatterStyleKey);
        else ApplicationSettingsStore.Write(ApplicationPreferences.MarkdownFrontMatterStyleKey, _yamlStoredPreference);
        ApplicationPreferences.RefreshMarkdownFrontMatterStyle();
        _yamlPreferenceSaved = false;
    }
}
