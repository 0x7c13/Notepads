param (
  [string]$manifest_file = $(throw "-manifest_file=<path to manifest> is required"),
  [string]$output_path = $(throw "-output_path=<path to output directory> is required"),
  [string]$config = "Debug"
)

[xml]$manifest = Get-Content $manifest_file

if ($config -eq "Debug" -Or $config -eq "Release") {
    $manifest.Package.Identity.Name="Notepads-Dev"
    $manifest.Package.Applications.Application.VisualElements.DisplayName="Notepads-Dev"
    $manifest.Package.Applications.Application.Extensions.Extension.AppExecutionAlias.ExecutionAlias.Alias="Notepads-Dev.exe"
} elseif ($config -eq "Production") {
    $manifest.Package.Identity.Name="Notepads"
    $manifest.Package.Applications.Application.VisualElements.DisplayName="Notepads"
    $manifest.Package.Applications.Application.Extensions.Extension.AppExecutionAlias.ExecutionAlias.Alias="Notepads.exe"
}

$generated_manifest_file = "$($output_path.TrimEnd("\"))\Package.appxmanifest"
$manifest.Save($generated_manifest_file)
$generated_manifest_file