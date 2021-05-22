#pragma once
#pragma comment(lib, "shell32")
#include "iostream"
#include "sstream"
#include "windows.h"
#include "shlobj_core.h"
#include "winrt/Windows.ApplicationModel.h"
#include "winrt/Windows.ApplicationModel.Core.h"
#include "winrt/Windows.Foundation.h"
#include "winrt/Windows.Foundation.Collections.h"
#include "winrt/Windows.Globalization.DateTimeFormatting.h"
#include "winrt/Windows.Security.ExchangeActiveSyncProvisioning.h"
#include "winrt/Windows.Storage.h"
#include "winrt/Windows.Storage.AccessCache.h"
#include "winrt/Windows.System.h"
#include "winrt/Windows.System.Profile.h"
#include "winrt/Windows.System.UserProfile.h"
#include "winrt/Notepads.Core.h"
#include "fmt/core.h"

#define EXTENSION_MUTEX_NAME L"ExtensionMutex"
#define ELEVATED_MUTEX_NAME L"ElevatedMutex"

#define PIPE_READ_BUFFER 2 * MAX_PATH + 10
#define PIPE_NAME_FORMAT L"\\\\.\\pipe\\Sessions\\{}\\AppContainerNamedObjects\\{}\\{}"
#define NAMED_OBJECT_FORMAT L"AppContainerNamedObjects\\{}\\{}"

typedef unsigned long uint48_t;
typedef winrt::Notepads::Core::CoreKey CoreKey;

#define PACKAGE_SID_STR CoreKey::PackageSidStr().c_str()
#define APP_CENTER_INSTALL_ID_STR CoreKey::AppCenterInstallIdStr().c_str()
#define LAST_CHANGED_SETTINGS_KEY_STR CoreKey::LastChangedSettingsKeyStr().c_str()
#define LAST_CHANGED_SETTINGS_APP_INSTANCE_ID_STR CoreKey::LastChangedSettingsAppInstanceIdStr().c_str()
#define LAUNCH_ELEVATED_PROCESS_SUCCESS_STR CoreKey::LaunchElevatedProcessSuccessStr().c_str()
#define LAUNCH_ELEVATED_PROCESS_FAILED_STR CoreKey::LaunchElevatedProcessFailedStr().c_str()
#define EXTENSION_PROCESS_LIFETIME_OBJ_NAME_STR CoreKey::ExtensionProcessLifetimeObjNameStr().c_str()
#define ELEVATED_PROCESS_LIFETIME_OBJ_NAME_STR CoreKey::ElevatedProcessLifetimeObjNameStr().c_str()
#define EXTENSION_UNBLOCK_EVENT_NAME_STR CoreKey::ExtensionUnblockEventNameStr().c_str()
#define ELEVATED_WRITE_EVENT_NAME_STR CoreKey::ElevatedWriteEventNameStr().c_str()
#define ELEVATED_RENAME_EVENT_NAME_STR CoreKey::ElevatedRenameEventNameStr().c_str()
#define EXTENSION_UNBLOCK_PIPE_CONNECTION_NAME_STR CoreKey::ExtensionUnblockPipeConnectionNameStr().c_str()
#define ELEVATED_WRITE_PIPE_CONNECTION_NAME_STR CoreKey::ElevatedWritePipeConnectionNameStr().c_str()
#define ELEVATED_RENAME_PIPE_CONNECTION_NAME_STR CoreKey::ElevatedRenamePipeConnectionNameStr().c_str()

bool is_elevated_process();
bool is_first_instance(winrt::hstring mutexName);
void initialize_extension_service();
void initialize_elevated_service();
void exit_app(int code = 0);