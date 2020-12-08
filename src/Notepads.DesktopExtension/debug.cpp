#include "pch.h"

using namespace std;
using namespace winrt;
using namespace Windows::Foundation;
using namespace Windows::Storage;
using namespace Windows::System;

constexpr int MAX_TIME_STR = 20;
constexpr int MAX_DATE_STR = 20;
constexpr int MAX_DATETIME_STR = 100;

StorageFile logFile = nullptr;

void printDebugMessage(LPCTSTR message, DWORD sleepTime)
{
#ifdef _DEBUG
    wcout << message << endl;
    Sleep(sleepTime);
#endif
}

void printDebugMessage(LPCTSTR message, LPCTSTR parameter, DWORD sleepTime)
{
#ifdef _DEBUG
    wcout << message << " \"" << parameter << "\"" << endl;
    Sleep(sleepTime);
#endif
}

string to_string(ProcessorArchitecture arch)
{
	switch (arch)
	{
	case ProcessorArchitecture::Arm:
		return "Arm";
	case ProcessorArchitecture::Arm64:
		return "Arm64";
	case ProcessorArchitecture::X86OnArm64:
		return "X86OnArm64";
	case ProcessorArchitecture::X86:
		return "X86";
	case ProcessorArchitecture::X64:
		return "X64";
	case ProcessorArchitecture::Neutral:
		return "Neutral";
	default:
		return "Unknown";
	}
}

string getTimeStamp()
{
	SYSTEMTIME systemTime;
	GetSystemTime(&systemTime);
	stringstream timeStamp;
	timeStamp << getTimeStamp("%FT%T") << "." << systemTime.wMilliseconds << "Z";
	return timeStamp.str();
}

string getTimeStamp(const char* format)
{
	time_t timePast = time(NULL);
	tm* utcTime = gmtime(&timePast);
	CHAR timeStamp[MAX_DATETIME_STR];
	strftime(timeStamp, sizeof(timeStamp), format, utcTime);
	return timeStamp;
}

//From https://stackoverflow.com/a/34571089/5155484
typedef unsigned char uchar;
static const string b = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
string base64_encode(const string& in)
{
	string out;

	int val = 0, valb = -6;
	for (uchar c : in) {
		val = (val << 8) + c;
		valb += 8;
		while (valb >= 0) {
			out.push_back(b[(val >> valb) & 0x3F]);
			valb -= 6;
		}
	}
	if (valb > -6) out.push_back(b[((val << 8) >> (valb + 8)) & 0x3F]);
	while (out.size() % 4) out.push_back('=');
	return out;
}

IAsyncAction logLastError(LPCTSTR errorTitle)
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

    AppCenter::trackError(errorCode, to_string(msg), wcscmp(errorTitle, L"OnUnhandledException: ") == 0);

    if (logFile != nullptr)
    {
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

fire_and_forget initializeLogging(LPCTSTR trailStr)
{
    auto localFolder = ApplicationData::Current().LocalFolder();
    auto logFolder = co_await localFolder.CreateFolderAsync(L"Logs", CreationCollisionOption::OpenIfExists);
    logFile = co_await logFolder.CreateFileAsync(to_hstring(getTimeStamp("%Y%m%dT%H%M%S")) + to_hstring(trailStr), CreationCollisionOption::OpenIfExists);
}