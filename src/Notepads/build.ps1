param (
  [string]$project_manifest_file = $(throw "-project_manifest_file=<path to project manifest> is required"),
  [string]$config = "Debug"
)

[xml]$manifest = Get-Content $project_manifest_file

if ($config -eq "Debug" -Or $config -eq "Release") {
    $manifest.Package.Identity.Name="Notepads-Dev"
    $manifest.Package.Applications.Application.VisualElements.DisplayName="Notepads-Dev"
    $manifest.Package.Applications.Application.Extensions.Extension.AppExecutionAlias.ExecutionAlias.Alias="Notepads-Dev.exe"
} elseif ($config -eq "Production") {
    $manifest.Package.Identity.Name="Notepads"
    $manifest.Package.Applications.Application.VisualElements.DisplayName="Notepads"
    $manifest.Package.Applications.Application.Extensions.Extension.AppExecutionAlias.ExecutionAlias.Alias="Notepads.exe"
}

$generated_manifest_file = "$([System.IO.Path]::GetDirectoryName($project_manifest_file))\obj\Package.appxmanifest"
$manifest.Save($generated_manifest_file)
$generated_manifest_file