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
		logger::print(L"\n\n\n");
		logger::print(winrt::to_hstring(data), 2000);
		logger::print(L"\n\n\n");
		curl_easy_setopt(curl.get(), CURLOPT_VERBOSE, 1L);
#endif

		auto res = curl_easy_perform(curl.get());
		if (res != CURLE_OK)
		{
			logger::print(
				fmt::format(
					L"curl_easy_perform() failed: {}",
					winrt::to_hstring(curl_easy_strerror(res)
					)
				).c_str()
			);
		}
	}

private:

	curl_client() noexcept = default;
	curl_client(curl_client&&) = default;
	curl_client(curl_client const& other) noexcept = default;
};