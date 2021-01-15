namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using System.IO;
    using System.Runtime.InteropServices;
    using System.Runtime.CompilerServices;
    using Microsoft.AppCenter.Crashes;
    using Microsoft.AppCenter.Analytics;
    using Microsoft.Toolkit.Uwp.Helpers;
    using Microsoft.Win32.SafeHandles;

    public static class Win32FileSystemUtility
    {
        [DllImport("api-ms-win-core-file-l2-1-0.dll", CharSet = CharSet.Auto,
        CallingConvention = CallingConvention.StdCall,
        SetLastError = true)]
        public unsafe static extern bool GetFileInformationByHandleEx(
            SafeFileHandle hFile,
            FILE_INFO_BY_HANDLE_CLASS FileInformationClass,
            byte* lpFileInformation,
            uint dwBufferSize
        );

        [DllImport("api-ms-win-core-file-fromapp-l1-1-0.dll", CharSet = CharSet.Auto,
        CallingConvention = CallingConvention.StdCall,
        SetLastError = true)]
        public static extern bool SetFileAttributesFromApp(
            string lpFileName,
            uint dwFileAttributes
        );

        [DllImport("api-ms-win-core-file-l2-1-0.dll", CharSet = CharSet.Auto,
        CallingConvention = CallingConvention.StdCall,
        SetLastError = true)]
        public unsafe static extern bool SetFileInformationByHandle(
            SafeFileHandle hFile,
            FILE_INFO_BY_HANDLE_CLASS FileInformationClass,
            byte* lpFileInformation,
            uint dwBufferSize
        );

        public enum FILE_INFO_BY_HANDLE_CLASS
        {
            FileBasicInfo,
            FileStandardInfo,
            FileNameInfo,
            FileRenameInfo,
            FileDispositionInfo,
            FileAllocationInfo,
            FileEndOfFileInfo,
            FileStreamInfo,
            FileCompressionInfo,
            FileAttributeTagInfo,
            FileIdBothDirectoryInfo,
            FileIdBothDirectoryRestartInfo,
            FileIoPriorityHintInfo,
            FileRemoteProtocolInfo,
            FileFullDirectoryInfo,
            FileFullDirectoryRestartInfo,
            FileStorageInfo,
            FileAlignmentInfo,
            FileIdInfo,
            FileIdExtdDirectoryInfo,
            FileIdExtdDirectoryRestartInfo,
            FileDispositionInfoEx,
            FileRenameInfoEx,
            FileCaseSensitiveInfo,
            FileNormalizedNameInfo,
            MaximumFileInfoByHandleClass
        }

        [StructLayout(LayoutKind.Sequential)]
        public unsafe struct FILE_BASIC_INFO
        {
            public LARGE_INTEGER CreationTime;
            public LARGE_INTEGER LastAccessTime;
            public LARGE_INTEGER LastWriteTime;
            public LARGE_INTEGER ChangeTime;
            public uint FileAttributes;
        }

        [StructLayout(LayoutKind.Explicit, Size = 8)]
        public unsafe struct LARGE_INTEGER
        {
            [FieldOffset(0)] public Int64 QuadPart;
            [FieldOffset(0)] public UInt32 LowPart;
            [FieldOffset(4)] public Int32 HighPart;
        }

        public static FileAttributes GetFileAttributes(this Windows.Storage.IStorageFile file)
        {
            FileAttributes fileAttributes = 0;
            unsafe
            {
                var size = Marshal.SizeOf<FILE_BASIC_INFO>();
                var buff = new byte[size];
                fixed (byte* fileInformationBuff = buff)
                {
                    ref var fileInformation = ref Unsafe.As<byte, FILE_BASIC_INFO>(ref buff[0]);
                    SafeFileHandle hFile = null;

                    try
                    {
                        hFile = file.CreateSafeFileHandle(FileAccess.Read);
                        if (GetFileInformationByHandleEx(hFile, FILE_INFO_BY_HANDLE_CLASS.FileBasicInfo, fileInformationBuff, (uint)size))
                        {
                            fileAttributes = (FileAttributes)fileInformation.FileAttributes;
                        }
                        else
                        {
                            var e = Marshal.GetExceptionForHR(Marshal.GetHRForLastWin32Error());

                            var diagnosticInfo = new Dictionary<string, string>()
                            {
                                { "Message", e.Message },
                                { "Exception", e.ToString() },
                                { "Culture", SystemInformation.Culture.EnglishName },
                                { "AvailableMemory", SystemInformation.AvailableMemory.ToString("F0") },
                                { "OSArchitecture", SystemInformation.OperatingSystemArchitecture.ToString() },
                                { "OSVersion", SystemInformation.OperatingSystemVersion.ToString() },
                                { "FileType", Path.GetExtension(file.Path) }
                            };

                            var attachment = ErrorAttachmentLog.AttachmentWithText(
                                $"Exception: {e}, " +
                                $"Message: {e.Message}, " +
                                $"InnerException: {e.InnerException}, " +
                                $"InnerExceptionMessage: {e.InnerException?.Message}",
                                "FileAttribuesFetchException");

                            Analytics.TrackEvent("OnFileAttribuesFetchException", diagnosticInfo);
                            Crashes.TrackError(e, diagnosticInfo, attachment);
                        }
                    }
                    catch (Exception e)
                    {
                        var diagnosticInfo = new Dictionary<string, string>()
                        {
                            { "Message", e.Message },
                            { "Exception", e.ToString() },
                            { "Culture", SystemInformation.Culture.EnglishName },
                            { "AvailableMemory", SystemInformation.AvailableMemory.ToString("F0") },
                            { "OSArchitecture", SystemInformation.OperatingSystemArchitecture.ToString() },
                            { "OSVersion", SystemInformation.OperatingSystemVersion.ToString() },
                            { "FileType", Path.GetExtension(file.Path) }
                        };

                        var attachment = ErrorAttachmentLog.AttachmentWithText(
                            $"Exception: {e}, " +
                            $"Message: {e.Message}, " +
                            $"InnerException: {e.InnerException}, " +
                            $"InnerExceptionMessage: {e.InnerException?.Message}",
                            "FileAttribuesFetchException");

                        Analytics.TrackEvent("OnFileAttribuesFetchException", diagnosticInfo);
                        Crashes.TrackError(e, diagnosticInfo, attachment);
                    }
                    finally
                    {
                        hFile?.Dispose();
                    }
                }
            }
            return fileAttributes;
        }

        public static string SetFileAttributes(this Windows.Storage.IStorageFile file, FileAttributes fileAttributes)
        {
            Exception ex = null;
            if (!SetFileAttributesFromApp(file.Path, (uint)fileAttributes))
            {
                unsafe
                {
                    var size = Marshal.SizeOf<FILE_BASIC_INFO>();
                    var buff = new byte[size];
                    fixed (byte* fileInformationBuff = buff)
                    {
                        ref var fileInformation = ref Unsafe.As<byte, FILE_BASIC_INFO>(ref buff[0]);
                        SafeFileHandle hFile = null;

                        try
                        {
                            hFile = file.CreateSafeFileHandle();

                            if (GetFileInformationByHandleEx(hFile, FILE_INFO_BY_HANDLE_CLASS.FileBasicInfo, fileInformationBuff, (uint)size))
                            {
                                fileInformation.FileAttributes = (uint)fileAttributes;
                                if (!SetFileInformationByHandle(hFile, FILE_INFO_BY_HANDLE_CLASS.FileBasicInfo, fileInformationBuff, (uint)size))
                                {
                                    ex = Marshal.GetExceptionForHR(Marshal.GetHRForLastWin32Error());
                                }
                            }
                            else
                            {
                                ex = Marshal.GetExceptionForHR(Marshal.GetHRForLastWin32Error());
                            }
                        }
                        catch (Exception e)
                        {
                            ex = e;
                        }
                        finally
                        {
                            hFile?.Dispose();
                        }
                    }
                }
            }

            if (ex != null)
            {
                var diagnosticInfo = new Dictionary<string, string>()
                {
                    { "Message", ex.Message },
                    { "Exception", ex.ToString() },
                    { "Culture", SystemInformation.Culture.EnglishName },
                    { "AvailableMemory", SystemInformation.AvailableMemory.ToString("F0") },
                    { "OSArchitecture", SystemInformation.OperatingSystemArchitecture.ToString() },
                    { "OSVersion", SystemInformation.OperatingSystemVersion.ToString() },
                    { "FileType", Path.GetExtension(file.Path) }
                };

                var attachment = ErrorAttachmentLog.AttachmentWithText(
                    $"Exception: {ex}, " +
                    $"Message: {ex.Message}, " +
                    $"InnerException: {ex.InnerException}, " +
                    $"InnerExceptionMessage: {ex.InnerException?.Message}",
                    "FileAttribuesSetException");

                Analytics.TrackEvent("OnFileAttribuesSetException", diagnosticInfo);
                Crashes.TrackError(ex, diagnosticInfo, attachment);
            }

            return ex?.Message;
        }
    }
}
