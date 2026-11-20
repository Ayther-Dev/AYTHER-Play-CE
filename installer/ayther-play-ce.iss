; ---------------------------------------------------------------------------
; ayther-play-ce.iss — Windows installer for AYTHER Play CE (Inno Setup 6).
;
; Do not compile this file by hand: tools/build-installer.ps1 builds Play CE,
; downloads and verifies the Runtime named by tools/runtime-lock.json, stages
; everything in one folder and passes these defines:
;
;   AppVersion   Play CE version (Cargo.toml)
;   RuntimeTag   Runtime release bundled with it
;   StageDir     folder with every file to install
;   OutputDir    where the setup .exe is written
;   RepoDir      repository root (icon)
;
; Per-user install, no administrator rights: {autopf} resolves to
; %LOCALAPPDATA%\Programs. Play CE and the Runtime share the install folder,
; which is where Play looks for ayther_runtime.exe first. The Visual C++
; runtime DLLs travel in the same folder (app-local deployment).
;
; User data (play_config.toml, saves, covers) lives in %APPDATA%\Ayther and
; %LOCALAPPDATA%; uninstalling never removes it.
; ---------------------------------------------------------------------------

#ifndef AppVersion
  #error AppVersion is required; run tools/build-installer.ps1
#endif

#define AppName "AYTHER Play CE"
#define AppExe "ayther_play_ce.exe"

[Setup]
; The AppId identifies the product for upgrades and uninstall: never change it.
AppId={{479E1261-DA67-4EB1-807F-7F99A6420D5E}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=AYTHER
AppComments=Runtime {#RuntimeTag} con Tessera
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=ayther-play-ce-{#AppVersion}-windows-x86_64-setup
SetupIconFile={#RepoDir}\resources\AYTHER-Play-CE.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; An upgrade over a running Play CE or Runtime closes them first.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
