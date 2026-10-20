#ifndef AppBinary
  #define AppBinary "..\target\release\cult-copy.exe"
#endif
#ifndef AppArchitecture
  #error Package with scripts/package-windows.ps1 to detect the executable architecture.
#endif
#ifndef AppVersion
  #error Package with scripts/package-windows.ps1 to supply the Cargo.toml version.
#endif

[Setup]
; The installer ID recovers existing install directories during upgrades.
AppId={{736A9FD9-50C7-43B8-9E1C-A9AA51AF943E}
AppName=Cult Copy
UninstallDisplayName=Cult Copy
AppVersion={#AppVersion}
AppPublisher=Cult Copy contributors
DefaultDirName={localappdata}\Programs\CultCopy
DefaultGroupName=Cult Copy
PrivilegesRequired=lowest
#if AppArchitecture == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=CultCopy-{#AppVersion}-windows-{#AppArchitecture}-setup
SetupIconFile=icons\cult-copy.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\cult-copy.exe
CloseApplications=yes
RestartApplications=no
SetupLogging=yes

[Files]
Source: "{#AppBinary}"; DestDir: "{app}"; DestName: "cult-copy.exe"; Flags: ignoreversion
Source: "{#AppRuntime}\*.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#AppRuntime}\LICENSE-LLVM.txt"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\NOTICE.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "RUST-STDLIB-NOTICES.html"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Cult Copy"; Filename: "{app}\cult-copy.exe"; AppUserModelID: "io.duetclipboard.app"

[InstallDelete]
Type: files; Name: "{autoprograms}\Duet Clipboard.lnk"
Type: files; Name: "{app}\duet-clipboard.exe"

[Run]
Filename: "{app}\cult-copy.exe"; Description: "Open Cult Copy"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: files; Name: "{app}\*.dll"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "DuetClipboard"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\io.duetclipboard.app"; ValueType: string; ValueName: "DisplayName"; ValueData: "Cult Copy"; Flags: uninsdeletekey
