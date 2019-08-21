
namespace Notepads.Services
{
    using System;
    using System.Threading.Tasks;
    using Windows.System;

    public enum NotepadsOperationProtocol
    {
        Invalid,
        OpenNewInstance,
        OpenFileDraggedOutside,
        CloseEditor,
    }

    public static class NotepadsProtocolService
    {
        public static NotepadsOperationProtocol GetOperationProtocol(Uri uri, out string context)
        {
            context = null;

            var uriStr = uri.ToString().Trim();

            if (string.IsNullOrEmpty(uriStr) || !uriStr.StartsWith("notepads://", StringComparison.InvariantCultureIgnoreCase))
            {
                return NotepadsOperationProtocol.Invalid;
            }

            var operation = uriStr.Substring("notepads://".Length);

            if (operation.EndsWith("/"))
            {
                operation = operation.Remove(operation.Length - 1);
            }

            if (string.IsNullOrEmpty(operation))
            {
                return NotepadsOperationProtocol.OpenNewInstance;
            }

            if (operation.StartsWith("OnFileDraggedOutside/", StringComparison.InvariantCultureIgnoreCase))
            {
                context = operation.Substring("OnFileDraggedOutside/".Length);
                return NotepadsOperationProtocol.OpenFileDraggedOutside;
            }

            if (operation.StartsWith("CloseEditor/", StringComparison.InvariantCultureIgnoreCase))
            {
                context = operation.Substring("CloseEditor/".Length);
                return NotepadsOperationProtocol.CloseEditor;
            }

            return NotepadsOperationProtocol.Invalid;
        }

        public static async Task<bool> LaunchProtocolAsync(NotepadsOperationProtocol operation, string appInstanceId = null, string editorGuid = null, string fileToken = null)
        {
            var uriToLaunch = "notepads://";

            switch (operation)
            {
                case NotepadsOperationProtocol.OpenFileDraggedOutside:
                    uriToLaunch += $"OnFileDraggedOutside/{appInstanceId}:{editorGuid}:{fileToken}";
                    break;
                case NotepadsOperationProtocol.CloseEditor:
                    uriToLaunch += $"CloseEditor/{appInstanceId}:{editorGuid}";
                    break;
            }

            return await Windows.System.Launcher.LaunchUriAsync(new Uri(uriToLaunch.ToLower()), new LauncherOptions()
            {
                
            });
        }
    }
}
