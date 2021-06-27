#pragma once
#include "pch.h"
#include "logger.h"
#include "curl/curl.h"

struct curl_slist_handle_traits
{
	using type = struct curl_slist*;

	static void close(type value) noexcept
	{
		curl_slist_free_all(value);
	}

	static constexpr type invalid() noexcept
	{
		return nullptr;
	}
};

struct curl_handle_traits
{
	using type = CURL*;

	static void close(type value) noexcept
	{
		curl_easy_cleanup(value);
	}

	static constexpr type invalid() noexcept
	{
		return nullptr;
	}
};

using curl_slist_handle = winrt::handle_type<curl_slist_handle_traits>;
using curl_handle = winrt::handle_type<curl_handle_traits>;

struct curl_client
{
	static void post(char const* url, curl_slist_handle const& header, std::string const& data) noexcept
	{
		curl_handle curl{ curl_easy_init() };
		curl_easy_setopt(curl.get(), CURLOPT_URL, url);
		curl_easy_setopt(curl.get(), CURLOPT_HTTPHEADER, header.get());
		curl_easy_setopt(curl.get(), CURLOPT_COPYPOSTFIELDS, data.c_str());

#ifdef  _DEBUG
		curl_easy_setopt(curl.get(), CURLOPT_DEBUGFUNCTION, debug_callback);
		curl_easy_setopt(curl.get(), CURLOPT_VERBOSE, 1L);
#endif

		auto res = curl_easy_perform(curl.get());
		if (res != CURLE_OK)
		{
			logger::log_info(
				std::format(
					L"curl_easy_perform() failed: {}",
					winrt::to_hstring(curl_easy_strerror(res)).c_str()
				).c_str()
			);
		}
	}

private:

#ifdef  _DEBUG
	static int debug_callback(curl_handle /* handle */, curl_infotype type, char* data, size_t size, void* /* userp */)
	{
		std::string curl_data{ data, size };
		if (curl_data.empty()) return 0;

		std::string log_data;
		std::string time_stamp = winrt::to_string(logger::get_time_stamp(LOG_FORMAT));

		switch (type)
		{
		case CURLINFO_TEXT:
			log_data = std::format("{} [CURL] {}", time_stamp, curl_data);
			break;
		default:
			log_data = std::format("{} [CURL] Unknown Data: {}", time_stamp, curl_data);
			break;
		case CURLINFO_HEADER_OUT:
			log_data = std::format("\n\n{} [CURL] Request Headers: \n> {}", time_stamp, curl_data);
			break;
		case CURLINFO_DATA_OUT:
			log_data = std::format("\n{} [CURL] Request Data: \n{}\n\n\n", time_stamp, curl_data);
			break;
		case CURLINFO_SSL_DATA_OUT:
			log_data = std::format("\n{} [CURL] Request SSL Data: \n{}\n\n\n", time_stamp, curl_data);
			break;
		case CURLINFO_HEADER_IN:
			log_data = std::format("\n{} [CURL] Response Headers: \n< {}", time_stamp, curl_data);
			break;
		case CURLINFO_DATA_IN:
			log_data = std::format("\n\n{} [CURL] Response Data: \n{}\n\n\n", time_stamp, curl_data);
			break;
		case CURLINFO_SSL_DATA_IN:
			log_data = std::format("\n\n{} [CURL] Response SSL Data: \n{}\n\n\n", time_stamp, curl_data);
			break;
		}

		logger::print(winrt::to_hstring(log_data));
		return 0;
	}
#endif

	curl_client() noexcept = default;
	curl_client(curl_client&&) = default;
	curl_client(curl_client const& other) noexcept = default;
};