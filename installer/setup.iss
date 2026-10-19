; IDG – Internet Download GO  –  Inno Setup 6 script
; Build: ISCC installer\setup.iss
; Produces: IDG-Setup-1.0.0.exe

#define AppName      "IDG – Internet Download GO"
#define AppVersion   "1.0.0"
#define AppPublisher "IDG Project"
#define AppURL       "https://github.com/Mustafa1p/IDG-internet-download-go"
#define AppExeName   "IDG.exe"
#define AppId        "{{A8B3C4D5-E6F7-4890-ABCD-EF1234567890}"

[Setup]
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
DefaultDirName={autopf}\IDG
DefaultGroupName=IDG
AllowNoIcons=yes
OutputDir=..\dist
OutputBaseFilename=IDG-Setup-{#AppVersion}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DisableProgramGroupPage=yes
CloseApplications=yes
UninstallDisplayIcon={app}\{#AppExeName}
VersionInfoVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}
VersionInfoDescription={#AppName}
VersionInfoProductName={#AppName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "startupicon"; Description: "Start IDG when Windows starts"; GroupDescription: "Startup"; Flags: unchecked

[Files]
Source: "..\target\release\IDG.exe";   DestDir: "{app}"; Flags: ignoreversion
Source: "..\extension\*";              DestDir: "{app}\extension"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\README.md";                DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\IDG";              Filename: "{app}\{#AppExeName}"
Name: "{group}\Uninstall IDG";    Filename: "{uninstallexe}"
Name: "{autodesktop}\IDG";        Filename: "{app}\{#AppExeName}"; Tasks: desktopicon
Name: "{userstartup}\IDG";        Filename: "{app}\{#AppExeName}"; Tasks: startupicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,IDG}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{app}"

[Registry]
Root: HKCU; Subkey: "Software\IDG"; ValueType: string; ValueName: "Version"; ValueData: "{#AppVersion}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\IDG"; ValueType: string; ValueName: "InstallDir"; ValueData: "{app}"; Flags: uninsdeletekey

[Code]
procedure CurStepChanged(CurStep: TSetupStep);
var
  ExtJsonPath: string;
  ExtJson:     string;
  ExePath:     string;
  EscapedPath: string;
begin
  if CurStep = ssPostInstall then
  begin
    ExtJsonPath := ExpandConstant('{userappdata}\IDG\com.rust_idm.host.json');
    ForceDirectories(ExtractFilePath(ExtJsonPath));

    ExePath     := ExpandConstant('{app}\IDG.exe');
    EscapedPath := '';
    begin
      // replace single backslash with double
      StringChangeEx(ExePath, '\', '\\', True);
      EscapedPath := ExePath;
    end;

    ExtJson :=
      '{' + #13#10 +
      '  "name": "com.rust_idm.host",' + #13#10 +
      '  "description": "IDG Internet Download GO native host",' + #13#10 +
      '  "type": "stdio",' + #13#10 +
      '  "path": "' + EscapedPath + '",' + #13#10 +
      '  "allowed_origins": ["chrome-extension://REPLACE_WITH_EXTENSION_ID/"]' + #13#10 +
      '}';
    SaveStringToFile(ExtJsonPath, ExtJson, False);

    RegWriteStringValue(HKCU,
      'Software\Google\Chrome\NativeMessagingHosts\com.rust_idm.host', '',
      ExtJsonPath);
    RegWriteStringValue(HKCU,
      'Software\Microsoft\Edge\NativeMessagingHosts\com.rust_idm.host', '',
      ExtJsonPath);
  end;
end;

