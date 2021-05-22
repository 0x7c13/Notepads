#include "pch.h"
#include "logger.h"
#include "appcenter.h"

using namespace std;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::System;
using namespace Windows::System::UserProfile;

void exit_app(int code)
{
    uninit_apartment();
    exit(code);
}

bool is_first_instance(hstring mutex_name)
{
    auto result = true;

    auto hMutex = handle(OpenMutex(MUTEX_ALL_ACCESS, FALSE, mutex_name.c_str()));
    if (!hMutex)
    {
        try
        {
            check_bool(CreateMutex(nullptr, FALSE, mutex_name.c_str()));
        }
        catch (hresult_error const& e)
        {
            winrt_error error{ e, true, 3 };
            report::dictionary properties{};
            report::add_device_metadata(properties);
            logger::log_error(error);
            crashes::track_error(error, properties);
            analytics::track_event("OnExtensionInstanceDetectionFailed", properties);
            exit_app(e.code());
        }

#ifdef _DEBUG
        logger::start(mutex_name == EXTENSION_MUTEX_NAME);
#endif
    }
    else
    {
        result = false;
        logger::print(L"Closing this instance as another instance is already running.\n", 3000);
        exit_app();
    }

    return result;
}

bool is_elevated_process()
{
    try
    {
        handle h_token;
        check_bool(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, h_token.put()));
        TOKEN_ELEVATION elevation;
        auto size = static_cast<DWORD>(sizeof(elevation));
        check_bool(GetTokenInformation(h_token.get(), TokenElevation, &elevation, sizeof(elevation), &size));
        return elevation.TokenIsElevated;
    }
    catch (hresult_error const& e)
    {
        winrt_error error{ e, true, 3 };
        report::dictionary properties{};
        report::add_device_metadata(properties);
        crashes::track_error(error, properties);
        analytics::track_event("OnPrivilageDetectionFailed", properties);
        return false;
    }
}

#ifndef _DEBUG
int APIENTRY wWinMain(
    _In_ HINSTANCE /* hInstance */,
    _In_opt_ HINSTANCE /* hPrevInstance */,
    _In_ LPWSTR /* lpCmdLine */,
    _In_ int /* nCmdShow */
)
#else
int main()
#endif
{
    init_apartment();
    appcenter::start();

    if (is_elevated_process())
    {
        initialize_elevated_service();
    }
    else
    {
        initialize_extension_service();
    }

    uninit_apartment();
}