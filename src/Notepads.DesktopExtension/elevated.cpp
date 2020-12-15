#include "pch.h"
#include "appcenter.h"
#include "fmt/format.h"

using namespace fmt;
using namespace std;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::Foundation;
using namespace Windows::Storage;

constexpr INT PIPE_READ_BUFFER = MAX_PATH + 240;

DWORD sessionId;

IInspectable readSettingsKey(hstring key)
{
    return ApplicationData::Current().LocalSettings().Values().TryLookup(key);
}

hstring packageSid = unbox_value_or<hstring>(readSettingsKey(PackageSidStr), L"");

DWORD WINAPI saveFileFromPipeData(LPVOID /* param */)
{
    setExceptionHandling();

    LPCSTR result = "Failed";

    wstring pipeName = format(L"\\\\.\\pipe\\Sessions\\{}\\AppContainerNamedObjects\\{}\\{}\\{}",
        sessionId, packageSid, Package::Current().Id().FamilyName(), AdminPipeConnectionNameStr);

    HANDLE hPipe = INVALID_HANDLE_VALUE;
    while (hPipe == INVALID_HANDLE_VALUE)
    {
        Sleep(50);
        if (WaitNamedPipe(pipeName.c_str(), NMPWAIT_WAIT_FOREVER))
        {
            hPipe = CreateFile(pipeName.c_str(), GENERIC_READ | GENERIC_WRITE, 0, NULL, OPEN_EXISTING, 0, NULL);
        }
    }

    CreateThread(NULL, 0, saveFileFromPipeData, NULL, 0, NULL);

    CHAR readBuffer[PIPE_READ_BUFFER] = { 0 };
    string pipeDataStr;
    DWORD byteRead;
    do
    {
        if (ReadFile(hPipe, readBuffer, (PIPE_READ_BUFFER - 1) * sizeof(CHAR), &byteRead, NULL))
        {
            pipeDataStr.append(readBuffer);
            fill(begin(readBuffer), end(readBuffer), '\0');
        }
    } while (byteRead >= (PIPE_READ_BUFFER - 1) * sizeof(CHAR));

    // Need to cnvert pipe data string to UTF-16 to properly read unicode characters
    wstring pipeDataWstr;
    INT convertResult = MultiByteToWideChar(CP_UTF8, 0, pipeDataStr.c_str(), (INT)strlen(pipeDataStr.c_str()), NULL, 0);
    if (convertResult > 0)
    {
        pipeDataWstr.resize(convertResult);
        MultiByteToWideChar(CP_UTF8, 0, pipeDataStr.c_str(), (INT)pipeDataStr.size(), &pipeDataWstr[0], (INT)pipeDataWstr.size());
    }
    wstringstream pipeData(pipeDataWstr);

    wstring filePath;
    wstring memoryMapId;
    wstring dataArrayLengthStr;
    getline(pipeData, filePath, L'|');
    getline(pipeData, memoryMapId, L'|');
    getline(pipeData, dataArrayLengthStr);

    INT dataArrayLength = stoi(dataArrayLengthStr);
    wstring memoryMapName = format(L"AppContainerNamedObjects\\{}\\{}", packageSid, memoryMapId);

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
                DWORD byteWrote;
                if (WriteFile(hFile, mapView, dataArrayLength, &byteWrote, NULL) && FlushFileBuffers(hFile))
                {
                    result = "Success";
                }

                CloseHandle(hFile);
            }

            CloseHandle(mapView);
        }

        CloseHandle(hMemory);
    }

    if (WriteFile(hPipe, result, strlen(result) * sizeof(CHAR), NULL, NULL)) FlushFileBuffers(hPipe);

    CloseHandle(hPipe);

    if (strcmp(result, "Success") == 0)
    {
        printDebugMessage(format(L"Successfully wrote to", filePath).c_str());
    }
    else
    {
        printDebugMessage(format(L"Failed to write to", filePath).c_str());
    }
    printDebugMessage(L"Waiting on uwp app to send data.");

    vector<pair<const CHAR*, string>> properties;
    properties.push_back(pair("Result", result));
    AppCenter::trackEvent("OnWriteToSystemFileRequested", properties);

    return 0;
}

void initializeAdminService()
{
    ProcessIdToSessionId(GetCurrentProcessId(), &sessionId);

    printDebugMessage(L"Successfully started Adminstrator Extension.");
    printDebugMessage(L"Waiting on uwp app to send data.");

    CreateThread(NULL, 0, saveFileFromPipeData, NULL, 0, NULL);
}