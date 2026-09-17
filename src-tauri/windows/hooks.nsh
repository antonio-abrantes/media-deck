; MediaDeck NSIS hooks — keep install/uninstall scoped to the current user.
; Autostart uses HKCU\Software\Microsoft\Windows\CurrentVersion\Run\MediaDeck.

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Remove optional per-user autostart before files are deleted.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "MediaDeck"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; User data under %LOCALAPPDATA%\MediaDeck is intentionally retained so that
  ; library, artwork cache, labels, and backups survive reinstall. Operators may
  ; delete that folder manually after uninstall if a full wipe is required.
!macroend
