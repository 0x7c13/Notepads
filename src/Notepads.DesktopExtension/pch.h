#pragma once
#pragma comment(lib, "shell32")
#include <iostream>
#include <sstream>
#include <Windows.h>
#include <shellapi.h>
#include <winrt/Windows.ApplicationModel.h>
#include <winrt/Windows.ApplicationModel.AppService.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Storage.h>

// These values depend upon constant fields described in ..\Notepads\Settings\SettingsKey.cs.
// Changing value in one place require change in another.
constexpr LPCTSTR PackageSidStr = L"PackageSidStr";
constexpr LPCTSTR AdminPipeConnectionNameStr = L"NotepadsAdminWritePipe";
constexpr LPCTSTR InteropCommandLabel = L"Command";
constexpr LPCTSTR InteropCommandAdminCreatedLabel = L"AdminCreated";
constexpr LPCTSTR RegisterExtensionCommandStr = L"RegisterExtension";
constexpr LPCTSTR CreateElevetedExtensionCommandStr = L"CreateElevetedExtension";
constexpr LPCTSTR ExitAppCommandStr = L"ExitApp";