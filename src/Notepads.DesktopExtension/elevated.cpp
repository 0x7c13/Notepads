#include "pch.h"
#include "appcenter.h"
#include "fmt/format.h"

using namespace fmt;
using namespace std;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::ApplicationModel::DataTransfer;

constexpr INT PIPE_READ_BUFFER = 2 * MAX_PATH + 10;
constexpr LPCTSTR pipeNameFormat = L"\\\\.\\pipe\\Sessions\\{}\\AppContainerNamedObjects\\{}\\{}\\{}";
constexpr LPCTSTR namedObjectFormat = L"AppContainerNamedObjects\\{}\\{}";

extern DWORD sessionId;
extern hstring packageSid;

HANDLE adminWriteEvent = NULL;
HANDLE adminRenameEvent = NULL;

wstring writePipeName;
wstring renamePipeName;

DWORD WINAPI saveFileFromPipeData(LPVOID /* param */)
{
    setExceptionHandling();

    vector<pair<const CHAR*, string>> properties;
    LPCTSTR result = L"Failed";

    HANDLE hPipe = INVALID_HANDLE_VALUE;
    if (!WaitForSingleObject(adminWriteEvent, INFINITE) && ResetEvent(adminWriteEvent) && WaitNamedPipe(writePipeName.c_str(), NMPWAIT_WAIT_FOREVER))
    {
        hPipe = CreateFile(writePipeName.c_str(), GENERIC_READ | GENERIC_WRITE, 0, NULL, OPEN_EXISTING, 0, NULL);
    }

    if (hPipe)
    {
        CreateThread(NULL, 0, saveFileFromPipeData, NULL, 0, NULL);

        TCHAR readBuffer[PIPE_READ_BUFFER];
        wstringstream pipeData;
        DWORD byteRead;
        do
        {
            fill(begin(readBuffer), end(readBuffer), '\0');
            if (ReadFile(hPipe, readBuffer, (PIPE_READ_BUFFER - 1) * sizeof(TCHAR), &byteRead, NULL))
            {
                pipeData << readBuffer;
            }
        } while (byteRead >= (PIPE_READ_BUFFER - 1) * sizeof(TCHAR));

        wstring filePath;
        wstring memoryMapId;
        wstring dataArrayLengthStr;
        getline(pipeData, filePath, L'|');
        getline(pipeData, memoryMapId, L'|');
        getline(pipeData, dataArrayLengthStr);

        INT dataArrayLength = stoi(dataArrayLengthStr);
        wstring memoryMapName = format(namedObjectFormat, packageSid, memoryMapId);

        HANDLE hMemory = OpenFileMapping(FILE_MAP_READ, FALSE, memoryMapName.c_str());
        if (hMemory)
        {
            LPVOID mapView = MapViewOfFile(hMemory, FILE_MAP_READ, 0, 0, dataArrayLength);
            if (mapView)
            {
                HANDLE hFile = CreateFile(
                    filePath.c_str(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    NULL,
                    TRUNCATE_EXISTING,
                    0,
                    NULL);

                if (hFile)
                {
                    if (WriteFile(hFile, mapView, dataArrayLength, NULL, NULL) && FlushFileBuffers(hFile))
                    {
                        result = L"Success";
                    }

                    CloseHandle(hFile);
                }

                CloseHandle(mapView);
            }

            CloseHandle(hMemory);
        }

        if (WriteFile(hPipe, result, wcslen(result) * sizeof(TCHAR), NULL, NULL)) FlushFileBuffers(hPipe);
        CloseHandle(hPipe);

        properties.push_back(pair("Result", to_string(result)));
        if (wcscmp(result, L"Success") == 0)
        {
            printDebugMessage(format(L"Successfully wrote to \"{}\"", filePath).c_str());
        }
        else
        {
            pair<DWORD, wstring> ex = getLastErrorDetails();
            properties.insert(properties.end(),
                {
                    pair("Error Code", to_string(ex.first)),
                    pair("Error Message", winrt::to_string(ex.second))
                });
            printDebugMessage(format(L"Failed to write to \"{}\"", filePath).c_str());
        }
        printDebugMessage(L"Waiting on uwp app to send data.");

        AppCenter::trackEvent("OnWriteToSystemFileRequested", properties);
    }
    else
    {
        pair<DWORD, wstring> ex = getLastErrorDetails();
        properties.insert(properties.end(),
            {
                pair("Result", to_string(result)),
                pair("Error Code", to_string(ex.first)),
                pair("Error Message", winrt::to_string(ex.second))
            });
        AppCenter::trackEvent("OnWriteToSystemFileRequested", properties);
        exitApp();
    }

    return 0;
}

DWORD WINAPI renameFileFromPipeData(LPVOID /* param */)
{
    setExceptionHandling();

    vector<pair<const CHAR*, string>> properties;
    LPCTSTR result = L"Failed";

    HANDLE hPipe = INVALID_HANDLE_VALUE;
    if (!WaitForSingleObject(adminRenameEvent, INFINITE) && ResetEvent(adminRenameEvent) && WaitNamedPipe(renamePipeName.c_str(), NMPWAIT_WAIT_FOREVER))
    {
        hPipe = CreateFile(renamePipeName.c_str(), GENERIC_READ | GENERIC_WRITE, 0, NULL, OPEN_EXISTING, 0, NULL);
    }

    if (hPipe)
    {
        CreateThread(NULL, 0, renameFileFromPipeData, NULL, 0, NULL);

        TCHAR readBuffer[PIPE_READ_BUFFER];
        wstringstream pipeData;
        DWORD byteRead;
        do
        {
            fill(begin(readBuffer), end(readBuffer), '\0');
            if (ReadFile(hPipe, readBuffer, (PIPE_READ_BUFFER - 1) * sizeof(TCHAR), &byteRead, NULL))
            {
                pipeData << readBuffer;
            }
        } while (byteRead >= (PIPE_READ_BUFFER - 1) * sizeof(TCHAR));

        wstring fileToken;
        wstring newName;
        getline(pipeData, fileToken, L'|');
        getline(pipeData, newName, L'|');

        auto file = SharedStorageAccessManager::RedeemTokenForFileAsync(fileToken).get();
        SharedStorageAccessManager::RemoveFile(fileToken);
        auto oldName = file.Path();
        file.RenameAsync(newName).get();
        result = SharedStorageAccessManager::AddFile(file).c_str();

        if (WriteFile(hPipe, result, wcslen(result) * sizeof(TCHAR), NULL, NULL)) FlushFileBuffers(hPipe);
        CloseHandle(hPipe);

        if (wcscmp(result, L"Failed") == 0)
        {
            pair<DWORD, wstring> ex = getLastErrorDetails();
            properties.insert(properties.end(),
                {
                    pair("Result", to_string(result)),
                    pair("Error Code", to_string(ex.first)),
                    pair("Error Message", winrt::to_string(ex.second))
                });
            printDebugMessage(format(L"Failed to rename \"{}\" to \"{}\"", oldName, newName).c_str());
        }
        else
        {
            properties.push_back(pair("Result", "Success"));
            printDebugMessage(format(L"Successfully renamed \"{}\" to \"{}\"", oldName, newName).c_str());
        }
        printDebugMessage(L"Waiting on uwp app to send data.");

        AppCenter::trackEvent("OnRenameToSystemFileRequested", properties);
    }
    else
    {
        pair<DWORD, wstring> ex = getLastErrorDetails();
        properties.insert(properties.end(),
            {
                pair("Result", to_string(result)),
                pair("Error Code", to_string(ex.first)),
                pair("Error Message", winrt::to_string(ex.second))
            });
        AppCenter::trackEvent("OnRenameToSystemFileRequested", properties);
        exitApp();
    }

    return 0;
}

void initializeAdminService()
{
    adminWriteEvent = OpenEvent(SYNCHRONIZE | EVENT_MODIFY_STATE, FALSE, format(namedObjectFormat, packageSid, AdminWriteEventNameStr).c_str());
    adminRenameEvent = OpenEvent(SYNCHRONIZE | EVENT_MODIFY_STATE, FALSE, format(namedObjectFormat, packageSid, AdminRenameEventNameStr).c_str());

    writePipeName = format(pipeNameFormat, sessionId, packageSid, Package::Current().Id().FamilyName(), AdminWritePipeConnectionNameStr);
    renamePipeName = format(pipeNameFormat, sessionId, packageSid, Package::Current().Id().FamilyName(), AdminRenamePipeConnectionNameStr);

    printDebugMessage(L"Successfully started Adminstrator Extension.");
    printDebugMessage(L"Waiting on uwp app to send data.");

    CreateThread(NULL, 0, saveFileFromPipeData, NULL, 0, NULL);
    CreateThread(NULL, 0, renameFileFromPipeData, NULL, 0, NULL);
}