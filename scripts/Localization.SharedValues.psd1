# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Reviewed words which are legitimately identical to English. Keep these scoped
# to the resource, spelling and locale so new English fallback copies fail CI.
# English and Chinese are the semantic references for translation review.
@{
    'Resources.resw/EditorTools_FindAllQuery' = @{ Value = '“{0}”'; Locales = @('*') }
    'Resources.resw/EditorTools_FindAllText' = @{ Value = 'Text'; Locales = @('cs-CZ', 'de-CH', 'de-DE') }
    'Resources.resw/FileOpenErrorDialog_PrimaryButtonText' = @{ Value = 'OK'; Locales = @('cs-CZ', 'de-CH', 'de-DE', 'es-ES', 'fi-FI', 'fr-FR', 'hu-HU', 'it-IT', 'ja-JP', 'nl-NL', 'pl-PL', 'pt-BR', 'pt-PT', 'vi-VN') }
    'Resources.resw/FileSaveErrorDialog_PrimaryButtonText' = @{ Value = 'OK'; Locales = @('cs-CZ', 'de-CH', 'de-DE', 'es-ES', 'fi-FI', 'fr-FR', 'hu-HU', 'it-IT', 'ja-JP', 'nl-NL', 'pl-PL', 'pt-BR', 'pt-PT', 'vi-VN') }
    'Resources.resw/FontStyle_Normal' = @{ Value = 'Normal'; Locales = @('de-CH', 'de-DE', 'es-ES', 'fr-FR', 'pt-PT', 'tr-TR') }
    'Resources.resw/FontStyle_Oblique' = @{ Value = 'Oblique'; Locales = @('fr-FR') }
    'Resources.resw/FontWeight_Normal' = @{ Value = 'Normal'; Locales = @('de-CH', 'de-DE', 'es-ES', 'fr-FR', 'pt-PT', 'tr-TR') }
    'Resources.resw/MainMenu_AutomationName' = @{ Value = 'Menu'; Locales = @('fr-FR', 'it-IT', 'nl-NL', 'pl-PL', 'pt-BR', 'pt-PT', 'vi-VN') }
    'Resources.resw/TextEditor_Indentation_Tabs' = @{ Value = 'Tabs'; Locales = @('nl-NL') }
    'Resources.resw/TextEditor_LineColumnIndicator_FullText' = @{ Value = 'Ln {0}, Col {1} ({2} {3})'; Locales = @('es-ES', 'fr-FR', 'pt-BR', 'pt-PT') }
    'Resources.resw/TextEditor_LineColumnIndicator_ShortText' = @{ Value = 'Ln {0}, Col {1}'; Locales = @('es-ES', 'fr-FR', 'pt-BR', 'pt-PT') }
    'Settings.resw/PersonalizationPage_FontRendering_ClearType.Content' = @{ Value = 'ClearType'; Locales = @('*') }
    'Settings.resw/TextAndEditorPage_DecodingSettings_Utf8RadioButton.Content' = @{ Value = 'UTF-8'; Locales = @('*') }
    'Settings.resw/TextAndEditorPage_Title.Content' = @{ Value = 'Text & Editor'; Locales = @('de-CH', 'de-DE') }
}
