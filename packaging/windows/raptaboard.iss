; raptaboard's Windows installer (Inno Setup 6). It installs for this user
; only, with no administrator rights needed: into
; %LOCALAPPDATA%\Programs\raptaboard, with a Start menu shortcut, an optional
; desktop shortcut, and an uninstaller in Settings > Apps.
;
; scripts/package.py runs it; by hand:
;   iscc /DAppVersion=0.1.0 /DSourceDir=<folder with raptaboard.exe and docs>
;        /DOutputDir=dist packaging\windows\raptaboard.iss

#ifndef AppVersion
  #error Pass /DAppVersion=x.y.z
#endif
#ifndef SourceDir
  #error Pass /DSourceDir=<staged files>
#endif
#ifndef OutputDir
  #define OutputDir "dist"
#endif

[Setup]
; Keep AppId the same in every version: it's how an update finds the
; installed copy.
AppId={{F04C8F36-E297-4280-BC29-22CEFD768D61}
AppName=raptaboard
AppVersion={#AppVersion}
AppVerName=raptaboard {#AppVersion}
AppPublisher=FRC Team 3200 (Raptacon)
AppPublisherURL=https://github.com/tuffinmuffin/raptaboard
AppSupportURL=https://github.com/tuffinmuffin/raptaboard/issues
PrivilegesRequired=lowest
DefaultDirName={autopf}\raptaboard
DisableProgramGroupPage=yes
DisableDirPage=auto
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=raptaboard-{#AppVersion}-windows-x64-setup
SetupIconFile=..\..\media\icon\raptaboard.ico
UninstallDisplayIcon={app}\raptaboard.exe
UninstallDisplayName=raptaboard
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
; An update closes a running raptaboard first (its layout is saved by the
; user, not by the installer).
CloseApplications=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; Everything scripts/package.py staged (the program, README, changelog and
; licenses), so a file added there is installed too. The portable zip's
; .raptaboard settings folder isn't staged until after this runs.
Source: "{#SourceDir}\*"; DestDir: "{app}"; Excludes: "README.txt,.raptaboard"; Flags: ignoreversion
Source: "{#SourceDir}\README.txt"; DestDir: "{app}"; Flags: ignoreversion isreadme

[Icons]
Name: "{autoprograms}\raptaboard"; Filename: "{app}\raptaboard.exe"
Name: "{autodesktop}\raptaboard"; Filename: "{app}\raptaboard.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\raptaboard.exe"; Description: "{cm:LaunchProgram,raptaboard}"; Flags: nowait postinstall skipifsilent
