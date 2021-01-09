namespace Notepads.Settings
{
    internal static class SettingsKey
    {
        // App related
        internal const string AppVersionStr = "AppVersionStr";
        internal const string IsJumpListOutOfDateBool = "IsJumpListOutOfDateBool";
        internal const string ActiveInstanceIdStr = "ActiveInstanceIdStr";
        internal const string AlwaysOpenNewWindowBool = "AlwaysOpenNewWindowBool";

        // Theme related
        internal const string RequestedThemeStr = "RequestedThemeStr";
        internal const string AppBackgroundTintOpacityDouble = "AppBackgroundTintOpacityDouble";
        internal const string AppAccentColorHexStr = "AppAccentColorHexStr";
        internal const string CustomAccentColorHexStr = "CustomAccentColorHexStr";
        internal const string UseWindowsAccentColorBool = "UseWindowsAccentColorBool";

        // Editor related
        internal const string EditorFontFamilyStr = "EditorFontFamilyStr";
        internal const string EditorFontSizeInt = "EditorFontSizeInt";
        internal const string EditorFontStyleStr = "EditorFontStyleStr";
        internal const string EditorFontWeightUshort = "EditorFontWeightUshort";
        internal const string EditorDefaultTextWrappingStr = "EditorDefaultTextWrappingStr";
        internal const string EditorDefaultLineHighlighterViewStateBool = "EditorDefaultLineHighlighterViewStateBool";
        internal const string EditorDefaultLineEndingStr = "EditorDefaultLineEndingStr";
        internal const string EditorDefaultEncodingCodePageInt = "EditorDefaultEncodingCodePageInt";
        internal const string EditorDefaultDecodingCodePageInt = "EditorDefaultDecodingCodePageInt";
        internal const string EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool = "EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool";
        internal const string EditorDefaultTabIndentsInt = "EditorDefaultTabIndentsInt";
        internal const string EditorDefaultSearchEngineStr = "EditorDefaultSearchUrlStr";
        internal const string EditorCustomMadeSearchUrlStr = "EditorCustomMadeSearchUrlStr";
        internal const string EditorShowStatusBarBool = "EditorShowStatusBarBool";
        internal const string EditorEnableSessionBackupAndRestoreBool = "EditorEnableSessionBackupAndRestoreBool";
        internal const string EditorHighlightMisspelledWordsBool = "EditorHighlightMisspelledWordsBool";
        internal const string EditorDefaultDisplayLineNumbersBool = "EditorDefaultDisplayLineNumbersBool";
        internal const string EditorEnableSmartCopyBool = "EditorEnableSmartCopyBool";

        // Interop related
        // These values depend upon constant fields described in ..\Notepads.DesktopExtension\pch.h.
        // Changing value in one place require changing variable with similar name in another.
        internal const string AppCenterSecret = null;
        internal const string AppCenterInstallIdStr = "AppCenterInstallIdStr";
        internal const string LastChangedSettingsKeyStr = "LastChangedSettingsKeyStr";
        internal const string LastChangedSettingsAppInstanceIdStr = "LastChangedSettingsAppInstanceIdStr";
        internal const string LaunchElevetedProcessSuccessStr = "LaunchElevetedProcessSuccess";
        internal const string LaunchElevetedProcessFailedStr = "LaunchElevetedProcessFailed";
        internal const string DesktopExtensionLifetimeObjNameStr = "DesktopExtensionLifetimeObj";
        internal const string PackageSidStr = "PackageSidStr";
        internal const string AdminWriteEventNameStr = "NotepadsAdminWriteEvent";
        internal const string AdminWritePipeConnectionNameStr = "NotepadsAdminWritePipe";
        internal const string AdminRenameEventNameStr = "NotepadsAdminRenameEvent";
        internal const string AdminRenamePipeConnectionNameStr = "NotepadsAdminRenamePipe";
    }
}