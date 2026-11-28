// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;

namespace NotepadsIntegrationTests;

internal sealed partial class MarkdownKeyboardFixture
{
    internal Task PrepareMermaidAsync(string scenario) => PrepareMermaidSourceAsync(scenario switch
    {
        "single" => MarkdownMermaidTests.SingleSource,
        "mixed" => MarkdownMermaidTests.MixedSource,
        "diagrams" => MarkdownMermaidTests.DiagramSource,
        _ => throw new ArgumentException("Declared Mermaid fixture scenarios only.")
    });

    internal Task PrepareSourceTextAsync(string encoded)
    {
        const int maximumBytes = 1024 * 1024;
        if (encoded == null || encoded.Length > ((maximumBytes + 2) / 3) * 4)
            throw new ArgumentException("The isolated Markdown source fixture accepts at most 1 MiB of base64 UTF8 input.");
        var bytes = Convert.FromBase64String(encoded);
        if (bytes.Length > maximumBytes) throw new ArgumentException("The isolated Markdown source exceeds 1 MiB.");
        return PrepareMermaidSourceAsync(new UTF8Encoding(false, true).GetString(bytes));
    }

    private async Task PrepareMermaidSourceAsync(string source)
    {
        _editor.TestNativeCore.SetText(source);
        _editor.TestNativeCore.TestFeatureNative.SetEmptySelection(0);
        if (!_editor.TestMarkdownPanelOpen) _editor.ShowHideContentPreview();
        var extension = await MarkdownFeatureTests.CurrentAsync(_editor);
        extension.FollowSource = false;
        extension.PreviewView.TrySetVerticalScrollProgress(0);
    }

    // Only current public UIA observations are exported. The regular fixture
    // Snapshot already exports source bytes separately; this compact reply does
    // not duplicate document text or introduce a production test hook.
    internal string DerivedStateSnapshot()
    {
        var preview = _editor.TestMarkdownPreview?.PreviewView;
        var objects = MarkdownMermaidTests.ReadObjects(preview);
        using var bytes = new MemoryStream();
        using (var json = new Utf8JsonWriter(bytes))
        {
            json.WriteStartObject();
            json.WriteNumber("schema", 1);
            json.WriteBoolean("current", _editor.TestMarkdownPreview?.SourceSession?.IsCurrent == true);
            json.WriteBoolean("surfaceReady", preview?.IsSurfaceReady == true);
            json.WriteNumber("formulaCount", objects.Count(item => item.Family == "Formula"));
            json.WriteNumber("diagramCount", objects.Count(item => item.Family == "Diagram"));
            json.WriteStartObject("states");
            foreach (var group in objects.GroupBy(item => item.Family + "." + item.State))
                json.WriteNumber(group.Key, group.Count());
            json.WriteEndObject();
            json.WriteStartArray("objects");
            for (var index = 0; index < objects.Length; ++index)
            {
                var item = objects[index];
                json.WriteStartObject();
                json.WriteNumber("index", index);
                json.WriteString("family", item.Family);
                json.WriteString("state", item.State);
                json.WriteStartArray("bounds");
                foreach (var value in new[] { item.Bounds.X, item.Bounds.Y, item.Bounds.Width, item.Bounds.Height })
                {
                    if (double.IsFinite(value)) json.WriteNumberValue(value);
                    else json.WriteNullValue();
                }
                json.WriteEndArray();
                json.WriteEndObject();
            }
            json.WriteEndArray();
            json.WriteEndObject();
        }
        return "markdownDerivedJson=" + Encoding.UTF8.GetString(bytes.ToArray()) + "\n";
    }
}
