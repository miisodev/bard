; Tauri 2 inserts exactly four hook macros into its generated installer.nsi:
; NSIS_HOOK_PREINSTALL, NSIS_HOOK_POSTINSTALL, NSIS_HOOK_PREUNINSTALL and
; NSIS_HOOK_POSTUNINSTALL. A macro under any other name compiles cleanly and
; never runs — electron-builder's customInstall/customUnInstall did exactly
; that here, and the Explorer verb they defined was never registered.
; Confirm against src-tauri/target/release/nsis/x64/installer.nsi.
;
; No hook raises a dialog. The updater installs in passive mode, and a prompt
; in the install section would stop every update on a modal; Tauri's own
; reinstall page already covers an interactive install over an existing one.

!macro NSIS_HOOK_POSTINSTALL
  ; A deliberately small shell integration: a familiar "Send to Bard" verb
  ; without a privileged Explorer extension. The application owns the actual
  ; file intake and validation. Windows 11 lists it under "Show more options".
  WriteRegStr HKCU "Software\Classes\*\shell\BardSend" "MUIVerb" "Send to Bard"
  WriteRegStr HKCU "Software\Classes\*\shell\BardSend" "Icon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\*\shell\BardSend\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Classes\*\shell\BardSend"
!macroend
