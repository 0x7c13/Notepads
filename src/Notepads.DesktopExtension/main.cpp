#include "pch.h"
#include "appcenter.h"

using namespace fmt;
using namespace std;
using namespace winrt;
using namespace Windows::Foundation;
using namespace Windows::Storage;

constexpr LPCTSTR DesktopExtensionMutexName = L"DesktopExtensionMutexName";
constexpr LPCTSTR AdminExtensionMutexName = L"AdminExtensionMutexName";

DWORD sessionId;
hstring packageSid = unbox_value_or<hstring>(readSettingsKey(PackageSidStr), L"");

extern HANDLE adminWriteEvent;
extern HANDLE adminRenameEvent;

IInspectable readSettingsKey(hstring key)
{
    return ApplicationData::Current().LocalSettings().Values().TryLookup(key);
}

INT releaseResources()
{
    CloseHandle(adminWriteEvent);
    CloseHandle(adminRenameEvent);
    return 0;
}

VOID exitApp()
{
    exit(0);
}

VOID onUnhandledException()
{
    logLastError(true).get();
    Sleep(5000);
    exitApp();
}

VOID onUnexpectedException()
{
    logLastError(false).get();
    Sleep(5000);
    exitApp();
}

VOID setExceptionHandling()
{
    set_terminate(onUnhandledException);
    set_unexpected(onUnexpectedException);
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

        printDebugMessage(L"Adminstrator Extension has been launched.");
        properties.push_back(pair("Accepted", "True"));
    }
    else
    {
        ApplicationData::Current().LocalSettings().Values().Insert(LastChangedSettingsAppInstanceIdStr, box_value(L""));
        ApplicationData::Current().LocalSettings().Values().Insert(LastChangedSettingsKeyStr, box_value(LaunchElevetedProcessSuccessStr));

        printDebugMessage(L"Launching of Adminstrator Extension was cancelled.");
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

bool isFirstInstance(LPCTSTR mutexName)
{
    auto result = true;

    HANDLE hMutex = OpenMutex(MUTEX_ALL_ACCESS, FALSE, mutexName);
    if (!hMutex)
    {
        CreateMutex(NULL, FALSE, mutexName);

#ifdef _DEBUG
        initializeLogging(wcscmp(mutexName, DesktopExtensionMutexName) == 0 ? L"-extension.log" : L"-elevated-extension.log");
#endif
    }
    else
    {
        result = false;
        printDebugMessage(L"Closing this instance as another instance is already running.", 3000);
        exitApp();
    }
    if (hMutex) ReleaseMutex(hMutex);
    
    return result;
}

bool isElevatedProcess()
{
    auto result = false;

    HANDLE hToken = NULL;
    if (OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &hToken))
    {
        TOKEN_ELEVATION Elevation;
        DWORD cbSize = sizeof(TOKEN_ELEVATION);
        if (GetTokenInformation(hToken, TokenElevation, &Elevation, sizeof(Elevation), &cbSize))
        {
            result = Elevation.TokenIsElevated;
        }
    }

    if (hToken)
    {
        CloseHandle(hToken);
    }

    return result;
}

#ifndef _DEBUG
INT APIENTRY wWinMain(_In_ HINSTANCE /* hInstance */, _In_opt_ HINSTANCE /* hPrevInstance */, _In_ LPWSTR /* lpCmdLine */, _In_ INT /* nCmdShow */)
#else
INT main()
#endif
{
    setExceptionHandling();
    SetErrorMode(SEM_NOGPFAULTERRORBOX);
    _onexit(releaseResources);

    ProcessIdToSessionId(GetCurrentProcessId(), &sessionId);

    init_apartment();
    AppCenter::start();

    if (isElevatedProcess())
    {
        if (!isFirstInstance(AdminExtensionMutexName)) return 0;

        initializeAdminService();
    }
    else
    {
        if (!isFirstInstance(DesktopExtensionMutexName)) return 0;

        launchElevatedProcess();
        return 0;
    }

    LifeTimeCheck:
    HANDLE lifeTimeObj = OpenMutex(SYNCHRONIZE, FALSE, format(L"AppContainerNamedObjects\\{}\\{}", packageSid, DesktopExtensionLifetimeObjNameStr).c_str());
    if (lifeTimeObj)
    {
        CloseHandle(lifeTimeObj);
        Sleep(1000);
        goto LifeTimeCheck;
    }
    exit(0);
}