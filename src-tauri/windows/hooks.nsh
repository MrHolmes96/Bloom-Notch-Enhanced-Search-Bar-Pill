; Bloom's installer and uninstaller hooks (bundle > windows > nsis > installerHooks).
;
; Bloom runs alongside Explorer, which stays Windows' shell (so every native
; feature keeps working), and takes over its taskbar while it runs. The
; installer never touches Windows' shell setting; it only hands the taskbar
; between Bloom versions, adds a way back to plain Windows, and gives Windows
; back on uninstall.

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

; Updating: a running Bloom that knows how hands its hidden taskbar to the new
; version instead of showing it, so Windows' taskbar never appears during an
; update (a copy of Bloom in the temp folder shows it after all if no Bloom
; takes it over within 3 minutes). Older versions are asked to quit.
!macro BLOOM_HAND_OVER
  System::Call 'kernel32::OpenEventW(i 0x0002, i 0, w "BloomUpdate") p.r0'
  ${If} $0 != 0
    System::Call 'kernel32::SetEvent(p r0)'
    System::Call 'kernel32::CloseHandle(p r0)'
    Sleep 2500
  ${Else}
    !insertmacro BLOOM_ASK_TO_QUIT
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro BLOOM_HAND_OVER
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; A way back to plain Windows that needs no taskbar: press the Windows key
  ; and type "Bloom Recovery" (or run "Bloom.exe --recovery" from Task
  ; Manager's Run new task).
  CreateShortCut "$SMPROGRAMS\Bloom Recovery.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "--recovery" "$INSTDIR\${MAINBINARYNAME}.exe" 0
  ; A silent or passive update (no finish page to start it from): the new
  ; Bloom starts now, as the signed-in user, and takes the taskbar over.
  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
    nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" "--autostart"
  ${EndIf}
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
    ; Updating: the new version takes over (it was handed the taskbar).
    !insertmacro BLOOM_HAND_OVER
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    Delete "$SMPROGRAMS\Bloom Recovery.lnk"
  ${EndIf}
!macroend
