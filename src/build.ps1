param (
  [string]$project_dir = $(throw "-project_dir=<path to project> is required"),
  [string]$event = "PostBuild"
)

$project_dir = Resolve-Path $project_dir.TrimEnd("\")
$manifest_file = "${project_dir}\Package.appxmanifest"

[xml]$manifest = Get-Content $manifest_file
if ($event -eq "PreBuild") {
    $manifest.Package.Identity.Name="Notepads"
    $manifest.Package.Applications.Application.VisualElements.DisplayName="Notepads"
    $manifest.Package.Applications.Application.Extensions.Extension.AppExecutionAlias.ExecutionAlias.Alias="Notepads.exe"
} elseif ($event -eq "PostBuild") {
    $manifest.Package.Identity.Name="Notepads-Dev"
    $manifest.Package.Applications.Application.VisualElements.DisplayName="Notepads-Dev"
    $manifest.Package.Applications.Application.Extensions.Extension.AppExecutionAlias.ExecutionAlias.Alias="Notepads-Dev.exe"
}
$manifest.Save("${manifest_file}")