#include "pch.h"
#include "appcenter.h"
#include "fmt/format.h"

using namespace fmt;
using namespace std;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::Storage::AccessCache;

extern DWORD sessionId;
extern hstring packageSid;

HANDLE elevatedWriteEvent = NULL;
HANDLE elevatedRenameEvent = NULL;

wstring writePipeName;
wstring renamePipeName;

DWORD WINAPI saveFileFromPipeData(LPVOID /* param */)
{
    setExceptionHandling();

    vector<pair<const CHAR*, string>> properties;
    LPCTSTR result = L"Failed";

    HANDLE hPipe = INVALID_HANDLE_VALUE;
    if (!WaitForSingleObject(elevatedWriteEvent, INFINITE) &&
        ResetEvent(elevatedWriteEvent) &&
        WaitNamedPipe(writePipeName.c_str(), NMPWAIT_WAIT_FOREVER))
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
        wstring memoryMapName = format(NAMED_OBJECT_FORMAT, packageSid, memoryMapId);
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

                UnmapViewOfFile(mapView);
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
    if (!WaitForSingleObject(elevatedRenameEvent, INFINITE) &&
        ResetEvent(elevatedRenameEvent) &&
        WaitNamedPipe(renamePipeName.c_str(), NMPWAIT_WAIT_FOREVER))
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

        auto file = StorageApplicationPermissions::FutureAccessList().GetFileAsync(fileToken).get();
        StorageApplicationPermissions::FutureAccessList().Remove(fileToken);
        auto oldName = file.Path();
        file.RenameAsync(newName).get();
        result = StorageApplicationPermissions::FutureAccessList().Add(file).c_str();

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

VOID initializeElevatedService()
{
    if (!isFirstInstance(ElevatedMutexName)) return;

    ProcessIdToSessionId(GetCurrentProcessId(), &sessionId);
    packageSid = unbox_value_or<hstring>(readSettingsKey(PackageSidStr), L"");

    elevatedWriteEvent = OpenEvent(SYNCHRONIZE | EVENT_MODIFY_STATE, FALSE, format(NAMED_OBJECT_FORMAT, packageSid, ElevatedWriteEventNameStr).c_str());
    elevatedRenameEvent = OpenEvent(SYNCHRONIZE | EVENT_MODIFY_STATE, FALSE, format(NAMED_OBJECT_FORMAT, packageSid, ElevatedRenameEventNameStr).c_str());

    writePipeName = format(PIPE_NAME_FORMAT, sessionId, packageSid, ElevatedWritePipeConnectionNameStr);
    renamePipeName = format(PIPE_NAME_FORMAT, sessionId, packageSid, ElevatedRenamePipeConnectionNameStr);

    printDebugMessage(L"Successfully started Elevated Process.");
    printDebugMessage(L"Waiting on uwp app to send data.");

    CreateThread(NULL, 0, saveFileFromPipeData, NULL, 0, NULL);
    CreateThread(NULL, 0, renameFileFromPipeData, NULL, 0, NULL);

LifeTimeCheck:
    HANDLE lifeTimeObj = OpenMutex(SYNCHRONIZE, FALSE, format(L"AppContainerNamedObjects\\{}\\{}", packageSid, ElevatedProcessLifetimeObjNameStr).c_str());
    if (lifeTimeObj)
    {
        CloseHandle(lifeTimeObj);
        Sleep(1000);
        goto LifeTimeCheck;
    }
}