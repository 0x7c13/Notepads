#include "pch.h"
#include "appcenter.h"
#include "shlobj_core.h"

using namespace fmt;
using namespace std;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::Foundation;
using namespace Windows::Storage;

constexpr LPCTSTR DesktopExtensionMutexName = L"DesktopExtensionMutexName";
constexpr LPCTSTR AdminExtensionMutexName = L"AdminExtensionMutexName";

wstring adminParams;

extern hstring packageSid;
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
        .lpParameters = adminParams.c_str(),
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

bool isFileLaunchRequested()
{
    bool isFileLaunchRequested = true;
    LPWSTR* szArglist = NULL;
    INT nArgs;
    szArglist = CommandLineToArgvW(GetCommandLineW(), &nArgs);
    if (szArglist)
    {
        // Assumed first entry is for uwp app
        auto aumid = Package::Current().GetAppListEntries().GetAt(0).AppUserModelId();
        wstring praid(aumid);
        praid.erase(0, praid.find(L"!") + 1);
        if (nArgs > 2 && wcscmp(szArglist[1], L"/InvokerPRAID:") == 0 && wcscmp(szArglist[2], praid.c_str()) == 0)
        {
            adminParams = format(L"{} {}", szArglist[1], szArglist[2]);
            isFileLaunchRequested = false;
        }
        else
        {
            INT ct = nArgs - 1;
            HRESULT hr = E_OUTOFMEMORY;
            com_ptr< IShellItemArray> ppsia = NULL;
            PIDLIST_ABSOLUTE* rgpidl = new(std::nothrow) PIDLIST_ABSOLUTE[ct];
            if (rgpidl)
            {
                hr = S_OK;
                INT cpidl;
                for (cpidl = 0; SUCCEEDED(hr) && cpidl < ct; cpidl++)
                {
                    hr = SHParseDisplayName(szArglist[cpidl + 1], nullptr, &rgpidl[cpidl], 0, nullptr);
                }

                if (cpidl > 0 && SUCCEEDED(SHCreateShellItemArrayFromIDLists(cpidl, rgpidl, ppsia.put())))
                {
                    com_ptr<IApplicationActivationManager> appActivationMgr = NULL;
                    if (SUCCEEDED(CoCreateInstance(CLSID_ApplicationActivationManager, NULL, CLSCTX_LOCAL_SERVER, __uuidof(appActivationMgr), appActivationMgr.put_void())))
                    {
                        DWORD pid = 0;
                        appActivationMgr->ActivateForFile(aumid.c_str(), ppsia.get(), NULL, &pid);
                        printDebugMessage(format(L"Launched files with process id: {}", pid).c_str(), 5000);
                    }
                    appActivationMgr.~com_ptr();
                }

                for (INT i = 0; i < cpidl; i++)
                {
                    CoTaskMemFree(rgpidl[i]);
                }
            }

            ppsia.~com_ptr();
            delete[] rgpidl;
        }
    }

    LocalFree(szArglist);

    return isFileLaunchRequested;
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

    init_apartment();
    if (isFileLaunchRequested())
    {
        uninit_apartment();
        return 0;
    }

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
        AppCenter::exit();
        uninit_apartment();
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
    AppCenter::exit();
    uninit_apartment();
    exit(0);
}