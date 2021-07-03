#pragma once
#include "pch.h"
#include "error.h"

#define LOG_FILE_FORMAT L"{year.full}{month.integer(2)}{day.integer(2)}" \
						 "T{hour.integer(2)}{minute.integer(2)}{second.integer(2)}"
#define LOG_FORMAT L"{month.integer(2)}/{day.integer(2)}/{year.full} " \
					"{hour.integer(2)}:{minute.integer(2)}:{second.integer(2)}"

__declspec(selectany) winrt::Windows::Storage::StorageFile log_file = nullptr;

struct logger
{
	static winrt::fire_and_forget start(bool elevated) noexcept
	{
		auto local_folder = ApplicationData::Current().LocalFolder();
		auto log_folder = co_await local_folder.CreateFolderAsync(L"Logs", CreationCollisionOption::OpenIfExists);
		log_file = co_await log_folder.CreateFileAsync(
			get_time_stamp(LOG_FILE_FORMAT) +
			winrt::to_hstring(elevated  ? L"-extension.log" : L"-elevated-extension.log"),
			CreationCollisionOption::OpenIfExists
		);
	}

	static void log_error(winrt_error const& error) noexcept
	{
		std::wstring formatted_message = std::format(
			L"{} [Error] HResult Error {}: {}\n{}\n",
			get_time_stamp(LOG_FORMAT).c_str(),
			error.code().value,
			error.message().c_str(),
			error.stacktrace().c_str()
		);

		logger::print(formatted_message.c_str());

		if (log_file)
		{
			FileIO::AppendTextAsync(log_file, formatted_message).get();
		}
	}

	static void log_info(winrt::hstring const& message, bool console_only = false) noexcept
	{
		std::wstring formatted_message = std::format(
			L"{} [Info] {}\n",
			get_time_stamp(LOG_FORMAT).c_str(),
			message.c_str()
		);

		logger::print(formatted_message.c_str());

		if (!console_only && log_file)
		{
			FileIO::AppendTextAsync(log_file, formatted_message).get();
		}
	}

#ifdef _DEBUG
	static void print(winrt::hstring const& message, uint48_t sleep = 0) noexcept
	{
		wprintf_s(L"%ls", message.c_str());
		Sleep(sleep);
#else
	static void print(winrt::hstring const& message, uint48_t /* sleep */ = 0) noexcept
	{
#endif
		OutputDebugStringW(message.c_str());
	}

	static winrt::hstring get_time_stamp(winrt::hstring const& format) noexcept
	{
		return DateTimeFormatter(format).Format(winrt::clock::now());
	}

	static winrt::hstring get_utc_time_stamp(winrt::hstring const& format) noexcept
	{
		return DateTimeFormatter(format).Format(winrt::clock::now(), L"UTC");
	}

private:

	using FileIO = winrt::Windows::Storage::FileIO;
	using StorageFile = winrt::Windows::Storage::StorageFile;
	using ApplicationData = winrt::Windows::Storage::ApplicationData;
	using CreationCollisionOption = winrt::Windows::Storage::CreationCollisionOption;
	using DateTimeFormatter = winrt::Windows::Globalization::DateTimeFormatting::DateTimeFormatter;

	logger() noexcept = default;
	logger(logger&&) noexcept = default;
	logger(logger const& other) noexcept = default;
};