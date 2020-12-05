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

constexpr int PIPE_READ_BUFFER = MAX_PATH + 240;
constexpr int MAX_TIME_STR = 9;
constexpr int MAX_DATE_STR = 11;

HANDLE appExitEvent = CreateEvent(NULL, TRUE, FALSE, NULL);
HANDLE appExitJob = NULL;

DWORD sessionId;

AppServiceConnection interopServiceConnection = { 0 };

StorageFile logFile = nullptr;

int releaseResources()
{
    CloseHandle(appExitEvent);
    CloseHandle(appExitJob);
    return 0;
}

void exitApp()
{
    SetEvent(appExitEvent);
}

void printDebugMessage(LPCTSTR message, DWORD sleepTime = 0)
{
#ifdef _DEBUG
    wcout << message << endl;
    Sleep(sleepTime);
#endif
}

void printDebugMessage(LPCTSTR message, LPCTSTR parameter, DWORD sleepTime = 0)
{
#ifdef _DEBUG
    wcout << message << " \"" << parameter << "\"" << endl;
    Sleep(sleepTime);
#endif
}

fire_and_forget logLastError(LPCTSTR errorTitle)
{
    if (logFile != nullptr)
    {
        LPVOID msgBuf;
        DWORD errorCode = GetLastError();

        FormatMessage(
            FORMAT_MESSAGE_ALLOCATE_BUFFER | FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
            NULL,
            errorCode,
            0,
            (LPTSTR)&msgBuf,
            0,
            NULL);

        wstringstream msgStrm;
        wstring msg;
        msgStrm << (LPCTSTR)msgBuf;
        LocalFree(msgBuf);
        getline(msgStrm, msg, L'\r');

        SYSTEMTIME systemTime;
        GetSystemTime(&systemTime);
        TCHAR timeStr[MAX_TIME_STR];
        GetTimeFormatEx(LOCALE_NAME_INVARIANT, 0, &systemTime, NULL, timeStr, MAX_TIME_STR);
        TCHAR dateStr[MAX_DATE_STR];
        GetDateFormatEx(LOCALE_NAME_INVARIANT, 0, &systemTime, NULL, dateStr, MAX_DATE_STR, NULL);
        wstringstream debugMsg;
        debugMsg << dateStr << " " << timeStr << " " << "[" << "Error" << "]" << " " << "[" << "Error Code: " << errorCode << "]" << " " << errorTitle << msg << endl;

        co_await PathIO::AppendTextAsync(logFile.Path(), debugMsg.str().c_str());
    }
}

void onUnhandledException()
{
    logLastError(L"OnUnhandledException: ");
    exitApp();
}

void onUnexpectedException()
{
    logLastError(L"OnUnexpectedException: ");
    exitApp();
}

void setExceptionHandling()
{
    set_terminate(onUnhandledException);
    set_unexpected(onUnexpectedException);
}

IInspectable readSettingsKey(hstring key)
{
    return ApplicationData::Current().LocalSettings().Values().TryLookup(key);
}

hstring packageSid = unbox_value_or<hstring>(readSettingsKey(PackageSidStr), L"");

DWORD WINAPI saveFileFromPipeData(LPVOID param)
{
    setExceptionHandling();

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

    char readBuffer[PIPE_READ_BUFFER] = { 0 };
    string pipeDataStr;
    DWORD byteRead;
    do
    {
        if (ReadFile(hPipe, readBuffer, (PIPE_READ_BUFFER - 1) * sizeof(char), &byteRead, NULL))
        {
            pipeDataStr.append(readBuffer);
            fill(begin(readBuffer), end(readBuffer), '\0');
        }
    } while (byteRead >= (PIPE_READ_BUFFER - 1) * sizeof(char));

    // Need to cnvert pipe data string to UTF-16 to properly read unicode characters
    wstring pipeDataWstr;
    int convertResult = MultiByteToWideChar(CP_UTF8, 0, pipeDataStr.c_str(), (int)strlen(pipeDataStr.c_str()), NULL, 0);
    if (convertResult > 0)
    {
        pipeDataWstr.resize(convertResult);
        MultiByteToWideChar(CP_UTF8, 0, pipeDataStr.c_str(), (int)pipeDataStr.size(), &pipeDataWstr[0], (int)pipeDataWstr.size());
    }
    wstringstream pipeData(pipeDataWstr);

    wstring filePath;
    wstring memoryMapId;
    wstring dataArrayLengthStr;
    getline(pipeData, filePath, L'|');
    getline(pipeData, memoryMapId, L'|');
    getline(pipeData, dataArrayLengthStr);

    int dataArrayLength = stoi(dataArrayLengthStr);
    wstringstream memoryMapName;
    memoryMapName << "AppContainerNamedObjects\\" << packageSid.c_str() << "\\" << memoryMapId;

    HANDLE hMemory = OpenFileMapping(FILE_MAP_READ, FALSE, memoryMapName.str().c_str());
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

    CreateThread(NULL, 0, saveFileFromPipeData, NULL, 0, NULL);
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
        // Create Job to close child process when parent exits/crashes.
        if (appExitJob) CloseHandle(appExitJob);
        appExitJob = CreateJobObject(NULL, NULL);
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION info = { 0 };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(appExitJob, JobObjectExtendedLimitInformation, &info, sizeof(info));
        AssignProcessToJobObject(appExitJob, shExInfo.hProcess);

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
    setExceptionHandling();

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

fire_and_forget initializeLogging(LPCTSTR trailStr)
{
    auto localFolder = ApplicationData::Current().LocalFolder();
    auto logFolder = co_await localFolder.CreateFolderAsync(L"Logs", CreationCollisionOption::OpenIfExists);
    SYSTEMTIME systemTime;
    GetSystemTime(&systemTime);
    TCHAR timeStr[MAX_TIME_STR];
    GetTimeFormatEx(LOCALE_NAME_INVARIANT, 0, &systemTime, L"HHmmss", timeStr, MAX_TIME_STR);
    TCHAR dateStr[MAX_DATE_STR];
    GetDateFormatEx(LOCALE_NAME_INVARIANT, 0, &systemTime, L"yyyyMMdd", dateStr, MAX_DATE_STR, NULL);
    wstringstream logFilePath;
    logFilePath << dateStr << "T" << timeStr << trailStr;
    logFile = co_await logFolder.CreateFileAsync(logFilePath.str(), CreationCollisionOption::OpenIfExists);
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

    auto hMutex = OpenMutex(MUTEX_ALL_ACCESS, FALSE, mutexName);
    if (!hMutex)
    {
        CreateMutex(NULL, FALSE, mutexName);
    }
    else
    {
        result = false;
        printDebugMessage(L"Closing this instance as another instance is already running.", 5000);
        exitApp();
    }
    ReleaseMutex(hMutex);
    
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

#ifndef _DEBUG
int APIENTRY wWinMain(_In_ HINSTANCE hInstance, _In_opt_ HINSTANCE hPrevInstance, _In_ LPWSTR lpCmdLine, _In_ int nCmdShow)
#else
int main()
#endif
{
    setExceptionHandling();
    SetErrorMode(SEM_NOGPFAULTERRORBOX);
    _onexit(releaseResources);

    init_apartment();

    if (isElevatedProcess())
    {
        if (!isFirstInstance(AdminExtensionMutexName)) return 0;

#ifdef _DEBUG
        initializeLogging(L"-elevated-extension.log");
#endif

        ProcessIdToSessionId(GetCurrentProcessId(), &sessionId);
        initializeAdminService();
    }
    else
    {
        if (!isFirstInstance(DesktopExtensionMutexName)) return 0;

#ifdef _DEBUG
        initializeLogging(L"-extension.log");
#endif

        initializeInteropService();
        if (isElevetedProcessLaunchRequested()) launchElevatedProcess();
    }

    WaitForSingleObject(appExitEvent, INFINITE);
    exit(0);
}