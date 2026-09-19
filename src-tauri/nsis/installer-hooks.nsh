!macro NSIS_HOOK_PREINSTALL
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.miisodev.bard" "DisplayName"
  StrCmp $0 "" bard_no_previous_install
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "An existing installation of Bard was detected.$\r$\n$\r$\nClick Yes to repair or upgrade it — your clipboard history and settings are preserved.$\r$\nClick No to cancel." IDYES bard_proceed
  Abort
  bard_proceed:
  bard_no_previous_install:
!macroend