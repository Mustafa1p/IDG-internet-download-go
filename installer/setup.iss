; IDG – Internet Download GO  –  Inno Setup 6 script
; Build: ISCC installer\setup.iss
; Produces: IDG-Setup-1.4.5.exe

#define AppName      "IDG – Internet Download GO"
#define AppVersion   "1.4.5"
#define AppPublisher "IDG Project"
#define AppURL       "https://github.com/Mustafa1p/IDG-internet-download-go"
#define AppExeName   "IDG.exe"
#define AppId        "{{A8B3C4D5-E6F7-4890-ABCD-EF1234567890}"

#ifndef BuildDir
#define BuildDir "..\target\release"
#endif
#ifndef PackageArch
#define PackageArch "x64"
#endif
#ifndef PackageSuffix
#define PackageSuffix ""
#endif

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
OutputBaseFilename=IDG-Setup-v{#AppVersion}{#PackageSuffix}
Compression=lzma2/normal
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\assets\idg.ico
#if PackageArch == "x64"
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#else
ArchitecturesAllowed=x86compatible
#endif
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DisableProgramGroupPage=yes
CloseApplications=force
RestartApplications=no
UninstallDisplayIcon={app}\{#AppExeName}
VersionInfoVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}
VersionInfoDescription={#AppName}
VersionInfoProductName={#AppName}
#ifdef IDG_SIGN_RELEASE
; The compiler invokes this tool for Setup, its temporary copies, and uninstall.
SignTool=idg
SignedUninstaller=yes
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "startupicon"; Description: "Start IDG when Windows starts"; GroupDescription: "Startup"

[Files]
Source: "{#BuildDir}\IDG-native.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BuildDir}\IDG.exe";   DestDir: "{app}"; Flags: ignoreversion
Source: "..\extension\*";              DestDir: "{app}\extension"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#BuildDir}\tools\*"; DestDir: "{app}\tools"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\README.md";                DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\*";                  DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\IDG";              Filename: "{app}\{#AppExeName}"; AppUserModelID: "IDG.InternetDownloadGO"
Name: "{group}\Uninstall IDG";    Filename: "{uninstallexe}"
Name: "{autodesktop}\IDG";        Filename: "{app}\{#AppExeName}"; Tasks: desktopicon; AppUserModelID: "IDG.InternetDownloadGO"


[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,IDG}"; Flags: nowait postinstall skipifsilent
Filename: "{app}\{#AppExeName}"; Flags: nowait skipifnotsilent; Check: IsIDGUpdate

[UninstallDelete]
Type: files; Name: "{userappdata}\IDG\com.rust_idm.host.json"
Type: dirifempty; Name: "{userappdata}\IDG"

[InstallDelete]
Type: files; Name: "{userstartup}\IDG.lnk"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "IDG"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "IDG"; ValueData: """{app}\{#AppExeName}"" --background"; Tasks: startupicon; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "IDG"; Flags: deletevalue; Check: not WizardIsTaskSelected('startupicon')
Root: HKCU; Subkey: "Software\IDG"; ValueType: dword; ValueName: "StartupConfigured"; ValueData: "1"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\IDG"; ValueType: string; ValueName: "Version"; ValueData: "{#AppVersion}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\IDG"; ValueType: string; ValueName: "InstallDir"; ValueData: "{app}"; Flags: uninsdeletekey

Root: HKCU; Subkey: "Software\Google\Chrome\NativeMessagingHosts\com.rust_idm.host"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Edge\NativeMessagingHosts\com.rust_idm.host"; Flags: uninsdeletekey

[Code]
function IsIDGUpdate: Boolean;
begin
  Result := ExpandConstant('{param:IDGUPDATE|0}') = '1';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ExtJsonPath: string;
  ExtJson:     string;
  ExtJsonLines: TArrayOfString;
  ExePath:     string;
  EscapedPath: string;
begin
  if CurStep = ssPostInstall then
  begin
    ExtJsonPath := ExpandConstant('{userappdata}\IDG\com.rust_idm.host.json');
    ForceDirectories(ExtractFilePath(ExtJsonPath));

    ExePath     := ExpandConstant('{app}\IDG-native.exe');
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
      '  "allowed_origins": ["chrome-extension://blmjnmockbofpnipcniflmalfmiaolcj/"]' + #13#10 +
      '}';
    SetArrayLength(ExtJsonLines, 1);
    ExtJsonLines[0] := ExtJson;
    if not SaveStringsToUTF8FileWithoutBOM(ExtJsonPath, ExtJsonLines, False) then
      RaiseException('Could not write IDG browser integration manifest.');

    RegWriteStringValue(HKCU,
      'Software\Google\Chrome\NativeMessagingHosts\com.rust_idm.host', '',
      ExtJsonPath);
    RegWriteStringValue(HKCU,
      'Software\Microsoft\Edge\NativeMessagingHosts\com.rust_idm.host', '',
      ExtJsonPath);
  end;
end;

