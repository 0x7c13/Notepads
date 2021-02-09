#include "pch.h"
#include "iostream"

#ifdef _PRODUCTION
#define AUMID L"19282JackieLiu.Notepads-Beta_echhpq9pdbte8!App"
#else
#define AUMID L"Notepads-Dev_ezhh5fms182ha!App"
#endif

#define OPEN L"open"
#define PRINT L"print"

#define NOTEPADPRINTCOMMAND L"/p"
#define NOTEPADOPENWITHANSIENCODINGCOMMAND L"/a"
#define NOTEPADOPENWITHUTF16ENCODINGCOMMAND L"/w"

using namespace std;
using namespace winrt;

#ifndef _DEBUG
INT APIENTRY wWinMain(_In_ HINSTANCE /* hInstance */, _In_opt_ HINSTANCE /* hPrevInstance */, _In_ LPWSTR /* lpCmdLine */, _In_ INT /* nCmdShow */)
#else
INT main()
#endif
{
    init_apartment();
    
    LPWSTR* szArglist = NULL;
    INT nArgs;
    szArglist = CommandLineToArgvW(GetCommandLine(), &nArgs);
    if (szArglist && nArgs > 1)
    {
        LPCTSTR verb = OPEN;
        if (wcscmp(NOTEPADPRINTCOMMAND, szArglist[1]) == 0) verb = PRINT;
        bool isOpenRequested = (wcscmp(OPEN, verb) == 0);

        INT ct = nArgs - isOpenRequested ? 1 : 2;
        HRESULT hr = E_OUTOFMEMORY;
        com_ptr<IShellItemArray> ppsia = NULL;
        PIDLIST_ABSOLUTE* rgpidl = new(std::nothrow) PIDLIST_ABSOLUTE[ct];
        if (rgpidl)
        {
            hr = S_OK;
            INT cpidl;
            for (cpidl = 0; SUCCEEDED(hr) && cpidl < ct; cpidl++)
            {
                hr = SHParseDisplayName(szArglist[cpidl + isOpenRequested ? 1 : 2], NULL, &rgpidl[cpidl], 0, NULL);
            }

            if (cpidl > 0 && SUCCEEDED(SHCreateShellItemArrayFromIDLists(cpidl, rgpidl, ppsia.put())))
            {
                com_ptr<IApplicationActivationManager> appActivationMgr = NULL;
                if (SUCCEEDED(CoCreateInstance(CLSID_ApplicationActivationManager, NULL, CLSCTX_LOCAL_SERVER, __uuidof(appActivationMgr), appActivationMgr.put_void())))
                {
                    DWORD pid = 0;
                    appActivationMgr->ActivateForFile(AUMID, ppsia.get(), verb, &pid);
#ifdef _DEBUG
                    cout << "Launched files with process id: " << pid;
                    Sleep(2000);
#endif
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

    LocalFree(szArglist);
    uninit_apartment();
}