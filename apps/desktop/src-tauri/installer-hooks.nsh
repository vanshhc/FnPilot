!macro NSIS_HOOK_PREUNINSTALL
  ; Remove only this installation's login registration. User models/settings are preserved.
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "FnPilotPreview"
  StrCmp $0 '$\"$INSTDIR\fnpilot-desktop.exe$\"' 0 +2
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "FnPilotPreview"
!macroend
