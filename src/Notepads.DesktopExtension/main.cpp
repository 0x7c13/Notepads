#include "pch.h"

using namespace std;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::ApplicationModel::AppService;
using namespace Windows::Foundation;
using namespace Windows::Foundation::Collections;
using namespace Windows::Storage;

constexpr LPCTSTR DesktopExtensionMutexName = L"DesktopExtensionMutexName";
constexpr LPCTSTR AdminExtensionMutexName = L"AdminExtensionMutexName";

constexpr int PIPE_READ_BUFFER = MAX_PATH + 40;

HANDLE appExitEvent = CreateEvent(NULL, TRUE, FALSE, NULL);
HANDLE childProcessHandle = INVALID_HANDLE_VALUE;

DWORD sessionId;

AppServiceConnection interopServiceConnection = { 0 };

int releaseResources()
{
    CloseHandle(appExitEvent);
    CloseHandle(childProcessHandle);
    return 0;
}

void exitApp()
{
    if(childProcessHandle) TerminateProcess(childProcessHandle, 0);
    SetEvent(appExitEvent);
}

void printDebugMessage(LPCTSTR message, DWORD sleepTime = 0)
{
#ifdef _DEBUG
    wcout << message << endl;
    Sleep(sleepTime);
#endif
}

void printDebugMessage(LPCTSTR message, LPCTSTR parameter)
{
#ifdef _DEBUG
    wcout << message << " \"" << parameter << "\"" << endl;
#endif
}

void onUnhandledException()
{
    //TODO: Implement exception handling
}

void onUnexpectedException()
{
    //TODO: Implement exception handling
}

IInspectable readSettingsKey(hstring key)
{
    return ApplicationData::Current().LocalSettings().Values().TryLookup(key);
}

hstring packageSid = unbox_value_or<hstring>(readSettingsKey(PackageSidStr), L"");

DWORD WINAPI saveFileFromPipeData(LPVOID param)
{
    LPCSTR result = "Failed";

    wstringstream pipeName;
    pipeName << "\\\\.\\pipe\\Sessions\\" << sessionId << "\\AppContainerNamedObjects\\"<< packageSid.c_str() << "\\" << Package::Current().Id().FamilyName().c_str() << "\\" << AdminPipeConnectionNameStr;
    
    HANDLE hPipe = INVALID_HANDLE_VALUE;
    while (hPipe == INVALID_HANDLE_VALUE)
    {
        Sleep(50);
        if (WaitNamedPipe(pipeName.str().c_str(), NMPWAIT_WAIT_FOREVER))
        {
            hPipe = CreateFile(pipeName.str().c_str(), GENERIC_READ | GENERIC_WRITE, 0, NULL, OPEN_EXISTING, 0, NULL);
        }
    }

    CreateThread(NULL, 0, saveFileFromPipeData, NULL, 0, NULL);

    char readBuffer[PIPE_READ_BUFFER];
    stringstream bufferStore;
    DWORD byteRead;
    do
    {
        if (ReadFile(hPipe, readBuffer, PIPE_READ_BUFFER * sizeof(char), &byteRead, NULL))
        {
            bufferStore << readBuffer;
        }
    } while (byteRead >= PIPE_READ_BUFFER);

    string filePathBuffer;
    string memoryMapId;
    string dataArrayLengthStr;
    wstring filePath;
    getline(bufferStore, filePathBuffer, '|');
    getline(bufferStore, memoryMapId, '|');
    getline(bufferStore, dataArrayLengthStr);
    int dataArrayLength = stoi(dataArrayLengthStr);
    wstringstream memoryMapName;
    memoryMapName << "AppContainerNamedObjects\\" << packageSid.c_str() << "\\" << memoryMapId.c_str();

    HANDLE hMemory = OpenFileMapping(FILE_MAP_READ, FALSE, memoryMapName.str().c_str());
    if (hMemory)
    {
        LPVOID mapView = MapViewOfFile(hMemory, FILE_MAP_READ, 0, 0, dataArrayLength);
        if (mapView)
        {
            // Need to cnvert file path string to UTF-16 to properly read emoji characters if present
            int convertResult = MultiByteToWideChar(CP_UTF8, 0, filePathBuffer.c_str(), (int)strlen(filePathBuffer.c_str()), NULL, 0);
            if (convertResult > 0)
            {
                filePath.resize(convertResult);
                MultiByteToWideChar(CP_UTF8, 0, filePathBuffer.c_str(), (int)filePathBuffer.size(), &filePath[0], (int)filePath.size());
            }

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

    if (WriteFile(hPipe, result, strlen(result) * sizeof(char), NULL, NULL)) FlushFileBuffers(hPipe);

    CloseHandle(hPipe);

    if (strcmp(result, "Success") == 0)
    {
        printDebugMessage(L"Successfully wrote to", filePath.c_str());
    }
    else
    {
        printDebugMessage(L"Failed to write to", filePath.c_str());
    }
    printDebugMessage(L"Waiting on uwp app to send data.");

    return 0;
}

void initializeAdminService()
{
    printDebugMessage(L"Successfully started Adminstrator Extension.");
    printDebugMessage(L"Waiting on uwp app to send data.");

    saveFileFromPipeData(NULL);
}

fire_and_forget launchElevatedProcess()
{
    TCHAR fileName[MAX_PATH];
    GetModuleFileName(NULL, fileName, MAX_PATH);

    SHELLEXECUTEINFO shExInfo = { 0 };
    shExInfo.cbSize = sizeof(shExInfo);
    shExInfo.fMask = SEE_MASK_NOCLOSEPROCESS;
    shExInfo.hwnd = 0;
    shExInfo.lpVerb = L"runas";
    shExInfo.lpFile = fileName;
    shExInfo.lpParameters = L"";
    shExInfo.lpDirectory = 0;
    shExInfo.nShow = SW_SHOW;
    shExInfo.hInstApp = 0;

    auto message = ValueSet();
    message.Insert(InteropCommandLabel, box_value(CreateElevetedExtensionCommandStr));
    if (ShellExecuteEx(&shExInfo))
    {
        childProcessHandle = shExInfo.hProcess;
        message.Insert(InteropCommandAdminCreatedLabel, box_value(true));
        printDebugMessage(L"Adminstrator Extension has been launched.");
    }
    else
    {
        message.Insert(InteropCommandAdminCreatedLabel, box_value(false));
        printDebugMessage(L"User canceled launching of Adminstrator Extension.");
    }
    co_await interopServiceConnection.SendMessageAsync(message);
}

void onConnectionServiceRequestRecieved(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
{
    // Get a deferral because we use an awaitable API below to respond to the message
    // and we don't want this call to get canceled while we are waiting.
    auto messageDeferral = args.GetDeferral();

    auto message = args.Request().Message();
    if (!message.HasKey(InteropCommandLabel)) return;

    auto command = unbox_value_or<hstring>(message.TryLookup(InteropCommandLabel), L"");
    if (command == CreateElevetedExtensionCommandStr)
    {
        launchElevatedProcess();
    }
    else if (command == ExitAppCommandStr)
    {
        exitApp();
    }

    messageDeferral.Complete();
}

void onConnectionServiceClosed(AppServiceConnection sender, AppServiceClosedEventArgs args)
{
    exitApp();
}

fire_and_forget initializeInteropService()
{
    printDebugMessage(L"Successfully started Desktop Extension.");

    interopServiceConnection = AppServiceConnection();
    interopServiceConnection.AppServiceName(InteropServiceName);
    interopServiceConnection.PackageFamilyName(Package::Current().Id().FamilyName());

    interopServiceConnection.RequestReceived(onConnectionServiceRequestRecieved);
    interopServiceConnection.ServiceClosed(onConnectionServiceClosed);

    auto status = co_await interopServiceConnection.OpenAsync();
    if (status != AppServiceConnectionStatus::Success)
    {
        exitApp();
    }

    auto message = ValueSet();
    message.Insert(InteropCommandLabel, box_value(RegisterExtensionCommandStr));
    co_await interopServiceConnection.SendMessageAsync(message);

    printDebugMessage(L"Successfully created App Service.");
}

bool isElevetedProcessLaunchRequested()
{
    bool result = false;

    LPTSTR* argList;
    int argCount;
    argList = CommandLineToArgvW(GetCommandLine(), &argCount);
    if (argCount > 3 && wcscmp(argList[3], L"/admin") == 0)
    {
        result = true;
    }

    return result;
}

bool isFirstInstance(LPCTSTR mutexName)
{
    bool result = true;
    try
    {
        auto hMutex = OpenMutex(MUTEX_ALL_ACCESS, FALSE, mutexName);
        if (!hMutex)
        {
            CreateMutex(NULL, FALSE, mutexName);
        }
        else
        {
            result = false;
            exitApp();
            printDebugMessage(L"Closing this instance as another instance is already running.", 5000);
        }
        ReleaseMutex(hMutex);
    }
    catch (...) { }
    
    return result;
}

bool isElevatedProcess()
{
    bool result = false;

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

int main()
{
    set_terminate(onUnhandledException);
    set_unexpected(onUnexpectedException);
    _onexit(releaseResources);

    init_apartment();

    ProcessIdToSessionId(GetCurrentProcessId(), &sessionId);

    if (isElevatedProcess())
    {
        if (isFirstInstance(AdminExtensionMutexName))
        {
            initializeAdminService();
        }
    }
    else
    {
        if (isFirstInstance(DesktopExtensionMutexName))
        {
            initializeInteropService();
            if (isElevetedProcessLaunchRequested()) launchElevatedProcess();
        }
    }

    WaitForSingleObject(appExitEvent, INFINITE);
}

#ifndef _DEBUG
int APIENTRY wWinMain(_In_ HINSTANCE hInstance,
    _In_opt_ HINSTANCE hPrevInstance,
    _In_ LPWSTR    lpCmdLine,
    _In_ int       nCmdShow)
{
    main();
}
#endif
