; Tawara's Windows installer, made with NSIS 3.13 (D36 decision 2;
; docs/RELEASE.md section 6.1). packaging/windows.sh runs makensis on this file
; and passes the defines listed below. The file holds no key, certificate or
; password. The signing certificate's thumbprint arrives as a define, read from
; the environment by the script, and it is used only when the script sets it.
;
; Defines, all set by packaging/windows.sh:
;   TAWARA_VERSION        the version people see, X.Y.Z (or X.Y.Z-dev)
;   TAWARA_FILE_VERSION   the numeric version, X.Y.Z.0
;   TAWARA_STAGE          the folder holding tawara-desktop.exe, LICENSE.md and
;                         THIRD-PARTY-NOTICES.txt
;   TAWARA_OUT            the installer file to write
;   TAWARA_PUBLISHER      the publisher name that Installed apps shows
; Optional:
;   TAWARA_ICO            an .ico file for the installer's and uninstaller's icons
;   TAWARA_WIN_CERT_SHA1  the signing certificate's SHA-1 thumbprint. With it, the
;                         uninstaller is signed as NSIS makes it, through
;                         !uninstfinalize. TAWARA_TIMESTAMP_URL and
;                         TAWARA_UNINST_STAMP are then required.
;   TAWARA_TIMESTAMP_URL  the certificate authority's RFC 3161 timestamp address
;   TAWARA_UNINST_STAMP   the file that the uninstaller signing command writes
;                         when it succeeds, so that the script can tell
;
; The program installs per user, into %LOCALAPPDATA%\Programs\Tawara, with no
; administrator prompt. The uninstaller removes only the program's files, the
; folder if that leaves it empty, the Start menu entry and the Installed apps
; entry. It never touches the wallet's store, which lives under
; %LOCALAPPDATA%\Tawara (D21), or the preferences (docs/RELEASE.md section 6.1).
;
; Wording awaits the owner: the pages use NSIS's English text, and the publisher
; and the file description are proposals (packaging/README.md, "Not decided").

Unicode true
SetCompressor zlib
RequestExecutionLevel user
ManifestDPIAware true

!ifndef TAWARA_VERSION
  !error "TAWARA_VERSION is not set: run packaging/windows.sh, which passes it"
!endif
!ifndef TAWARA_FILE_VERSION
  !error "TAWARA_FILE_VERSION is not set: run packaging/windows.sh, which passes it"
!endif
!ifndef TAWARA_STAGE
  !error "TAWARA_STAGE is not set: run packaging/windows.sh, which passes it"
!endif
!ifndef TAWARA_OUT
  !error "TAWARA_OUT is not set: run packaging/windows.sh, which passes it"
!endif
!ifndef TAWARA_PUBLISHER
  !error "TAWARA_PUBLISHER is not set: run packaging/windows.sh, which passes it"
!endif

!ifdef TAWARA_WIN_CERT_SHA1
  !ifndef TAWARA_TIMESTAMP_URL
    !error "TAWARA_WIN_CERT_SHA1 is set, so TAWARA_TIMESTAMP_URL must be set too"
  !endif
  !ifndef TAWARA_UNINST_STAMP
    !error "TAWARA_WIN_CERT_SHA1 is set, so TAWARA_UNINST_STAMP must be set too"
  !endif
  ; NSIS runs this command once the uninstaller is made, with "%1" naming that
  ; file. signtool is found on PATH, which packaging/windows.sh extends with the
  ; SDK's folder: a quoted path to it here would be split by cmd.exe, which runs
  ; the command. The thumbprint and the timestamp address are checked by the
  ; script before they reach this line.
  ; makensis does not stop when such a command fails, so the command also writes
  ; TAWARA_UNINST_STAMP on success, and packaging/windows.sh stops if it is
  ; missing.
  !uninstfinalize 'signtool sign /sha1 ${TAWARA_WIN_CERT_SHA1} /fd sha256 /tr ${TAWARA_TIMESTAMP_URL} /td sha256 "%1" && echo signed > "${TAWARA_UNINST_STAMP}"'
!endif

!define TAWARA_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Tawara"

Name "Tawara"
OutFile "${TAWARA_OUT}"
InstallDir "$LOCALAPPDATA\Programs\Tawara"

VIProductVersion "${TAWARA_FILE_VERSION}"
VIAddVersionKey "ProductName" "Tawara"
VIAddVersionKey "FileDescription" "Tawara installer"
VIAddVersionKey "FileVersion" "${TAWARA_FILE_VERSION}"
; NSIS warns (9100, which -WX makes fatal) when LegalCopyright is missing. The
; copyright line is the owner's wording, so the key is present and empty.
VIAddVersionKey "LegalCopyright" ""

!include "MUI2.nsh"

!ifdef TAWARA_ICO
  !define MUI_ICON "${TAWARA_ICO}"
  !define MUI_UNICON "${TAWARA_ICO}"
!endif

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; The per-user context: $SMPROGRAMS and the other shell folders resolve for the
; current user. SetShellVarContext is valid only inside a section or a function.
Function .onInit
  SetShellVarContext current
FunctionEnd

Function un.onInit
  SetShellVarContext current
FunctionEnd

Section "Tawara"
  SetOutPath "$INSTDIR"
  File "${TAWARA_STAGE}\tawara-desktop.exe"
  File "${TAWARA_STAGE}\LICENSE.md"
  File "${TAWARA_STAGE}\THIRD-PARTY-NOTICES.txt"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortCut "$SMPROGRAMS\Tawara.lnk" "$INSTDIR\tawara-desktop.exe" "" "$INSTDIR\tawara-desktop.exe" 0
  WriteRegStr HKCU "${TAWARA_UNINSTALL_KEY}" "DisplayName" "Tawara"
  WriteRegStr HKCU "${TAWARA_UNINSTALL_KEY}" "DisplayVersion" "${TAWARA_VERSION}"
  WriteRegStr HKCU "${TAWARA_UNINSTALL_KEY}" "Publisher" "${TAWARA_PUBLISHER}"
  WriteRegStr HKCU "${TAWARA_UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\tawara-desktop.exe"
  WriteRegStr HKCU "${TAWARA_UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${TAWARA_UNINSTALL_KEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegStr HKCU "${TAWARA_UNINSTALL_KEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  WriteRegDWORD HKCU "${TAWARA_UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${TAWARA_UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "un.Tawara"
  Delete "$INSTDIR\tawara-desktop.exe"
  Delete "$INSTDIR\LICENSE.md"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.txt"
  Delete "$SMPROGRAMS\Tawara.lnk"
  DeleteRegKey HKCU "${TAWARA_UNINSTALL_KEY}"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
