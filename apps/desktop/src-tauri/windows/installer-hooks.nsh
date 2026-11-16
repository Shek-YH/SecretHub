; Claude Code / Claude Desktop / Cursor may be running secrethub-mcp.exe (stdio MCP server), which locks the file and
; makes the installer fail with "Error opening file for writing". Tauri only closes the main exe, so end the sidecar here.
; Clients start it again on the next MCP call, so this is safe. taskkill returns non-zero when no process exists; ignored.
!macro SecretHubStopMcp
  nsExec::Exec 'taskkill /F /T /IM secrethub-mcp.exe'
  Pop $0
  Sleep 500
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro SecretHubStopMcp
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro SecretHubStopMcp
!macroend
