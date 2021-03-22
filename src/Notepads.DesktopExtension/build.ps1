param (
  [string]$project_dir = $(throw "-project_dir=<path to project> is required")
)

$project_dir = Resolve-Path $project_dir.TrimEnd("\")
$appcenter_file = "${project_dir}\appcenter-sdk-version.txt"
$uwp_project_file = "${project_dir}\..\Notepads\Notepads.csproj"

# Keep appcenter sdk version same as AppCenter nuget package version
[xml]$sdk_target = Get-Content $uwp_project_file
$sdk_target.Project.ItemGroup.PackageReference | ForEach-Object {if($_.Include -eq 'Microsoft.AppCenter.Crashes'){$_.Version | Set-Content $appcenter_file  -NoNewline}}