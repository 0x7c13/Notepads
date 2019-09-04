
namespace Notepads.Services
{
    using System;
    using System.Threading.Tasks;

    public enum NotepadsOperationProtocol
    {
        Unrecognized,
        OpenNewInstance
    }

    public static class NotepadsProtocolService
    {
        public static NotepadsOperationProtocol GetOperationProtocol(Uri uri, out string context)
        {
            context = null;

            var uriStr = uri.ToString().Trim();

            if (string.IsNullOrEmpty(uriStr) || !uriStr.StartsWith("notepads://", StringComparison.OrdinalIgnoreCase))
            {
                return NotepadsOperationProtocol.Unrecognized;
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

            return NotepadsOperationProtocol.Unrecognized;
        }

        public static async Task<bool> LaunchProtocolAsync(NotepadsOperationProtocol operation)
        {
            if (operation == NotepadsOperationProtocol.Unrecognized)
            {
                return false;
            }
            else if (operation == NotepadsOperationProtocol.OpenNewInstance)
            {
                var uriToLaunch = "notepads://";
                return await Windows.System.Launcher.LaunchUriAsync(new Uri(uriToLaunch.ToLower()));
            }
            else
            {
                return false;
            }
        }
    }
}
