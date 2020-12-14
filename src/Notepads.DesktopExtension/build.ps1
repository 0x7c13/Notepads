param (
  [string]$project_dir = $(throw "-project_dir=<path to project> is required")
)

$project_dir = Resolve-Path $project_dir.TrimEnd("\")
$appcenter_file = "${project_dir}\appcenter.json"
$manifest_file = "${project_dir}\..\Notepads.Package\Package.appxmanifest"
$uwp_project_file = "${project_dir}\..\Notepads\Notepads.csproj"

$appcenter_version_data = Get-Content $appcenter_file -raw | ConvertFrom-Json
[xml]$manifest = Get-Content $manifest_file
[xml]$sdk_target = Get-Content $uwp_project_file

# Keep version string in same as package version
$appcenter_version_data.appVersion = $manifest.Package.Identity.Version
$appcenter_version_data.appBuild = $manifest.Package.Identity.Version

# Keep appcenter sdk version same as nuget package version
$sdk_target.Project.ItemGroup.PackageReference | ForEach-Object {if($_.Include -eq 'Microsoft.AppCenter.Crashes'){$appcenter_version_data.sdkVersion = $_.Version}}

$appcenter_version_data | ConvertTo-Json -depth 10| Set-Content $appcenter_file