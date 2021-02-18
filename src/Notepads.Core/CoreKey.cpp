#include "pch.h"
#include "CoreKey.h"
#include "CoreKey.g.cpp"

namespace winrt::Notepads::Core::implementation
{
    hstring CoreKey::AppCenterSecret()
    {
        return APP_CENTER_SECRET;
    }

    hstring CoreKey::PackageSidStr()
    {
        return PACKAGE_SID_STR;
    }

    hstring CoreKey::AppCenterInstallIdStr()
    {
        return APP_CENTER_INSTALL_ID_STR;
    }

    hstring CoreKey::LastChangedSettingsKeyStr()
    {
        return LAST_CHANGED_SETTINGS_KEY_STR;
    }

    hstring CoreKey::LastChangedSettingsAppInstanceIdStr()
    {
        return LAST_CHANGED_SETTINGS_APP_INSTANCE_ID_STR;
    }

    hstring CoreKey::LaunchElevatedProcessSuccessStr()
    {
        return LAUNCH_ELEVATED_PROCESS_SUCCESS_STR;
    }

    hstring CoreKey::LaunchElevatedProcessFailedStr()
    {
        return LAUNCH_ELEVATED_PROCESS_FAILED_STR;
    }

    hstring CoreKey::ExtensionProcessLifetimeObjNameStr()
    {
        return EXTENSION_PROCESS_LIFETIME_OBJ_NAME_STR;
    }

    hstring CoreKey::ElevatedProcessLifetimeObjNameStr()
    {
        return ELEVATED_PROCESS_LIFETIME_OBJ_NAME_STR;
    }

    hstring CoreKey::ExtensionUnblockEventNameStr()
    {
        return EXTENSION_UNBLOCK_EVENT_NAME_STR;
    }

    hstring CoreKey::ElevatedWriteEventNameStr()
    {
        return ELEVATED_WRITE_EVENT_NAME_STR;
    }

    hstring CoreKey::ElevatedRenameEventNameStr()
    {
        return ELEVATED_RENAME_EVENT_NAME_STR;
    }

    hstring CoreKey::ExtensionUnblockPipeConnectionNameStr()
    {
        return EXTENSION_UNBLOCK_PIPE_CONNECTION_NAME_STR;
    }

    hstring CoreKey::ElevatedWritePipeConnectionNameStr()
    {
        return ELEVATED_WRITE_PIPE_CONNECTION_NAME_STR;
    }

    hstring CoreKey::ElevatedRenamePipeConnectionNameStr()
    {
        return ELEVATED_RENAME_PIPE_CONNECTION_NAME_STR;
    }
}