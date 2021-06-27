#pragma once
#include "pch.h"
#include "error.h"
#include "device.h"
#include "curl_client.h"
#include "error_report.h"
#include "event_report.h"
#include "settings_key.h"
#include "rapidjson/stringbuffer.h"

// Documentation is at https://docs.microsoft.com/en-us/appcenter/diagnostics/upload-crashes
#define APP_CENTER_ENDPOINT "https://in.appcenter.ms/logs?Api-Version=1.0.0"

__declspec(selectany) curl_slist_handle header { nullptr };

struct appcenter
{
	static void start() noexcept
	{
		if (!APP_CENTER_SECRET || strlen(APP_CENTER_SECRET) == 0) return;

		auto install_id = winrt::unbox_value_or<winrt::hstring>(settings_key::read(APP_CENTER_INSTALL_ID_STR), L"");
		if (install_id.empty()) return;

		struct curl_slist* slist = nullptr;
		slist = curl_slist_append(slist, "Content-Type: application/json");
		slist = curl_slist_append(slist, std::format("app-secret: {}", APP_CENTER_SECRET).c_str());
		slist = curl_slist_append(slist, std::format("install-id: {}", winrt::to_string(install_id)).c_str());
		header.attach(slist);
	}

private:

	appcenter() noexcept = default;
	appcenter(appcenter&&) noexcept = default;
	appcenter(appcenter const& other) noexcept = default;
};

struct crashes
{
	static void track_error(
		winrt_error const& error,
		report::dictionary const& properties,
		std::string const& attachment = ""
	) noexcept
	{
		if (!header) return;

		rapidjson::StringBuffer report;
		report::json_writer writer(report);

		writer.StartObject();
		writer.String("logs");
		writer.StartArray();
		error_report(error, properties, attachment).serialize(writer);
		writer.EndArray();
		writer.EndObject();

		curl_client::post(APP_CENTER_ENDPOINT, header, report.GetString());
	}

private:

	crashes() noexcept = default;
	crashes(crashes&&) noexcept = default;
	crashes(crashes const& other) noexcept = default;
};

struct analytics
{
	static void track_event(
		std::string const& name,
		report::dictionary const& properties
	) noexcept
	{
		if (!header) return;

		rapidjson::StringBuffer report;
		report::json_writer writer(report);

		writer.StartObject();
		writer.String("logs");
		writer.StartArray();
		event_report(name, properties).serialize(writer);
		writer.EndArray();
		writer.EndObject();

		curl_client::post(APP_CENTER_ENDPOINT, header, report.GetString());
	}

private:

	analytics() noexcept = default;
	analytics(analytics&&) noexcept = default;
	analytics(analytics const& other) noexcept = default;
};