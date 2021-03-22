#include "pch.h"
#include "appcenter.h"

using namespace winrt;

extern HANDLE extensionUnblockEvent;
extern HANDLE elevatedWriteEvent;
extern HANDLE elevatedRenameEvent;

INT releaseResources()
{
    CloseHandle(extensionUnblockEvent);
    CloseHandle(elevatedWriteEvent);
    CloseHandle(elevatedRenameEvent);
    AppCenter::exit();
    uninit_apartment();
    return 0;
}

VOID exitApp()
{
    exit(0);
}

VOID onUnhandledException()
{
    logLastError(true).get();
    exitApp();
}

VOID onUnexpectedException()
{
    logLastError(false).get();
    exitApp();
}

VOID setExceptionHandling()
{
    set_terminate(onUnhandledException);
    set_unexpected(onUnexpectedException);
}

bool isFirstInstance(LPCTSTR mutexName)
{
    auto result = true;

    HANDLE hMutex = OpenMutex(MUTEX_ALL_ACCESS, FALSE, mutexName);
    if (!hMutex)
    {
        CreateMutex(NULL, FALSE, mutexName);

#ifdef _DEBUG
        initializeLogging(wcscmp(mutexName, ExtensionMutexName) == 0 ? L"-extension.log" : L"-elevated-extension.log");
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

    init_apartment();
    AppCenter::start();

    if (isElevatedProcess())
    {
        initializeElevatedService();
    }
    else
    {
        initializeExtensionService();
    }
}