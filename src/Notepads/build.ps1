param (
  [string]$project_dir = $(throw "-project_dir=<path to project> is required"),
  [string]$config = "Debug",
  [string]$target = "PostBuild"
)

$project_dir = Resolve-Path $project_dir.TrimEnd("\")
$manifest_file = "${project_dir}\Package.appxmanifest"

[xml]$manifest = Get-Content $manifest_file

if ($config -eq "Debug" -Or $config -eq "Release") {
    if ($target -eq "PreBuild") {
        $manifest.Package.Identity.Name="Notepads-Dev"
        $manifest.Package.Applications.Application.VisualElements.DisplayName="Notepads-Dev"
        $manifest.Package.Applications.Application.Extensions.Extension.AppExecutionAlias.ExecutionAlias.Alias="Notepads-Dev.exe"
    } elseif ($target -eq "PostBuild") {
        $manifest.Package.Identity.Name="Notepads"
        $manifest.Package.Applications.Application.VisualElements.DisplayName="Notepads"
        $manifest.Package.Applications.Application.Extensions.Extension.AppExecutionAlias.ExecutionAlias.Alias="Notepads.exe"
    }
} elseif ($config -eq "Production") {
    if ($target -eq "PreBuild") {
        $manifest.Package.Identity.Name="Notepads"
        $manifest.Package.Applications.Application.VisualElements.DisplayName="Notepads"
        $manifest.Package.Applications.Application.Extensions.Extension.AppExecutionAlias.ExecutionAlias.Alias="Notepads.exe"
    } elseif ($target -eq "PostBuild") {
        $manifest.Package.Identity.Name="Notepads-Dev"
        $manifest.Package.Applications.Application.VisualElements.DisplayName="Notepads-Dev"
        $manifest.Package.Applications.Application.Extensions.Extension.AppExecutionAlias.ExecutionAlias.Alias="Notepads-Dev.exe"
    }
}

$manifest.Save("${manifest_file}")