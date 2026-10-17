#ifndef AppBinary
  #define AppBinary "..\target\release\duet-clipboard.exe"
#endif
#ifndef AppVersion
  #define AppVersion "0.1.1"
#endif

[Setup]
AppId={{736A9FD9-50C7-43B8-9E1C-A9AA51AF943E}
AppName=Duet Clipboard
AppVersion={#AppVersion}
AppPublisher=Duet Clipboard contributors
DefaultDirName={localappdata}\Programs\DuetClipboard
DefaultGroupName=Duet Clipboard
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=DuetClipboard-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\duet-clipboard.exe
CloseApplications=yes
RestartApplications=no
SetupLogging=yes

[Files]
Source: "{#AppBinary}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\runtime\*.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\dist\runtime\LICENSE-LLVM.txt"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "RUST-STDLIB-NOTICES.html"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Duet Clipboard"; Filename: "{app}\duet-clipboard.exe"; AppUserModelID: "io.duetclipboard.app"

[Run]
Filename: "{app}\duet-clipboard.exe"; Description: "Open Duet Clipboard"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: files; Name: "{app}\*.dll"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "DuetClipboard"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\io.duetclipboard.app"; ValueType: string; ValueName: "DisplayName"; ValueData: "Duet Clipboard"; Flags: uninsdeletekey
