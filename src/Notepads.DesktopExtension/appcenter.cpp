// Documentation is at https://docs.microsoft.com/en-us/appcenter/diagnostics/upload-crashes
#include "pch.h"
#include "appcenter.h"
#include "log.h"
#include "rapidjson/stringbuffer.h"

#define APPCENTER_ENDPOINT "https://in.appcenter.ms/logs?Api-Version=1.0.0"

using namespace fmt;
using namespace rapidjson;
using namespace std;
using namespace winrt;
using namespace AppCenter;
using namespace Windows::ApplicationModel;
using namespace Windows::Foundation;
using namespace Windows::Security::ExchangeActiveSyncProvisioning;
using namespace Windows::Storage;
using namespace Windows::System;
using namespace Windows::System::Profile;

void AppCenter::start()
{
	if (!AppCenterSecret) return;

	hstring installId = unbox_value_or<hstring>(readSettingsKey(AppCenterInstallIdStr), L"");
	if (installId.empty()) return;

	headerList = curl_slist_append(headerList, "Content-Type: application/json");
	headerList = curl_slist_append(headerList, format("app-secret: {}", to_string(AppCenterSecret)).c_str());
	headerList = curl_slist_append(headerList, format("install-id: {}", to_string(installId)).c_str());
}

void AppCenter::trackError(bool isFatal, DWORD errorCode, const string& message, const stacktrace& stackTrace)
{
	if (!AppCenterSecret) return;

	string crashReportId = to_string(to_hstring(GuidHelper::CreateNewGuid()));
	crashReportId.erase(0, crashReportId.find_first_not_of('{')).erase(crashReportId.find_last_not_of('}') + 1);
	string crashReportSid = to_string(to_hstring(GuidHelper::CreateNewGuid()));
	crashReportSid.erase(0, crashReportSid.find_first_not_of('{')).erase(crashReportSid.find_last_not_of('}') + 1);
	string errorAttachmentId = to_string(to_hstring(GuidHelper::CreateNewGuid()));
	errorAttachmentId.erase(0, errorAttachmentId.find_first_not_of('{')).erase(errorAttachmentId.find_last_not_of('}') + 1);
	string eventReportId = to_string(to_hstring(GuidHelper::CreateNewGuid()));
	eventReportId.erase(0, eventReportId.find_first_not_of('{')).erase(eventReportId.find_last_not_of('}') + 1);

	TCHAR locale[LOCALE_NAME_MAX_LENGTH + 1];
	LCIDToLocaleName(GetThreadLocale(), locale, LOCALE_NAME_MAX_LENGTH, 0);
	TCHAR localeDisplayName[LOCALE_NAME_MAX_LENGTH + 1];
	GetLocaleInfoEx(locale, LOCALE_SENGLISHDISPLAYNAME, localeDisplayName, LOCALE_NAME_MAX_LENGTH);

	string isElevated = isElevatedProcess() ? "True" : "False";
	string eventType = isFatal ? "OnWin32UnhandledException" : "OnWin32UnexpectedException";

	vector<pair<const CHAR*, string>> properties;
	properties.push_back(pair("Exception", format("Win32Exception: Exception of code no. {} was thrown.", errorCode)));
	properties.push_back(pair("Message", message));
	properties.push_back(pair("Culture", to_string(localeDisplayName)));
	properties.push_back(pair("AvailableMemory", to_string((FLOAT)MemoryManager::AppMemoryUsageLimit() / 1024 / 1024)));
	properties.push_back(pair("FirstUseTimeUTC", getTimeStamp("%m/%d/%Y %T")));
	properties.push_back(pair("OSArchitecture", to_string(Package::Current().Id().Architecture())));
	properties.push_back(pair("OSVersion", deviceInfo.getOsVersion()));
	properties.push_back(pair("IsDesktopExtension", "True"));
	properties.push_back(pair("IsElevated", isElevated));

	vector<Log> errorReportSet;
	errorReportSet.push_back(Log(LogType::managedError, crashReportId, crashReportSid, isFatal, new Exception(message, stackTrace), properties));
	errorReportSet.push_back(Log(LogType::errorAttachment, errorAttachmentId, crashReportId,
		base64_encode(format("Exception: Win32Exception code no. {}, Message: {}, IsDesktopExtension: True, IsElevated: {}",
			errorCode, message, isElevated))));
	errorReportSet.push_back(Log(LogType::event, eventReportId, crashReportSid, eventType, properties));

	StringBuffer errorReport;
#ifdef  _DEBUG
	PrettyWriter<StringBuffer> writer(errorReport);
#else
	Writer<StringBuffer> writer(errorReport);
#endif

	writer.StartObject();
	writer.String("logs");
	writer.StartArray();
	for (vector<Log>::const_iterator report = errorReportSet.begin(); report != errorReportSet.end(); ++report)
	{
		report->Serialize(writer);
	}
	writer.EndArray();
	writer.EndObject();

	CURL* curl = curl_easy_init();
	if (curl) {
		curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headerList);
		curl_easy_setopt(curl, CURLOPT_URL, APPCENTER_ENDPOINT);
		curl_easy_setopt(curl, CURLOPT_POSTFIELDS, errorReport.GetString());

#ifdef  _DEBUG
		curl_easy_setopt(curl, CURLOPT_VERBOSE, 1L);
#endif

		CURLcode res = curl_easy_perform(curl);
		if (res != CURLE_OK)
		{
			printDebugMessage(format(L"curl_easy_perform() failed: {}", to_hstring(curl_easy_strerror(res))).c_str());
		}
	}
	curl_easy_cleanup(curl);
}

void AppCenter::trackEvent(const string& name, const vector<pair<const CHAR*, string>>& properties, const string& sid)
{
	if (!AppCenterSecret) return;

	string eventReportId = to_string(to_hstring(GuidHelper::CreateNewGuid()));
	eventReportId.erase(0, eventReportId.find_first_not_of('{')).erase(eventReportId.find_last_not_of('}') + 1);

	StringBuffer eventReport;
#ifdef  _DEBUG
	PrettyWriter<StringBuffer> writer(eventReport);
#else
	Writer<StringBuffer> writer(eventReport);
#endif

	writer.StartObject();
	writer.String("logs");
	writer.StartArray();
	Log(LogType::event, eventReportId, sid, name, properties).Serialize(writer);
	writer.EndArray();
	writer.EndObject();

	CURL* curl = curl_easy_init();
	if (curl) {
		curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headerList);
		curl_easy_setopt(curl, CURLOPT_URL, APPCENTER_ENDPOINT);
		curl_easy_setopt(curl, CURLOPT_POSTFIELDS, eventReport.GetString());

#ifdef  _DEBUG
		curl_easy_setopt(curl, CURLOPT_VERBOSE, 1L);
#endif

		CURLcode res = curl_easy_perform(curl);
		if (res != CURLE_OK)
		{
			printDebugMessage(format(L"curl_easy_perform() failed: {}", to_hstring(curl_easy_strerror(res))).c_str());
		}
	}
	curl_easy_cleanup(curl);
}