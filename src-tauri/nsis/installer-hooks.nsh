!macro NSIS_HOOK_PREINSTALL
  ; Tauri 2's current-user NSIS installer registers under Uninstall\Bard.
  ; The old identifier key never existed on a clean install, which made the
  ; upgrade prompt silently unreachable.
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Bard" "DisplayName"
  StrCmp $0 "" bard_no_previous_install
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "An existing installation of Bard was detected.$\r$\n$\r$\nClick Yes to repair or upgrade it — your clipboard history and settings are preserved.$\r$\nClick No to cancel." IDYES bard_proceed
  Abort
  bard_proceed:
  bard_no_previous_install:
!macroend

!macro customInstall
  ; A deliberately small shell integration: Bard appears as a familiar
  ; "Send to Bard" verb without installing a privileged Explorer extension.
  ; The application owns the actual file intake and validation.
  WriteRegStr HKCU "Software\Classes\*\shell\BardSend" "MUIVerb" "Send to Bard"
  WriteRegStr HKCU "Software\Classes\*\shell\BardSend" "Icon" "$INSTDIR\Bard.exe,0"
  WriteRegStr HKCU "Software\Classes\*\shell\BardSend\command" "" '"$INSTDIR\Bard.exe" "%1"'
!macroend

!macro customUnInstall
  DeleteRegKey HKCU "Software\Classes\*\shell\BardSend\command"
  DeleteRegKey HKCU "Software\Classes\*\shell\BardSend"
!macroend
