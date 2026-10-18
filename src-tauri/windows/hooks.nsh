; Bloom's installer and uninstaller hooks (bundle > windows > nsis > installerHooks).

; Asks a running Bloom to quit the normal way, which puts the taskbar and
; Windows' volume, brightness and notification pop-ups back, before the
; installer stops it by force. Versions without this signal are stopped by
; force as before (their own watchdog then restores the taskbar).
!macro BLOOM_ASK_TO_QUIT
  ; EVENT_MODIFY_STATE
  System::Call 'kernel32::OpenEventW(i 0x0002, i 0, w "BloomQuit") p.r0'
  ${If} $0 != 0
    System::Call 'kernel32::SetEvent(p r0)'
    System::Call 'kernel32::CloseHandle(p r0)'
    Sleep 2500
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro BLOOM_ASK_TO_QUIT
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ; Uninstalling: hand Windows back before Bloom's files go. This quits a
    ; running Bloom, then restores the taskbar with its own auto-hide setting
    ; and position, the volume and brightness pop-ups and the notification
    ; pop-ups, and removes Bloom's startup task.
    ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --restore-windows'
      Pop $0
    ${EndIf}
  ${Else}
    ; Updating: the new version takes over, so only ask the old one to quit.
    !insertmacro BLOOM_ASK_TO_QUIT
  ${EndIf}
!macroend
