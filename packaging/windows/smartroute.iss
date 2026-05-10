#define MyAppName "SmartRoute"
#define MyAppVersion GetEnv("SMARTROUTE_VERSION")
#define MyAppPublisher "PA3MA3AH"
#define MyAppURL "https://github.com/PA3MA3AH/smartroute"
#define MyAppExeName "smartroute.exe"

#if MyAppVersion == ""
  #define MyAppVersion "0.1.0"
#endif

[Setup]
AppId={{2D0D0C92-FA6E-47C1-9E10-AB98BC70D8A6}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\SmartRoute
DefaultGroupName=SmartRoute
DisableProgramGroupPage=yes
LicenseFile=LICENSE
OutputDir=dist\windows-installer
OutputBaseFilename=SmartRoute-Setup-x64
SetupIconFile=packaging\windows\smartroute.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
ChangesEnvironment=yes
UninstallDisplayIcon={app}\{#MyAppExeName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "addtopath"; Description: "Add SmartRoute to PATH"; GroupDescription: "Windows integration:"; Flags: checkedonce
Name: "desktopicon"; Description: "Create desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "target\release\smartroute.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "packaging\windows\README-Windows.txt"; DestDir: "{app}"; Flags: ignoreversion

[Dirs]
Name: "{commonappdata}\SmartRoute\run"; Permissions: users-modify
Name: "{commonappdata}\SmartRoute\config"; Permissions: users-modify

[Icons]
Name: "{group}\SmartRoute PowerShell"; Filename: "powershell.exe"; Parameters: "-NoExit -ExecutionPolicy Bypass -Command cd '{app}'; .\smartroute.exe --help"; WorkingDir: "{app}"
Name: "{group}\SmartRoute README"; Filename: "{app}\README-Windows.txt"
Name: "{commondesktop}\SmartRoute PowerShell"; Filename: "powershell.exe"; Parameters: "-NoExit -ExecutionPolicy Bypass -Command cd '{app}'; .\smartroute.exe --help"; WorkingDir: "{app}"; Tasks: desktopicon

[Registry]
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Check: NeedsAddPath('{app}') and IsTaskSelected('addtopath')
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: string; ValueName: "SMARTROUTE_SINGBOX"; ValueData: "sing-box.exe"; Flags: preservestringtype

[Run]
Filename: "{app}\smartroute.exe"; Parameters: "--help"; Description: "Show SmartRoute help"; Flags: nowait postinstall skipifsilent

[Code]
function NeedsAddPath(Param: string): boolean;
var
  OldPath: string;
begin
  if not RegQueryStringValue(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', OldPath) then begin
    Result := true;
    exit;
  end;

  Result := Pos(';' + Uppercase(Param) + ';', ';' + Uppercase(OldPath) + ';') = 0;
end;
