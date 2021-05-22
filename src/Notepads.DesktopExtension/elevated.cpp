#include "pch.h"
#include "logger.h"
#include "appcenter.h"
#include "fmt/format.h"

using namespace fmt;
using namespace std;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::Storage::AccessCache;

extern DWORD session_id;
extern hstring package_sid;

hstring write_pipe;
hstring rename_pipe;

DWORD WINAPI saveFileFromPipeData(LPVOID /* param */)
{
    try
    {
        handle elevated_write_event = handle(OpenEvent(SYNCHRONIZE | EVENT_MODIFY_STATE, FALSE, format(NAMED_OBJECT_FORMAT, package_sid, ELEVATED_WRITE_EVENT_NAME_STR).c_str()));

        check_bool(!WaitForSingleObject(elevated_write_event.get(), INFINITE));
        check_bool(ResetEvent(elevated_write_event.get()));
        check_bool(WaitNamedPipe(write_pipe.c_str(), NMPWAIT_WAIT_FOREVER));

        auto h_pipe = handle(CreateFile(write_pipe.c_str(), GENERIC_READ | GENERIC_WRITE, 0, nullptr, OPEN_EXISTING, 0, nullptr));
        check_bool(bool(h_pipe));

        CreateThread(nullptr, 0, saveFileFromPipeData, nullptr, 0, nullptr);

        TCHAR read_buffer[PIPE_READ_BUFFER];
        wstringstream pipe_data;
        DWORD byte_read;
        do
        {
            fill(begin(read_buffer), end(read_buffer), '\0');
            if (ReadFile(h_pipe.get(), read_buffer, (PIPE_READ_BUFFER - 1) * sizeof(TCHAR), &byte_read, nullptr))
            {
                pipe_data << read_buffer;
            }
        } while (byte_read >= (PIPE_READ_BUFFER - 1) * sizeof(TCHAR));

        wstring filePath;
        wstring memory_map_id;
        wstring data_length_str;
        getline(pipe_data, filePath, L'|');
        getline(pipe_data, memory_map_id, L'|');
        getline(pipe_data, data_length_str);

        int data_length = stoi(data_length_str);
        wstring memory_map = format(NAMED_OBJECT_FORMAT, package_sid, memory_map_id);

        auto h_memory = handle(OpenFileMapping(FILE_MAP_READ, FALSE, memory_map.c_str()));
        check_bool(bool(h_memory));

        auto map_view = MapViewOfFile(h_memory.get(), FILE_MAP_READ, 0, 0, data_length);
        check_bool(map_view);

        auto h_file = handle(CreateFile(filePath.c_str(), GENERIC_READ | GENERIC_WRITE, 0, nullptr, TRUNCATE_EXISTING, 0, nullptr));
        check_bool(bool(h_file));
        check_bool(WriteFile(h_file.get(), map_view, data_length, nullptr, nullptr));
        check_bool(FlushFileBuffers(h_file.get()));

        UnmapViewOfFile(map_view);

        auto result = L"Success";
        check_bool(WriteFile(h_pipe.get(), result, static_cast<DWORD>(wcslen(result) * sizeof(TCHAR)), nullptr, nullptr));
        check_bool(FlushFileBuffers(h_pipe.get()));

        report::dictionary properties
        {
            pair("Result", to_string(result))
        };
        logger::log_info(format(L"Successfully wrote to \"{}\"", filePath).c_str(), true);
        logger::log_info(L"Waiting on uwp app to send data.", true);

        analytics::track_event("OnWriteToSystemFileRequested", properties);
        return 0;
    }
    catch (hresult_error const& e)
    {
        winrt_error error{ e, true, 3 };
        report::dictionary properties
        {
            pair("Result", "Failed")
        };
        report::add_device_metadata(properties);
        logger::log_error(error);
        crashes::track_error(error, properties);
        analytics::track_event("OnWriteToSystemFileRequested", properties);
        exit_app(e.code());
        return e.code();
    }
}

DWORD WINAPI renameFileFromPipeData(LPVOID /* param */)
{
    hstring old_name;
    wstring new_name;
    try
    {
        handle elevated_rename_event = handle(OpenEvent(SYNCHRONIZE | EVENT_MODIFY_STATE, FALSE, format(NAMED_OBJECT_FORMAT, package_sid, ELEVATED_RENAME_EVENT_NAME_STR).c_str()));

        check_bool(!WaitForSingleObject(elevated_rename_event.get(), INFINITE));
        check_bool(ResetEvent(elevated_rename_event.get()));
        check_bool(WaitNamedPipe(rename_pipe.c_str(), NMPWAIT_WAIT_FOREVER));

        auto h_pipe = handle(CreateFile(rename_pipe.c_str(), GENERIC_READ | GENERIC_WRITE, 0, nullptr, OPEN_EXISTING, 0, nullptr));
        check_bool(bool(h_pipe));

        CreateThread(nullptr, 0, renameFileFromPipeData, nullptr, 0, nullptr);

        TCHAR read_buffer[PIPE_READ_BUFFER];
        wstringstream pipe_data;
        DWORD byte_read;
        do
        {
            fill(begin(read_buffer), end(read_buffer), '\0');
            if (ReadFile(h_pipe.get(), read_buffer, (PIPE_READ_BUFFER - 1) * sizeof(TCHAR), &byte_read, nullptr))
            {
                pipe_data << read_buffer;
            }
        } while (byte_read >= (PIPE_READ_BUFFER - 1) * sizeof(TCHAR));

        wstring file_token;
        getline(pipe_data, file_token, L'|');
        getline(pipe_data, new_name, L'|');

        auto file = StorageApplicationPermissions::FutureAccessList().GetFileAsync(file_token).get();
        StorageApplicationPermissions::FutureAccessList().Remove(file_token);
        old_name = file.Path();
        file.RenameAsync(new_name).get();

        auto result = StorageApplicationPermissions::FutureAccessList().Add(file).c_str();

        check_bool(WriteFile(h_pipe.get(), result, static_cast<DWORD>(wcslen(result) * sizeof(TCHAR)), nullptr, nullptr));
        check_bool(FlushFileBuffers(h_pipe.get()));

        logger::log_info(format(L"Successfully renamed \"{}\" to \"{}\"", old_name, new_name).c_str(), true);
        logger::log_info(L"Waiting on uwp app to send data.", true);

        report::dictionary properties
        {
            pair("Result", "Success")
        };

        analytics::track_event("OnRenameToSystemFileRequested", properties);
        return 0;
    }
    catch (hresult_error const& e)
    {
        winrt_error error{ e, true, 3 };
        logger::log_info(
            old_name.empty()
            ? new_name.empty()
            ? L"Failed to rename file"
            : format(L"Failed to rename file to \"{}\"", new_name).c_str()
            : format(L"Failed to rename \"{}\" to \"{}\"", old_name, new_name).c_str(),
            true
        );

        report::dictionary properties
        {
            pair("Result", "Failed")
        };
        report::add_device_metadata(properties);
        logger::log_error(error);
        crashes::track_error(error, properties);
        analytics::track_event("OnRenameToSystemFileRequested", properties);
        exit_app(e.code());
        return e.code();
    }
}

void initialize_elevated_service()
{
    if (!is_first_instance(ELEVATED_MUTEX_NAME)) return;

    try
    {
        check_bool(ProcessIdToSessionId(GetCurrentProcessId(), &session_id));
        package_sid = unbox_value<hstring>(settings_key::read(PACKAGE_SID_STR));

        write_pipe = format(PIPE_NAME_FORMAT, session_id, package_sid, ELEVATED_WRITE_PIPE_CONNECTION_NAME_STR);
        rename_pipe = format(PIPE_NAME_FORMAT, session_id, package_sid, ELEVATED_RENAME_PIPE_CONNECTION_NAME_STR);

        logger::log_info(L"Successfully started Elevated Process.", true);
        logger::log_info(L"Waiting on uwp app to send data.", true);

        auto write_thread = handle(CreateThread(nullptr, 0, saveFileFromPipeData, nullptr, 0, nullptr));
        auto rename_thread = handle(CreateThread(nullptr, 0, renameFileFromPipeData, nullptr, 0, nullptr));
        check_bool(bool(write_thread));
    }
    catch (hresult_error const& e)
    {
        winrt_error error{ e, true, 3 };
        report::dictionary properties{};
        report::add_device_metadata(properties);
        logger::log_error(error);
        crashes::track_error(error, properties);
        analytics::track_event("OnInitializationForExtensionFailed", properties);
        exit_app(e.code());
    }

LifeTimeCheck:
    auto life_time_obj = handle(
        OpenMutex(
            SYNCHRONIZE,
            FALSE,
            format(
                L"AppContainerNamedObjects\\{}\\{}",
                package_sid,
                ELEVATED_PROCESS_LIFETIME_OBJ_NAME_STR
            ).c_str()
        )
    );

    if (life_time_obj)
    {
        life_time_obj.close();
        Sleep(1000);
        goto LifeTimeCheck;
    }
}