namespace Notepads.Controls.CompareFiles
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.ApplicationModel.Resources;
    using Windows.Storage;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;

    public static class CompareArgs
    {
        /// <summary>
        ///  A reference back to the source page used to access XAML elements on the source page
        /// </summary>
        private static Page _sourcePage;
        private static readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        private static List<StorageFile> _files;
        private static bool _result;

        public static async Task ShowCompareWinUIAsync()
        {
            _result = false;
            _files = new List<StorageFile>();
            CompareFilesDialog nDialog = new CompareFilesDialog();

            await nDialog.ShowAsync();

            _result = nDialog.Result;
            _files.Add(nDialog.File1);
            _files.Add(nDialog.File2);
        }

        internal static IReadOnlyList<StorageFile> GetFiles()
        {
            return _files;
        }

        internal static bool GetResult()
        {
            return _result;
        }
    }
}
