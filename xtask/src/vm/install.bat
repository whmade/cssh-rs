@echo off
rem Provision the Windows test VM of `cargo xtask vm start`: install the
rem OpenSSH server and authorize the xtask's SSH key in
rem C:\OEM\authorized_keys. dockurr copies the OEM folder into the VM and
rem runs this script once, at the first logon, in a window of its own.
rem
rem The key is authorized last: the xtask treats the first SSH login as the
rem end of provisioning. A failed step skips the rest and keeps this window
rem open, so the VM's screen shows what failed.

rem Windows Update serves the OpenSSH server.
call :retry powershell -NoProfile -ExecutionPolicy Bypass -Command "Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0" || goto :failed
rem Defender would scan each copied test binary before it runs, which only
rem slows the tests. The xtask fills in the directory to exclude.
powershell -NoProfile -ExecutionPolicy Bypass -Command "Add-MpPreference -ExclusionPath '@VM_ROOT@'" || echo Continuing without the Defender exclusion.

rem rustdoc links doc tests against the dynamic C runtime, which Windows
rem does not ship.
call :retry curl.exe -sSfL -o C:\OEM\vc_redist.x64.exe https://aka.ms/vs/17/release/vc_redist.x64.exe || goto :failed
C:\OEM\vc_redist.x64.exe /install /quiet /norestart
set result=%errorlevel%
rem 1638: a newer runtime is installed; 3010: a restart completes the install.
rem https://learn.microsoft.com/en-us/windows/win32/msi/error-codes
if %result% neq 0 if %result% neq 1638 if %result% neq 3010 goto :failed
del C:\OEM\vc_redist.x64.exe

powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference = 'Stop'; Set-Service -Name sshd -StartupType Automatic; Start-Service -Name sshd" || goto :failed
copy /y C:\OEM\authorized_keys C:\ProgramData\ssh\administrators_authorized_keys || goto :failed
icacls C:\ProgramData\ssh\administrators_authorized_keys /inheritance:r /grant *S-1-5-32-544:F /grant *S-1-5-18:F || goto :failed
exit /b 0

rem Run the command in the arguments up to three times, a minute apart:
rem downloads can fail while the network settles after the install.
:retry
for /l %%n in (1,1,3) do (
    if %%n gtr 1 powershell -NoProfile -Command "Start-Sleep -Seconds 60"
    %* && exit /b 0
)
exit /b 1

:failed
echo.
echo Provisioning the cssh-rs test VM failed at the step above.
pause
exit /b 1
