#include "pch.h"
#include "logger.h"
#include "appcenter.h"

using namespace std;
using namespace winrt;

void exit_app(int code)
{
    uninit_apartment();
    exit(code);
}

bool is_first_instance(hstring mutex_name)
{
    auto result = true;

    handle h_mutex{ OpenMutexW(MUTEX_ALL_ACCESS, false, mutex_name.c_str()) };
    if (!h_mutex)
    {
        try
        {
            check_bool(CreateMutexW(nullptr, false, mutex_name.c_str()));
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
        logger::log_info(L"Closing this instance as another instance is already running.", true);
        logger::print(L"", 3000);
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