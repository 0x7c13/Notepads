param (
  [string]$project_dir = $(throw "-project_dir=<path to project> is required"),
  [string]$platform = "x86"
)

$project_dir = Resolve-Path $project_dir.TrimEnd("\")
$appcenter_file = "${project_dir}\appcenter.json"
$manifest_file = "${project_dir}\..\Notepads.Package\Package.appxmanifest"
$uwp_project_file = "${project_dir}\..\Notepads\Notepads.csproj"

$appcenter_format = Get-Content $appcenter_file -raw | ConvertFrom-Json
[xml]$manifest = Get-Content $manifest_file
[xml]$sdk_target = Get-Content $uwp_project_file

# Keep version string in format json same as package version
$appcenter_format.logs.device | ForEach-Object {$_.appVersion = $manifest.Package.Identity.Version}
$appcenter_format.logs.device | ForEach-Object {$_.appBuild = $manifest.Package.Identity.Version}

# Keep appcenter sdk version same as nuget package version
$appcenter_sdk_version = $appcenter_format.logs.device.sdkVersion[0]
$sdk_target.Project.ItemGroup.PackageReference | ForEach-Object {if($_.Include -eq 'Microsoft.AppCenter.Crashes'){$appcenter_sdk_version = $_.Version}}
$appcenter_format.logs.device | ForEach-Object {$_.sdkVersion = $appcenter_sdk_version}

$appcenter_format | ConvertTo-Json -depth 10| Set-Content $appcenter_file

# Set up vcpkg packages
$vcpkg_root_dir = "${project_dir}\..\..\vcpkg"

$vcpkg_triplet = "${platform}-windows"
If ($platform -eq "Win32") {
  $vcpkg_triplet = "x86-windows"
}

& ${vcpkg_root_dir}\vcpkg install rapidjson curl --triplet=$vcpkg_triplet