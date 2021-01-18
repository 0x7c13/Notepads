#pragma once
#pragma comment(lib, "shell32")
#include "iostream"
#include "sstream"
#include "windows.h"
#include "shellapi.h"
#include "shlobj_core.h"
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

#define ExtensionMutexName L"ExtensionMutex"
#define ElevatedMutexName L"ElevatedMutex"

#define PIPE_READ_BUFFER 2 * MAX_PATH + 10
#define PIPE_NAME_FORMAT L"\\\\.\\pipe\\Sessions\\{}\\AppContainerNamedObjects\\{}\\{}"
#define NAMED_OBJECT_FORMAT L"AppContainerNamedObjects\\{}\\{}"

// These values depend upon constant fields described in ..\Notepads\Settings\SettingsKey.cs.
// Changing value in one place require changing variable with similar name in another.
/////////////////////////////////////////////////////////////////////////////////////////////
#define AppCenterSecret NULL
#define PackageSidStr L"PackageSidStr"
#define AppCenterInstallIdStr L"AppCenterInstallIdStr"
#define LastChangedSettingsKeyStr L"LastChangedSettingsKeyStr"
#define LastChangedSettingsAppInstanceIdStr L"LastChangedSettingsAppInstanceIdStr"
#define LaunchElevetedProcessSuccessStr L"LaunchElevetedProcessSuccess"
#define LaunchElevetedProcessFailedStr L"LaunchElevetedProcessFailed"
#define ExtensionProcessLifetimeObjNameStr L"ExtensionProcessLifetimeObj"
#define ElevatedProcessLifetimeObjNameStr L"ElevatedProcessLifetimeObj"
#define ExtensionUnblockEventNameStr L"NotepadsExtensionUnblockEvent"
#define ElevatedWriteEventNameStr L"NotepadsElevatedWriteEvent"
#define ElevatedRenameEventNameStr L"NotepadsElevatedRenameEvent"
#define ExtensionUnblockPipeConnectionNameStr L"NotepadsExtensionUnblockPipe"
#define ElevatedWritePipeConnectionNameStr L"NotepadsElevatedWritePipe"
#define ElevatedRenamePipeConnectionNameStr L"NotepadsElevatedRenamePipe"
/////////////////////////////////////////////////////////////////////////////////////////////

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