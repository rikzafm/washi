; Washi Note installer (Inno Setup 6). Build it with `pwsh tools/build-installer.ps1`, which passes
; the version from washi/Cargo.toml, the release exe and the setup icon.
;
; Per-user install: no administrator prompt. The app goes to %LOCALAPPDATA%\Programs\Washi; notes,
; settings and installed fonts stay in %APPDATA%\Washi and are never touched by setup or uninstall.
; Washi finds the uninstaller as unins000.exe beside washi.exe (Settings > General > Uninstall).

#ifndef AppVersion
  #error Pass /DAppVersion=x.y.z (tools/build-installer.ps1 does)
#endif
#ifndef ExePath
  #define ExePath "..\target\release\washi.exe"
#endif
#ifndef OutputDir
  #define OutputDir "..\target\installer"
#endif

[Setup]
; Identifies Washi to Windows across versions: never change it.
AppId={{6F0B7C52-3E1A-4D0B-9C57-2A8E5B1D7F34}
AppName=Washi Note
AppVersion={#AppVersion}
AppVerName=Washi Note {#AppVersion}
AppPublisher=Washi
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\Washi
DefaultGroupName=Washi Note
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
LicenseFile=..\LICENSE
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
OutputDir={#OutputDir}
OutputBaseFilename=WashiNote-{#AppVersion}-setup
#ifdef SetupIcon
SetupIconFile={#SetupIcon}
#endif
UninstallDisplayIcon={app}\washi.exe
UninstallDisplayName=Washi Note
; The single-instance mutex of washi/src/win/instance.rs ("Local\" is the default namespace):
; setup and the uninstaller ask for Washi to be closed instead of failing on a locked exe.
AppMutex=Washi-SingleInstance
CloseApplications=no

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; DestName: "washi.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Washi Note"; Filename: "{app}\washi.exe"
Name: "{autodesktop}\Washi Note"; Filename: "{app}\washi.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\washi.exe"; Description: "{cm:LaunchProgram,Washi Note}"; Flags: nowait postinstall skipifsilent

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  RunValue = 'Washi';

{ "Start with Windows" (washi/src/win/startup.rs) writes a per-user Run value. Remove it with the
  app, but only when it points at this install: a portable washi.exe elsewhere keeps its own. }
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
begin
  if CurUninstallStep = usUninstall then
    if RegQueryStringValue(HKCU, RunKey, RunValue, Command) then
      if Pos(Lowercase(ExpandConstant('{app}\')), Lowercase(Command)) > 0 then
        RegDeleteValue(HKCU, RunKey, RunValue);
end;
