Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
Name "Hycli"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\Hycli"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Icon "${ICON}"
UninstallIcon "${ICON}"
!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${STAGE}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\hycli-desktop.exe"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
Function .onInit
!if "${ARCH}" == "arm64"
  ${IfNot} ${IsNativeARM64}
!else
  ${IfNot} ${IsNativeAMD64}
!endif
    MessageBox MB_ICONSTOP "This Hycli installer is for ${ARCH}. Download the installer matching your computer."
    Abort
  ${EndIf}
FunctionEnd
Section "Hycli"
  SetShellVarContext current
  IfFileExists "$INSTDIR\hycli.exe" 0 install
  nsExec::ExecToLog '"$INSTDIR\hycli.exe" stop'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Hycli is busy. Finish or cancel active work, quit Hycli, and run setup again."
    Abort
  ${EndIf}
  install:
  SetOutPath "$INSTDIR"
  File /r "${STAGE}\*"
  CreateDirectory "$SMPROGRAMS\Hycli"
  CreateShortCut "$SMPROGRAMS\Hycli\Hycli.lnk" "$INSTDIR\hycli-desktop.exe" "" "$INSTDIR\hycli.ico"
  CreateShortCut "$SMPROGRAMS\Hycli\Quit Hycli.lnk" "$INSTDIR\hycli-desktop.exe" "stop" "$INSTDIR\hycli.ico"
  CreateShortCut "$DESKTOP\Hycli.lnk" "$INSTDIR\hycli-desktop.exe" "" "$INSTDIR\hycli.ico"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hycli" "DisplayName" "Hycli"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hycli" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hycli" "Publisher" "Hybirdss"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hycli" "DisplayIcon" "$INSTDIR\hycli.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hycli" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hycli" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hycli" "NoRepair" 1
SectionEnd
Section "Uninstall"
  SetShellVarContext current
  nsExec::ExecToLog '"$INSTDIR\hycli.exe" stop'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Hycli is busy. Finish or cancel active work before uninstalling."
    Abort
  ${EndIf}
  Delete "$DESKTOP\Hycli.lnk"
  Delete "$SMPROGRAMS\Hycli\Hycli.lnk"
  Delete "$SMPROGRAMS\Hycli\Quit Hycli.lnk"
  RMDir "$SMPROGRAMS\Hycli"
  Delete "$INSTDIR\hycli.exe"
  Delete "$INSTDIR\hycli-desktop.exe"
  Delete "$INSTDIR\hycli.ico"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hycli"
  # User data is outside INSTDIR and deliberately retained.
SectionEnd
