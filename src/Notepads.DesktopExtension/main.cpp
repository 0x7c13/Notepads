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
extern HANDLE appExitJob;

IInspectable readSettingsKey(hstring key)
{
    return ApplicationData::Current().LocalSettings().Values().TryLookup(key);
}

INT releaseResources()
{
    CloseHandle(adminWriteEvent);
    CloseHandle(adminRenameEvent);
    CloseHandle(appExitJob);
    return 0;
}

void exitApp()
{
    exit(0);
}

void onUnhandledException()
{
    logLastError(true).get();
    exitApp();
}

void onUnexpectedException()
{
    logLastError(false).get();
    exitApp();
}

void setExceptionHandling()
{
    set_terminate(onUnhandledException);
    set_unexpected(onUnexpectedException);
}

bool isElevetedProcessLaunchRequested()
{
    auto result = false;

    INT argCount;
    LPTSTR* argList = CommandLineToArgvW(GetCommandLine(), &argCount);
    if (argCount > 3 && wcscmp(argList[3], L"/admin") == 0)
    {
        result = true;
    }

    return result;
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

        initializeInteropService();
        if (isElevetedProcessLaunchRequested()) launchElevatedProcess();
    }

    wstring pipeName = format(L"\\\\.\\pipe\\Sessions\\{}\\AppContainerNamedObjects\\{}\\{}",
        sessionId, packageSid, DesktopExtensionLifetimeObjNameStr);
    while (WaitNamedPipe(pipeName.c_str(), 2000) || GetLastError() == ERROR_SEM_TIMEOUT)
    {
        WaitNamedPipe(pipeName.c_str(), NMPWAIT_WAIT_FOREVER);
    }
    exit(0);
}