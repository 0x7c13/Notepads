#pragma once
#pragma comment(lib, "shell32")
#define STRICT
#define STRICT_TYPED_ITEMIDS
#include "iostream"
#include "sstream"
#include "windows.h"
#include "shellapi.h"
#include "winrt/Windows.ApplicationModel.h"
#include "winrt/Windows.ApplicationModel.Core.h"
#include "winrt/Windows.ApplicationModel.DataTransfer.h"
#include "winrt/Windows.Foundation.h"
#include "winrt/Windows.Foundation.Collections.h"
#include "winrt/Windows.Security.ExchangeActiveSyncProvisioning.h"
#include "winrt/Windows.Storage.h"
#include "winrt/Windows.System.h"
#include "winrt/Windows.System.Profile.h"
#include "boost/stacktrace.hpp"
#include "fmt/core.h"

// These values depend upon constant fields described in ..\Notepads\Settings\SettingsKey.cs.
// Changing value in one place require changing variable with similar name in another.
/////////////////////////////////////////////////////////////////////////////////////////////
constexpr LPCTSTR AppCenterSecret = NULL;
constexpr LPCTSTR AppCenterInstallIdStr = L"AppCenterInstallIdStr";
constexpr LPCTSTR LastChangedSettingsKeyStr = L"LastChangedSettingsKeyStr";
constexpr LPCTSTR LastChangedSettingsAppInstanceIdStr = L"LastChangedSettingsAppInstanceIdStr";
constexpr LPCTSTR LaunchElevetedProcessSuccessStr = L"LaunchElevetedProcessSuccess";
constexpr LPCTSTR LaunchElevetedProcessFailedStr = L"LaunchElevetedProcessFailed";
constexpr LPCTSTR DesktopExtensionLifetimeObjNameStr = L"DesktopExtensionLifetimeObj";
constexpr LPCTSTR PackageSidStr = L"PackageSidStr";
constexpr LPCTSTR AdminWriteEventNameStr = L"NotepadsAdminWriteEvent";
constexpr LPCTSTR AdminWritePipeConnectionNameStr = L"NotepadsAdminWritePipe";
constexpr LPCTSTR AdminRenameEventNameStr = L"NotepadsAdminRenameEvent";
constexpr LPCTSTR AdminRenamePipeConnectionNameStr = L"NotepadsAdminRenamePipe";
/////////////////////////////////////////////////////////////////////////////////////////////

bool isElevatedProcess();
VOID setExceptionHandling();
VOID exitApp();

VOID printDebugMessage([[maybe_unused]] LPCTSTR message, [[maybe_unused]] DWORD sleepTime = 0);
std::string getTimeStamp();
std::string getTimeStamp(const CHAR* format);
std::string to_string(winrt::Windows::System::ProcessorArchitecture arch);
std::string base64_encode(const std::string& in);

VOID initializeAdminService();
winrt::Windows::Foundation::IInspectable readSettingsKey(winrt::hstring key);

std::pair<DWORD, std::wstring> getLastErrorDetails();
winrt::Windows::Foundation::IAsyncAction logLastError(bool isFatal);
winrt::fire_and_forget initializeLogging(LPCTSTR trailStr);