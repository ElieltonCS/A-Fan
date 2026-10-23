param([string]$SourceExe=(Join-Path $PSScriptRoot 'a-fan.exe'))
$ErrorActionPreference='Stop'
$SourceExe=[IO.Path]::GetFullPath($SourceExe)
if (!(Test-Path -LiteralPath $SourceExe -PathType Leaf)) {throw "Executável não encontrado: $SourceExe"}
$principal=[Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    $process=Start-Process -FilePath "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -Verb RunAs -WindowStyle Hidden -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',('"'+$PSCommandPath+'"'),'-SourceExe',('"'+$SourceExe+'"')) -PassThru
    $process.WaitForExit()
    exit $process.ExitCode
}
$resultPath=Join-Path $env:LOCALAPPDATA 'A-Fan\install-result.json'
$result=[ordered]@{success=$false;timestamp=(Get-Date).ToUniversalTime().ToString('o');error=$null}
try {
    $destination=Join-Path $env:ProgramFiles 'A-Fan'
    $installedExe=Join-Path $destination 'a-fan.exe'
    $existingTask=Get-ScheduledTask -TaskName 'A-Fan' -ErrorAction SilentlyContinue
    if ($existingTask -and ($existingTask.Actions.Count -ne 1 -or $existingTask.Actions[0].Execute -ne $installedExe)) {throw 'Já existe uma tarefa A-Fan com outro destino. Instalação interrompida.'}
    $running=Get-Process -Name 'a-fan' -ErrorAction SilentlyContinue | Where-Object {$_.Path -eq $installedExe}
    if($running) {
        Start-Process -FilePath $installedExe -ArgumentList '--exit' -WindowStyle Hidden -Wait
        foreach($process in $running){if(!$process.WaitForExit(10000)){throw 'O A-Fan não confirmou saída segura. Nenhum processo foi encerrado à força.'}}
    }
    New-Item -Path $destination -ItemType Directory -Force | Out-Null
    if ($SourceExe -ne $installedExe) {Copy-Item -LiteralPath $SourceExe -Destination $installedExe -Force}
    foreach($name in @('set-startup.ps1','uninstall.ps1')) {
        $source=Join-Path $PSScriptRoot $name
        if(!(Test-Path -LiteralPath $source)){throw "Arquivo de instalação ausente: $name"}
        if($source -ne (Join-Path $destination $name)){Copy-Item -LiteralPath $source -Destination (Join-Path $destination $name) -Force}
    }
    $validationFolder=Join-Path $env:LOCALAPPDATA 'A-Fan\validation'
    New-Item -Path $validationFolder -ItemType Directory -Force|Out-Null
    $powerBefore=(& powercfg /getactivescheme|Out-String).Trim()
    $diagnosticPath=Join-Path $validationFolder 'native-diagnostic.json'
    $diagnosticProcess=Start-Process -FilePath $installedExe -ArgumentList @('--diagnose',('"'+$diagnosticPath+'"')) -WindowStyle Hidden -PassThru -Wait
    if($diagnosticProcess.ExitCode -ne 0){throw 'O diagnóstico nativo falhou.'}
    $diagnostic=Get-Content -LiteralPath $diagnosticPath -Raw|ConvertFrom-Json
    if($diagnostic.error -or $diagnostic.telemetry.Err -or $diagnostic.telemetry.Ok.temperature -le 0){throw 'O app nativo não confirmou leitura válida de temperatura.'}
    $smokePath=Join-Path $validationFolder 'ui-smoke.json'
    $smokeProcess=Start-Process -FilePath $installedExe -ArgumentList @('--smoke-test',('"'+$smokePath+'"')) -WindowStyle Hidden -PassThru -Wait
    if($smokeProcess.ExitCode -ne 0){throw 'O teste integrado da interface falhou.'}
    $smoke=Get-Content -LiteralPath $smokePath -Raw|ConvertFrom-Json
    if(!$smoke.starts_hidden -or !$smoke.minimize_hides -or !$smoke.close_hides -or !$smoke.tray_added -or !$smoke.editor_validated -or $smoke.hardware_writes -ne 0){throw 'A interface não passou na verificação integrada.'}
    $powerAfter=(& powercfg /getactivescheme|Out-String).Trim()
    if($powerBefore -ne $powerAfter){throw 'O plano de energia mudou durante a validação. Revise o diagnóstico.'}
    $result.native_diagnostic=$diagnosticPath;$result.ui_smoke=$smokePath;$result.power_scheme_unchanged=$true
    $account=[Security.Principal.WindowsIdentity]::GetCurrent().Name
    $action=New-ScheduledTaskAction -Execute $installedExe -WorkingDirectory $destination
    $trigger=New-ScheduledTaskTrigger -AtLogOn -User $account
    $taskPrincipal=New-ScheduledTaskPrincipal -UserId $account -LogonType Interactive -RunLevel Highest
    $taskSettings=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName 'A-Fan' -Action $action -Trigger $trigger -Principal $taskPrincipal -Settings $taskSettings -Description 'Monitoramento e controle protegido de ventoinha. Inicia oculto na bandeja.' -Force | Out-Null
    New-Item -Path 'HKCU:\Software\A-Fan' -Force | Out-Null
    New-ItemProperty -Path 'HKCU:\Software\A-Fan' -Name AutoStart -Value 1 -PropertyType DWord -Force | Out-Null
    $shell=New-Object -ComObject WScript.Shell
    foreach($folder in @([Environment]::GetFolderPath('Desktop'),[Environment]::GetFolderPath('Programs'))) {
        $shortcut=$shell.CreateShortcut((Join-Path $folder 'A-Fan.lnk'))
        $shortcut.TargetPath=$installedExe;$shortcut.WorkingDirectory=$destination;$shortcut.IconLocation="$installedExe,0";$shortcut.Description='A-Fan — abrir na bandeja';$shortcut.Save()
    }
    $installedHash=(Get-FileHash -LiteralPath $installedExe -Algorithm SHA256).Hash
    if($installedHash -ne (Get-FileHash -LiteralPath $SourceExe -Algorithm SHA256).Hash){throw 'A cópia instalada não corresponde ao executável validado.'}
    Start-ScheduledTask -TaskName 'A-Fan'
    $result.success=$true;$result.executable=$installedExe;$result.sha256=$installedHash
    $result.task=(Get-ScheduledTask -TaskName 'A-Fan').State.ToString()
} catch {$result.error=$_.Exception.Message} finally {
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($resultPath))|Out-Null
    [IO.File]::WriteAllText($resultPath,($result|ConvertTo-Json -Depth 4),(New-Object Text.UTF8Encoding($false)))
}
if(!$result.success){throw $result.error}
