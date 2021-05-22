#pragma once
#include "pch.h"

struct settings_key
{
	using IInspectable = winrt::Windows::Foundation::IInspectable;
	using ApplicationData = winrt::Windows::Storage::ApplicationData;

	static IInspectable read(winrt::hstring key) noexcept
	{
		return ApplicationData::Current().LocalSettings().Values().TryLookup(key);
	}

	static void write(winrt::hstring key, IInspectable data) noexcept
	{
		ApplicationData::Current().LocalSettings().Values().Insert(key, data);
	}

	static void signal_changed() noexcept
	{
		ApplicationData::Current().SignalDataChanged();
	}

private:

	settings_key() noexcept = default;
	settings_key(settings_key&&) noexcept = default;
	settings_key(settings_key const& other) noexcept = default;
};