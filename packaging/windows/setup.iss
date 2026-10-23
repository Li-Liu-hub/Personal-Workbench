; Build with Inno Setup 6. The payload is exactly the tested application EXE.
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef AppExe
  #error AppExe is required
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist\release"
#endif
[Setup]
AppId={{D1F9422A-73ED-4E48-AC17-A162F50F442C}
AppName=个人办公台
AppVersion={#AppVersion}
AppPublisher=Li-Liu-hub
AppPublisherURL=https://github.com/Li-Liu-hub/Personal-Workbench
AppSupportURL=https://github.com/Li-Liu-hub/Personal-Workbench/issues
DefaultDirName={localappdata}\Programs\PersonalDesk
DefaultGroupName=个人办公台
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=PersonalDesk-{#AppVersion}-Setup-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=no
RestartApplications=no
SetupMutex=PersonalDeskSetup
UninstallDisplayIcon={app}\个人办公台.exe
UninstallDisplayName=个人办公台

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; Flags: unchecked
Name: "autostart"; Description: "登录 Windows 后自动启动"; Flags: unchecked

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; DestName: "个人办公台.exe"; Flags: ignoreversion

[Icons]
Name: "{group}\个人办公台"; Filename: "{app}\个人办公台.exe"
Name: "{autodesktop}\个人办公台"; Filename: "{app}\个人办公台.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "PersonalDesk"; ValueData: """{app}\个人办公台.exe"" --autostart"; Tasks: autostart

[Run]
Filename: "{app}\个人办公台.exe"; Description: "打开个人办公台"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Only the launcher pointer is removed; neither SQLite nor documents are installation files.
Type: files; Name: "{app}\personal-desk-data-path.txt"

[Code]
var
  DataPage: TInputDirWizardPage;

function StripBOM(Value: String): String;
begin
  Result := Value;
  if Copy(Result, 1, 1) = #$FEFF then Delete(Result, 1, 1);
end;

function InitialDataDirectory(): String;
var Pinned: AnsiString; Remembered: String;
begin
  Result := ExpandConstant('{localappdata}\PersonalDesk');
  if RegQueryStringValue(HKCU, 'Software\PersonalDesk', 'DataDirectory', Remembered) then
    if Remembered <> '' then Result := Remembered;
  if LoadStringFromFile(ExpandConstant('{src}\personal-desk-data-path.txt'), Pinned) then
    if StripBOM(Trim(Utf8Decode(Pinned))) <> '' then Result := StripBOM(Trim(Utf8Decode(Pinned)));
  Result := ExpandConstant('{param:DATADIR|' + Result + '}');
end;

procedure InitializeWizard();
begin
  DataPage := CreateInputDirPage(wpSelectDir, '数据目录', '账号、文档、练习和游戏存档的位置',
    '已有办公台数据时选择原数据目录。安装和卸载都不会删除这里的内容。', False, '');
  DataPage.Add('数据目录');
  DataPage.Values[0] := InitialDataDirectory();
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := (PageID = DataPage.ID) and FileExists(ExpandConstant('{app}\personal-desk-data-path.txt'));
end;

procedure CurStepChanged(CurStep: TSetupStep);
var PinPath: String; DataDirectory: String; Pinned: AnsiString;
begin
  if CurStep = ssPostInstall then begin
    PinPath := ExpandConstant('{app}\personal-desk-data-path.txt');
    if FileExists(PinPath) then begin
      if not LoadStringFromFile(PinPath, Pinned) then RaiseException('无法读取原数据目录配置');
      DataDirectory := StripBOM(Trim(Utf8Decode(Pinned)));
    end else begin
      DataDirectory := DataPage.Values[0];
      if not SaveStringToFile(PinPath, Utf8Encode(DataDirectory), False) then RaiseException('无法保存数据目录配置');
    end;
    RegWriteStringValue(HKCU, 'Software\PersonalDesk', 'DataDirectory', DataDirectory);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var Command: String;
begin
  if CurUninstallStep = usUninstall then
    if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'PersonalDesk', Command) then
      if Command = '"' + ExpandConstant('{app}\个人办公台.exe') + '" --autostart' then
        RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'PersonalDesk');
end;
