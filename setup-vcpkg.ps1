param (
  [string]$vcpkg_root = $(throw "-vcpkg_root=<path to vcpkg> is required")
)

git submodule update --init --recursive

# Set up vcpkg
If (!(Test-Path -Path "${vcpkg_root}\vcpkg.exe")) {
  & ${vcpkg_root}\bootstrap-vcpkg.bat
}

& ${vcpkg_root}\vcpkg integrate install