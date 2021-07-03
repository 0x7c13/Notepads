#include "pch.h"
#include "logger.h"
#include "appcenter.h"
#include "settings_key.h"
#include "shellapi.h"

using namespace std;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::Storage;

DWORD session_id;
hstring package_sid;
hstring unblock_pipe;

DWORD WINAPI unblock_file_from_pipe_data(LPVOID /* param */)
{
    hstring file_path{};
    try
    {
        handle extension_unblock_event
        {
            OpenEventW(
                SYNCHRONIZE | EVENT_MODIFY_STATE,
                false,
                std::format(
                    NAMED_OBJECT_FORMAT,
                    package_sid.c_str(),
                    EXTENSION_UNBLOCK_EVENT_NAME_STR
                ).c_str()
            )
        };

        check_bool(!WaitForSingleObject(extension_unblock_event.get(), INFINITE));
        check_bool(ResetEvent(extension_unblock_event.get()));
        check_bool(WaitNamedPipeW(unblock_pipe.c_str(), NMPWAIT_WAIT_FOREVER));

        CreateThread(nullptr, 0, unblock_file_from_pipe_data, nullptr, 0, nullptr);

        handle h_pipe
        {
            CreateFileW(
                unblock_pipe.c_str(),
                GENERIC_READ,
                0,
                nullptr,
                OPEN_EXISTING,
                0,
                nullptr
            )
        };

        check_bool(bool(h_pipe));

        TCHAR read_buffer[PIPE_READ_BUFFER];
        wstringstream pipe_data{};
        auto byte_read = 0UL;
        do
        {
            fill(begin(read_buffer), end(read_buffer), '\0');
            if (ReadFile(h_pipe.get(), read_buffer, (PIPE_READ_BUFFER - 1) * sizeof(TCHAR), &byte_read, nullptr))
            {
                pipe_data << read_buffer;
            }
        } while (byte_read >= (PIPE_READ_BUFFER - 1) * sizeof(TCHAR));

        file_path = pipe_data.str();
        auto p_file = create_instance<IPersistFile>(CLSID_PersistentZoneIdentifier);
        check_hresult(p_file->Load(file_path.c_str(), STGM_READWRITE));

        LPTSTR last_writer_package_family;
        check_hresult(p_file.as<IZoneIdentifier2>()->GetLastWriterPackageFamilyName(&last_writer_package_family));
        check_bool(Package::Current().Id().FamilyName() == last_writer_package_family);
        check_hresult(p_file.as<IZoneIdentifier>()->Remove());
        check_hresult(p_file->Save(file_path.c_str(), true));

        logger::log_info(std::format(L"Successfully unblocked file \"{}\"", file_path.c_str()).c_str(), true);
        logger::log_info(L"Waiting on uwp app to send data.", true);
        return 0;
    }
    catch (hresult_error const& e)
    {
        winrt_error error{ e, true, 3 };

        logger::log_info(
            file_path.empty()
            ? L"Failed to unblock file"
            : std::format(L"Failed to unblock file \"{}\"", file_path.c_str()).c_str(),
            true
        );

        report::dictionary properties{};
        report::add_device_metadata(properties);
        logger::log_error(error);
        crashes::track_error(error, properties);
        analytics::track_event(L"OnFileUnblockFailed", properties);

        exit_app(error.code());
        return error.code();
    }
}

void launch_elevated_process()
{
    report::dictionary properties{};

    auto handle_launch_error = [&](winrt_error const& ex)
    {
        settings_key::write(LAST_CHANGED_SETTINGS_APP_INSTANCE_ID_STR, box_value(L""));
        settings_key::write(LAST_CHANGED_SETTINGS_KEY_STR, box_value(LAUNCH_ELEVATED_PROCESS_FAILED_STR));

        logger::log_info(L"Launching of Elevated Process was cancelled.", true);
        properties.insert(
            properties.end(),
            {
                pair(L"Denied", L"True"),
                pair(L"Error Code", to_hstring(ex.code())),
                pair(L"Error Message", ex.message())
            }
        );
    };

    try
    {
        wstring file_name{ MAX_PATH, '\0' };

    GetModulePath:
        auto result = GetModuleFileNameW(nullptr, &file_name[0], static_cast<ULONG>(file_name.size()));
        check_bool(result);
        if (result == file_name.size() && GetLastError() == ERROR_INSUFFICIENT_BUFFER)
        {
            file_name.resize(2 * file_name.size(), '\0');
            goto GetModulePath;
        }

        SHELLEXECUTEINFO sh_ex_info
        {
            .cbSize = sizeof(SHELLEXECUTEINFO),
            .fMask = SEE_MASK_NOCLOSEPROCESS,
            .hwnd = 0,
            .lpVerb = L"runas",
            .lpFile = file_name.c_str(),
            .lpParameters = L"",
            .lpDirectory = 0,
            .nShow = SW_SHOW,
            .hInstApp = 0
        };

        if (ShellExecuteExW(&sh_ex_info))
        {
            settings_key::write(LAST_CHANGED_SETTINGS_APP_INSTANCE_ID_STR, box_value(L""));
            settings_key::write(LAST_CHANGED_SETTINGS_KEY_STR, box_value(LAUNCH_ELEVATED_PROCESS_SUCCESS_STR));

            logger::log_info(L"Elevated Process has been launched.", true);
            properties.push_back(pair(L"Accepted", L"True"));
        }
        else
        {
            handle_launch_error(winrt_error::get_last_error());
        }
        settings_key::signal_changed();
    }
    catch (hresult_error const& e)
    {
        winrt_error error{ e, true, 3 };
        handle_launch_error(error);
        settings_key::signal_changed();

        report::add_device_metadata(properties);
        logger::log_error(error);
        crashes::track_error(error, properties);
    }

    analytics::track_event(L"OnAdminstratorPrivilageRequested", properties);
}

void launch_elevated_process_if_requested()
{
    auto n_args = 0;
    array_view args{ CommandLineToArgvW(GetCommandLineW(), &n_args), 4 };
    if (n_args < 4) return;

    // Assumed first entry is for uwp app
    wstring praid{ Package::Current().GetAppListEntries().GetAt(0).AppUserModelId() };
    praid.erase(0, praid.find(L"!") + 1);
    if (wcscmp(args[1], L"/InvokerPRAID:") == 0 &&
        wcscmp(args[2], praid.c_str()) == 0 &&
        wcscmp(args[3], L"/admin") == 0
        )
    {
        launch_elevated_process();
    }
}

void initialize_extension_service()
{
    launch_elevated_process_if_requested();

    if (!is_first_instance(EXTENSION_MUTEX_NAME)) return;

    try
    {
        check_bool(ProcessIdToSessionId(GetCurrentProcessId(), &session_id));
        package_sid = unbox_value<hstring>(settings_key::read(PACKAGE_SID_STR));

        unblock_pipe = std::format(PIPE_NAME_FORMAT, session_id, package_sid.c_str(), EXTENSION_UNBLOCK_PIPE_CONNECTION_NAME_STR);

        logger::log_info(L"Successfully started Desktop Extension.", true);
        logger::log_info(L"Waiting on uwp app to send data.", true);

        check_bool(CreateThread(nullptr, 0, unblock_file_from_pipe_data, nullptr, 0, nullptr));
    }
    catch (hresult_error const& e)
    {
        winrt_error error{ e, true, 3 };
        report::dictionary properties{};
        report::add_device_metadata(properties);
        logger::log_error(error);
        crashes::track_error(error, properties);
        analytics::track_event(L"OnInitializationForExtensionFailed", properties);
        exit_app(e.code());
    }

LifeTimeCheck:
    handle life_time_obj
    {
        OpenMutexW(
            SYNCHRONIZE,
            false,
            std::format(
                L"AppContainerNamedObjects\\{}\\{}",
                package_sid.c_str(),
                EXTENSION_PROCESS_LIFETIME_OBJ_NAME_STR
            ).c_str()
        )
    };

    if (life_time_obj)
    {
        life_time_obj.close();
        Sleep(1000);
        goto LifeTimeCheck;
    }
}