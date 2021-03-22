#pragma once
#pragma comment(lib, "shell32")
#include "iostream"
#include "sstream"
#include "windows.h"
#include "shellapi.h"
#include "shlobj_core.h"
#include "winrt/Windows.ApplicationModel.h"
#include "winrt/Windows.ApplicationModel.Core.h"
#include "winrt/Windows.Foundation.h"
#include "winrt/Windows.Foundation.Collections.h"
#include "winrt/Windows.Security.ExchangeActiveSyncProvisioning.h"
#include "winrt/Windows.Storage.h"
#include "winrt/Windows.Storage.AccessCache.h"
#include "winrt/Windows.System.h"
#include "winrt/Windows.System.Profile.h"
#include "winrt/Notepads.Core.h"
#include "boost/stacktrace.hpp"
#include "fmt/core.h"

#define ExtensionMutexName L"ExtensionMutex"
#define ElevatedMutexName L"ElevatedMutex"

#define PIPE_READ_BUFFER 2 * MAX_PATH + 10
#define PIPE_NAME_FORMAT L"\\\\.\\pipe\\Sessions\\{}\\AppContainerNamedObjects\\{}\\{}"
#define NAMED_OBJECT_FORMAT L"AppContainerNamedObjects\\{}\\{}"

#define AppCenterSecret winrt::Notepads::Core::CoreKey::AppCenterSecret().c_str()
#define PackageSidStr winrt::Notepads::Core::CoreKey::PackageSidStr().c_str()
#define AppCenterInstallIdStr winrt::Notepads::Core::CoreKey::AppCenterInstallIdStr().c_str()
#define LastChangedSettingsKeyStr winrt::Notepads::Core::CoreKey::LastChangedSettingsKeyStr().c_str()
#define LastChangedSettingsAppInstanceIdStr winrt::Notepads::Core::CoreKey::LastChangedSettingsAppInstanceIdStr().c_str()
#define LaunchElevatedProcessSuccessStr winrt::Notepads::Core::CoreKey::LaunchElevatedProcessSuccessStr().c_str()
#define LaunchElevatedProcessFailedStr winrt::Notepads::Core::CoreKey::LaunchElevatedProcessFailedStr().c_str()
#define ExtensionProcessLifetimeObjNameStr winrt::Notepads::Core::CoreKey::ExtensionProcessLifetimeObjNameStr().c_str()
#define ElevatedProcessLifetimeObjNameStr winrt::Notepads::Core::CoreKey::ElevatedProcessLifetimeObjNameStr().c_str()
#define ExtensionUnblockEventNameStr winrt::Notepads::Core::CoreKey::ExtensionUnblockEventNameStr().c_str()
#define ElevatedWriteEventNameStr winrt::Notepads::Core::CoreKey::ElevatedWriteEventNameStr().c_str()
#define ElevatedRenameEventNameStr winrt::Notepads::Core::CoreKey::ElevatedRenameEventNameStr().c_str()
#define ExtensionUnblockPipeConnectionNameStr winrt::Notepads::Core::CoreKey::ExtensionUnblockPipeConnectionNameStr().c_str()
#define ElevatedWritePipeConnectionNameStr winrt::Notepads::Core::CoreKey::ElevatedWritePipeConnectionNameStr().c_str()
#define ElevatedRenamePipeConnectionNameStr winrt::Notepads::Core::CoreKey::ElevatedRenamePipeConnectionNameStr().c_str()

VOID setExceptionHandling();
bool isElevatedProcess();
bool isFirstInstance(LPCTSTR mutexName);
VOID initializeExtensionService();
VOID initializeElevatedService();
VOID exitApp();

VOID printDebugMessage([[maybe_unused]] LPCTSTR message, [[maybe_unused]] DWORD sleepTime = 0);
std::string getTimeStamp();
std::string getTimeStamp(const CHAR* format);
std::string to_string(winrt::Windows::System::ProcessorArchitecture arch);
std::string base64_encode(const std::string& in);

winrt::Windows::Foundation::IInspectable readSettingsKey(winrt::hstring key);
std::pair<DWORD, std::wstring> getLastErrorDetails();
winrt::Windows::Foundation::IAsyncAction logLastError(bool isFatal);
winrt::fire_and_forget initializeLogging(LPCTSTR trailStr);