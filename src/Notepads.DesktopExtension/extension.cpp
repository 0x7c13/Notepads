#include "pch.h"
#include "appcenter.h"

using namespace fmt;
using namespace std;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::Storage;

DWORD sessionId;
hstring packageSid;

wstring extensionUnblockPipeName;

HANDLE extensionUnblockEvent = NULL;

DWORD WINAPI unblockFileFromPipeData(LPVOID /* param */)
{
    setExceptionHandling();

    HANDLE hPipe = INVALID_HANDLE_VALUE;
    if (!WaitForSingleObject(extensionUnblockEvent, INFINITE) &&
        ResetEvent(extensionUnblockEvent) &&
        WaitNamedPipe(extensionUnblockPipeName.c_str(), NMPWAIT_WAIT_FOREVER))
    {
        hPipe = CreateFile(extensionUnblockPipeName.c_str(), GENERIC_READ, 0, NULL, OPEN_EXISTING, 0, NULL);
    }

    if (hPipe)
    {
        CreateThread(NULL, 0, unblockFileFromPipeData, NULL, 0, NULL);

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

        hstring filePath(pipeData.str());
        com_ptr<IPersistFile> pFile;
        if (SUCCEEDED(CoCreateInstance(CLSID_PersistentZoneIdentifier, 0, CLSCTX_ALL, __uuidof(pFile), pFile.put_void())) &&
            SUCCEEDED(pFile->Load(filePath.c_str(), STGM_READWRITE)))
        {
            LPTSTR lastWriterPackageFamilyName;
            if (SUCCEEDED(pFile.as<IZoneIdentifier2>()->GetLastWriterPackageFamilyName(&lastWriterPackageFamilyName)) &&
                Package::Current().Id().FamilyName() == lastWriterPackageFamilyName &&
                SUCCEEDED(pFile.as<IZoneIdentifier>()->Remove()) &&
                SUCCEEDED(pFile->Save(filePath.c_str(), TRUE)))
            {
                printDebugMessage(format(L"Successfully unblocked file \"{}\"", filePath).c_str());
            }
            else
            {
                printDebugMessage(format(L"Failed to unblock file \"{}\"", filePath).c_str());
            }
        }
        else
        {
            printDebugMessage(format(L"Failed to unblock file \"{}\"", filePath).c_str());
        }
        pFile.~com_ptr();

        CloseHandle(hPipe);
        printDebugMessage(L"Waiting on uwp app to send data.");
    }
    else
    {
        exitApp();
    }

    return 0;
}

VOID launchElevatedProcess()
{
    TCHAR fileName[MAX_PATH];
    GetModuleFileName(NULL, fileName, MAX_PATH);

    SHELLEXECUTEINFO shExInfo
    { 
        .cbSize = sizeof(shExInfo),
        .fMask = SEE_MASK_NOCLOSEPROCESS,
        .hwnd = 0,
        .lpVerb = L"runas",
        .lpFile = fileName,
        .lpParameters = L"",
        .lpDirectory = 0,
        .nShow = SW_SHOW,
        .hInstApp = 0
    };

    vector<pair<const CHAR*, string>> properties;
    if (ShellExecuteEx(&shExInfo))
    {
        ApplicationData::Current().LocalSettings().Values().Insert(LastChangedSettingsAppInstanceIdStr, box_value(L""));
        ApplicationData::Current().LocalSettings().Values().Insert(LastChangedSettingsKeyStr, box_value(LaunchElevetedProcessSuccessStr));

        printDebugMessage(L"Elevated Process has been launched.");
        properties.push_back(pair("Accepted", "True"));
    }
    else
    {
        ApplicationData::Current().LocalSettings().Values().Insert(LastChangedSettingsAppInstanceIdStr, box_value(L""));
        ApplicationData::Current().LocalSettings().Values().Insert(LastChangedSettingsKeyStr, box_value(LaunchElevetedProcessFailedStr));

        printDebugMessage(L"Launching of Elevated Process was cancelled.");
        pair<DWORD, wstring> ex = getLastErrorDetails();
        properties.insert(properties.end(),
            {
                pair("Denied", "True"),
                pair("Error Code", to_string(ex.first)),
                pair("Error Message", to_string(ex.second))
            });
    }

    ApplicationData::Current().SignalDataChanged();
    AppCenter::trackEvent("OnAdminstratorPrivilageRequested", properties);
}

VOID launchElevatedProcessIfRequested()
{
    LPWSTR* szArglist = NULL;
    INT nArgs;
    szArglist = CommandLineToArgvW(GetCommandLineW(), &nArgs);
    if (szArglist)
    {
        // Assumed first entry is for uwp app
        auto aumid = Package::Current().GetAppListEntries().GetAt(0).AppUserModelId();
        wstring praid(aumid);
        praid.erase(0, praid.find(L"!") + 1);
        if (nArgs > 3 &&
            wcscmp(szArglist[1], L"/InvokerPRAID:") == 0 &&
            wcscmp(szArglist[2], praid.c_str()) == 0 &&
            wcscmp(szArglist[3], L"/admin") == 0)
        {
            launchElevatedProcess();
        }
    }
    LocalFree(szArglist);
}

VOID initializeExtensionService()
{
    launchElevatedProcessIfRequested();

    if (!isFirstInstance(ExtensionMutexName)) return;

    ProcessIdToSessionId(GetCurrentProcessId(), &sessionId);
    packageSid = unbox_value_or<hstring>(readSettingsKey(PackageSidStr), L"");

    extensionUnblockEvent = OpenEvent(SYNCHRONIZE | EVENT_MODIFY_STATE, FALSE, format(NAMED_OBJECT_FORMAT, packageSid, ExtensionUnblockEventNameStr).c_str());
    extensionUnblockPipeName = format(PIPE_NAME_FORMAT, sessionId, packageSid, ExtensionUnblockPipeConnectionNameStr);

    printDebugMessage(L"Successfully started Desktop Extension.");
    printDebugMessage(L"Waiting on uwp app to send data.");

    CreateThread(NULL, 0, unblockFileFromPipeData, NULL, 0, NULL);

LifeTimeCheck:
    HANDLE lifeTimeObj = OpenMutex(SYNCHRONIZE, FALSE, format(L"AppContainerNamedObjects\\{}\\{}", packageSid, ExtensionProcessLifetimeObjNameStr).c_str());
    if (lifeTimeObj)
    {
        CloseHandle(lifeTimeObj);
        Sleep(1000);
        goto LifeTimeCheck;
    }
}