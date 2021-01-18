#include "pch.h"
#include "appcenter.h"

#define MAX_TIME_STR 20
#define MAX_DATE_STR 20
#define MAX_DATETIME_STR 100

using namespace boost::stacktrace;
using namespace boost::stacktrace::detail;
using namespace fmt;
using namespace std;
using namespace winrt;
using namespace Windows::Foundation;
using namespace Windows::Storage;
using namespace Windows::System;

StorageFile logFile = NULL;

VOID printDebugMessage([[maybe_unused]] LPCTSTR message, [[maybe_unused]] DWORD sleepTime) {
#ifdef _DEBUG
	wcout << message << endl;
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
	return format("{}.{}Z", getTimeStamp("%FT%T"), systemTime.wMilliseconds);
}

string getTimeStamp(const CHAR* format)
{
	time_t timePast = time(NULL);
	tm utcTime;
	gmtime_s(&utcTime, &timePast);
	CHAR timeStamp[MAX_DATETIME_STR];
	strftime(timeStamp, sizeof(timeStamp), format, &utcTime);
	return timeStamp;
}

//From https://stackoverflow.com/a/34571089/5155484
static const string b = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
string base64_encode(const string& in)
{
	string out;

	INT val = 0, valb = -6;
	for (UCHAR c : in) {
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

pair<DWORD, wstring> getLastErrorDetails()
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

	wstring msg;
	getline(wstringstream((LPCTSTR)msgBuf), msg, L'\r');
	LocalFree(msgBuf);

	return pair<DWORD, wstring> { errorCode, msg };
}

IAsyncAction logLastError(bool isFatal)
{
	stacktrace st = stacktrace();
	hstring stackTrace = to_hstring(to_string(&st.as_vector()[0], st.size()));

	pair<DWORD, wstring> ex = getLastErrorDetails();

	printDebugMessage(stackTrace.c_str(), 5000);
	AppCenter::trackError(isFatal, ex.first, to_string(ex.second), st);

	if (logFile)
	{
		SYSTEMTIME systemTime;
		GetSystemTime(&systemTime);
		TCHAR timeStr[MAX_TIME_STR];
		GetTimeFormatEx(LOCALE_NAME_INVARIANT, 0, &systemTime, NULL, timeStr, MAX_TIME_STR);
		TCHAR dateStr[MAX_DATE_STR];
		GetDateFormatEx(LOCALE_NAME_INVARIANT, 0, &systemTime, NULL, dateStr, MAX_DATE_STR, NULL);
		wstring debugMsg = format(L"{} {} [Error] [Error Code: {}] {}{}\n{}\n",
			dateStr, timeStr, ex.first, isFatal ? L"OnUnhandledException: " : L"OnUnexpectedException: ", ex.second, stackTrace);

		co_await PathIO::AppendTextAsync(logFile.Path(), debugMsg);
	}
}

fire_and_forget initializeLogging(LPCTSTR trailStr)
{
	auto localFolder = ApplicationData::Current().LocalFolder();
	auto logFolder = co_await localFolder.CreateFolderAsync(L"Logs", CreationCollisionOption::OpenIfExists);
	logFile = co_await logFolder.CreateFileAsync(to_hstring(getTimeStamp("%Y%m%dT%H%M%S")) + to_hstring(trailStr), CreationCollisionOption::OpenIfExists);
}