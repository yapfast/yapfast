; Yapfast's Windows installer. scripts/package-windows.py compiles it with
; Inno Setup 7 under Wine, from the same files as the portable ZIP.
;
; It installs for the person running it, without administrator rights, and can
; start Yapfast in the tray when they sign in. That is the Run key value
; Settings > Advanced > "Start when you sign in" writes (src/autostart.rs), so
; the installer and the setting turn the same thing on and off.

#ifndef AppVersion
  #error Compile through scripts/package-windows.py, which defines AppVersion, VersionNumber and SourceDir
#endif
; The full preview defines Edition and FileSuffix; the Discord release neither.
; Both share the AppId, so installing one replaces the other.
#ifdef Edition
  #define DisplayName "Yapfast (" + Edition + ")"
#else
  #define DisplayName "Yapfast"
  #define FileSuffix ""
#endif

[Setup]
; Windows knows an installed Yapfast by this; never change it.
AppId={{19FF8D53-8D7E-4E66-91BB-E2F5BE307E9A}
AppName=Yapfast
AppVersion={#AppVersion}
AppVerName={#DisplayName} {#AppVersion}
AppPublisher=Yapfast
AppPublisherURL=https://yapfast.com
AppSupportURL=https://github.com/yapfast/yapfast/issues
AppUpdatesURL=https://github.com/yapfast/yapfast/releases
VersionInfoVersion={#VersionNumber}
; Per user: %LOCALAPPDATA%\Programs\Yapfast, with no UAC prompt.
PrivilegesRequired=lowest
DefaultDirName={autopf}\Yapfast
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputBaseFilename=Yapfast-{#AppVersion}{#FileSuffix}-windows-x64-setup
SetupIconFile=yapfast.ico
UninstallDisplayIcon={app}\yapfast.exe
UninstallDisplayName={#DisplayName}
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
; Yapfast keeps running in the tray with its window closed, so an update or
; uninstall closes it rather than asking.
CloseApplications=force
RestartApplications=no
#ifdef LicenseFile
LicenseFile={#LicenseFile}
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
english.StartOnSignIn=Start Yapfast automatically when Windows starts
english.StartOnSignInGroup=Startup:

[Tasks]
Name: "startonsignin"; Description: "{cm:StartOnSignIn}"; GroupDescription: "{cm:StartOnSignInGroup}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; README.txt explains the ZIP; the installer needs no instructions.
Source: "{#SourceDir}\*"; Excludes: "README.txt"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Yapfast"; Filename: "{app}\yapfast.exe"
Name: "{autodesktop}\Yapfast"; Filename: "{app}\yapfast.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Yapfast"; ValueData: """{app}\yapfast.exe"" --background"; Tasks: startonsignin
; Chosen here, it starts even if Task Manager had turned it off before.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"; ValueType: none; ValueName: "Yapfast"; Flags: deletevalue dontcreatekey; Tasks: startonsignin
; Yapfast's own links open here; claiming Discord's stays the person's choice.
Root: HKCU; Subkey: "Software\Classes\yapfast"; ValueType: string; ValueName: ""; ValueData: "URL:Yapfast Protocol"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\yapfast"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\yapfast\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\yapfast.exe"",0"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\yapfast\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\yapfast.exe"" --open-link ""%1"""; Flags: uninsdeletekey
; Uninstalling stops starting at sign-in, however it was turned on.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Yapfast"; Flags: uninsdeletevalue dontcreatekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"; ValueType: none; ValueName: "Yapfast"; Flags: uninsdeletevalue dontcreatekey

[Run]
Filename: "{app}\yapfast.exe"; Description: "{cm:LaunchProgram,Yapfast}"; Flags: nowait postinstall skipifsilent
