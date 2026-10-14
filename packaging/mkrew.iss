; Windows setup .exe (SPEC §4), built by scripts/package-setup.ps1 with Inno Setup 6 (ISCC).
; The first page asks "Install for me only" (no admin: %LOCALAPPDATA%\Programs\Mkrew, user PATH)
; or "Install for all users" (UAC: Program Files\Mkrew, system PATH). Silent: /CURRENTUSER or
; /ALLUSERS with /VERYSILENT. Defines: MyVersion, BinDir, AgentsDir, OutDir.
; Keep AppId fixed forever: upgrades replace the previous install of the same scope.

#ifndef MyVersion
  #error Pass /DMyVersion=<x.y.z>
#endif

[Setup]
AppId={{9C3E5A71-4B2D-4F86-A0E9-7D15C2B84F36}
AppName=Mkrew
AppVersion={#MyVersion}
AppVerName=Mkrew {#MyVersion}
AppPublisher=Mkrew
DefaultDirName={autopf}\Mkrew
DefaultGroupName=Mkrew
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesEnvironment=yes
CloseApplications=yes
UninstallDisplayIcon={app}\bin\mkrew.exe
UninstallDisplayName=Mkrew
OutputDir={#OutDir}
OutputBaseFilename=mkrew-{#MyVersion}-x64-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\crates\mkrew-ui\resources\windows\mkrew.ico
; Off: the guard is inherited by the post-install "Launch Mkrew", so the app, daemon and every
; agent it starts could not traverse user-created junctions (omp plugins in ~/.omp/plugins/
; node_modules are junctions: EUNKNOWN / "untrusted mount point"). Setup writes only {app} and the
; PATH value, never a publicly writable folder, so the guard protects nothing here.
RedirectionGuard=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#BinDir}\mkrew.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#BinDir}\mk.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#BinDir}\mkrew-daemon.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
; Reference copies: the daemon seeds the config dir from profiles compiled into it.
Source: "{#AgentsDir}\*.toml"; DestDir: "{app}\agents"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Mkrew"; Filename: "{app}\bin\mkrew.exe"; WorkingDir: "{app}\bin"
Name: "{autodesktop}\Mkrew"; Filename: "{app}\bin\mkrew.exe"; WorkingDir: "{app}\bin"; Tasks: desktopicon

[Run]
Filename: "{app}\bin\mkrew.exe"; Description: "{cm:LaunchProgram,Mkrew}"; Flags: nowait postinstall skipifsilent

[Code]
const
  SystemEnvKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
  UserEnvKey = 'Environment';

function EnvRoot: Integer;
begin
  if IsAdminInstallMode then Result := HKEY_LOCAL_MACHINE else Result := HKEY_CURRENT_USER;
end;

function EnvKey: String;
begin
  if IsAdminInstallMode then Result := SystemEnvKey else Result := UserEnvKey;
end;

function BinDir: String;
begin
  Result := ExpandConstant('{app}\bin');
end;

{ ';a;b;' contains ';<dir>;' (case-insensitive, trailing backslash ignored). }
function PathHas(Path, Dir: String): Boolean;
begin
  Result := Pos(';' + Lowercase(RemoveBackslashUnlessRoot(Dir)) + ';', ';' + Lowercase(Path) + ';') > 0;
end;

procedure AddToPath;
var
  Path: String;
begin
  if not RegQueryStringValue(EnvRoot, EnvKey, 'Path', Path) then Path := '';
  if PathHas(Path, BinDir) then Exit;
  if (Path <> '') and (Copy(Path, Length(Path), 1) <> ';') then Path := Path + ';';
  RegWriteExpandStringValue(EnvRoot, EnvKey, 'Path', Path + BinDir);
end;

procedure RemoveFromPath;
var
  Path, Entry, Kept: String;
  P: Integer;
begin
  if not RegQueryStringValue(EnvRoot, EnvKey, 'Path', Path) then Exit;
  Kept := '';
  Path := Path + ';';
  while Path <> '' do
  begin
    P := Pos(';', Path);
    Entry := Copy(Path, 1, P - 1);
    Delete(Path, 1, P);
    if (Entry <> '') and (Lowercase(RemoveBackslashUnlessRoot(Entry)) <> Lowercase(BinDir)) then
    begin
      if Kept <> '' then Kept := Kept + ';';
      Kept := Kept + Entry;
    end;
  end;
  RegWriteExpandStringValue(EnvRoot, EnvKey, 'Path', Kept);
end;

{ Asks the daemon to stop when it runs from this install's bin, so its exe can be replaced or
  removed; another install's daemon is left running (mk prints "left running", exit 0).
  Agent sessions resume on the next start (SPEC §5). An mk from before --only-from exits 2
  (unknown flag): an in-place upgrade still has to free the exe, so it stops the daemon the
  old way. }
procedure StopDaemon;
var
  Mk: String;
  Code: Integer;
begin
  Mk := ExpandConstant('{app}\bin\mk.exe');
  if not FileExists(Mk) then
    Exit;
  Exec(Mk, 'daemon stop --only-from "' + BinDir + '"', '', SW_HIDE, ewWaitUntilTerminated, Code);
  if Code = 2 then
    Exec(Mk, 'daemon stop', '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopDaemon;
  Result := '';
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then AddToPath;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    StopDaemon;
    RemoveFromPath;
  end;
end;
