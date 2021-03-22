#pragma once
#include "CoreKey.g.h"

namespace winrt::Notepads::Core::implementation
{
    struct CoreKey : CoreKeyT<CoreKey>
    {
        CoreKey() = default;

        static hstring AppCenterSecret();
        static hstring PackageSidStr();
        static hstring AppCenterInstallIdStr();
        static hstring LastChangedSettingsKeyStr();
        static hstring LastChangedSettingsAppInstanceIdStr();
        static hstring LaunchElevatedProcessSuccessStr();
        static hstring LaunchElevatedProcessFailedStr();
        static hstring ExtensionProcessLifetimeObjNameStr();
        static hstring ElevatedProcessLifetimeObjNameStr();
        static hstring ExtensionUnblockEventNameStr();
        static hstring ElevatedWriteEventNameStr();
        static hstring ElevatedRenameEventNameStr();
        static hstring ExtensionUnblockPipeConnectionNameStr();
        static hstring ElevatedWritePipeConnectionNameStr();
        static hstring ElevatedRenamePipeConnectionNameStr();
    };
}

namespace winrt::Notepads::Core::factory_implementation
{
    struct CoreKey : CoreKeyT<CoreKey, implementation::CoreKey> { };
}