#pragma once
#include "pch.h"
#include "error.h"
#include "device.h"
#include "error_report.h"
#include "event_report.h"
#include "settings_key.h"
#include "winrt/Windows.Web.Http.h"
#include "winrt/Windows.Web.Http.Headers.h"

// Documentation is at https://docs.microsoft.com/en-us/appcenter/diagnostics/upload-crashes
#define APP_CENTER_ENDPOINT L"https://in.appcenter.ms/logs?Api-Version=1.0.0"
const winrt::Windows::Foundation::Uri app_center_uri{ APP_CENTER_ENDPOINT };

__declspec(selectany) winrt::Windows::Web::Http::HttpClient client{};

struct appcenter
{
	static void start() noexcept
	{
		if (!APP_CENTER_SECRET || wcslen(APP_CENTER_SECRET) == 0) return;
		auto install_id = winrt::unbox_value_or<winrt::hstring>(settings_key::read(APP_CENTER_INSTALL_ID_STR), L"");
		if (install_id.empty()) return;

		client.DefaultRequestHeaders().Append(L"app-secret", APP_CENTER_SECRET);
		client.DefaultRequestHeaders().Append(L"install-id", install_id);
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
		if (!client.DefaultRequestHeaders().HasKey(L"app-secret") || !client.DefaultRequestHeaders().HasKey(L"install-id")) return;

		auto report = report::json_object();
		report.Insert(L"logs", error_report(error, properties, attachment).to_json());
		auto content = HttpStringContent(report.Stringify());
		auto response = client.TryPostAsync(app_center_uri, content).get();
	}

private:
	using HttpStringContent = winrt::Windows::Web::Http::HttpStringContent;

	crashes() noexcept = default;
	crashes(crashes&&) noexcept = default;
	crashes(crashes const& other) noexcept = default;
};

struct analytics
{
	static void track_event(
		winrt::hstring const& name,
		report::dictionary const& properties
	) noexcept
	{
		if (!client.DefaultRequestHeaders().HasKey(L"app-secret") || !client.DefaultRequestHeaders().HasKey(L"install-id")) return;

		auto report = report::json_object();
		report.Insert(L"logs", event_report(name, properties).to_json());
		auto content = HttpStringContent(report.Stringify());
		auto response = client.TryPostAsync(app_center_uri, content).get();
	}

private:
	using HttpStringContent = winrt::Windows::Web::Http::HttpStringContent;

	analytics() noexcept = default;
	analytics(analytics&&) noexcept = default;
	analytics(analytics const& other) noexcept = default;
};