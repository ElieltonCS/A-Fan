$ErrorActionPreference='Stop'
$principal=[Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    $process=Start-Process -FilePath "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -Verb RunAs -WindowStyle Hidden -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',('"'+$PSCommandPath+'"')) -PassThru
    $process.WaitForExit();exit $process.ExitCode
}
$destination=[IO.Path]::GetFullPath((Join-Path $env:ProgramFiles 'A-Fan'))
$installedExe=Join-Path $destination 'a-fan.exe'
$task=Get-ScheduledTask -TaskName 'A-Fan' -ErrorAction SilentlyContinue
if($task -and ($task.Actions.Count -ne 1 -or $task.Actions[0].Execute -ne $installedExe)){throw 'A tarefa A-Fan aponta para outro aplicativo.'}
$running=Get-Process -Name 'a-fan' -ErrorAction SilentlyContinue|Where-Object{$_.Path -eq $installedExe}
if($running){Start-Process -FilePath $installedExe -ArgumentList '--exit' -WindowStyle Hidden -Wait
    foreach($process in $running){if(!$process.WaitForExit(10000)){throw 'Saída segura não confirmada. Desinstalação interrompida.'}}}
if($task){Unregister-ScheduledTask -TaskName 'A-Fan' -Confirm:$false}
foreach($folder in @([Environment]::GetFolderPath('Desktop'),[Environment]::GetFolderPath('Programs'))) {
    $shortcut=Join-Path $folder 'A-Fan.lnk';if(Test-Path -LiteralPath $shortcut){Remove-Item -LiteralPath $shortcut}
}
if(Test-Path -LiteralPath 'HKCU:\Software\A-Fan'){Remove-ItemProperty -LiteralPath 'HKCU:\Software\A-Fan' -Name AutoStart -ErrorAction SilentlyContinue}
# Only remove the four known application files; preserve user settings and diagnostics.
if($destination -ne [IO.Path]::GetFullPath((Join-Path $env:ProgramFiles 'A-Fan'))){throw 'Destino inválido.'}
foreach($name in @('a-fan.exe','set-startup.ps1','uninstall.ps1')) {
    $file=Join-Path $destination $name;if(Test-Path -LiteralPath $file){Remove-Item -LiteralPath $file}
}
