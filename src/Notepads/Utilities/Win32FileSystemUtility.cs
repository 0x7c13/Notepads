namespace Notepads.Utilities
{
    using System.IO;
    using Microsoft.Win32.SafeHandles;
    using System.Runtime.InteropServices;
    using System.Runtime.CompilerServices;
    using System;

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
            string message = null;
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
                                    message = Marshal.GetExceptionForHR(Marshal.GetHRForLastWin32Error()).Message;
                                }
                            }
                            else
                            {
                                message = Marshal.GetExceptionForHR(Marshal.GetHRForLastWin32Error()).Message;
                            }
                        }
                        catch (Exception ex)
                        {
                            message = ex.Message;
                        }
                        finally
                        {
                            hFile?.Dispose();
                        }
                    }
                }
            }
            return message;
        }
    }
}
