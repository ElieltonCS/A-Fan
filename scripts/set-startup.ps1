param([Parameter(Mandatory=$true)][ValidateSet(0,1)][int]$Enabled)
$ErrorActionPreference='Stop'
$task=Get-ScheduledTask -TaskName 'A-Fan' -ErrorAction Stop
$expected=Join-Path $env:ProgramFiles 'A-Fan\a-fan.exe'
if ($task.Actions.Count -ne 1 -or $task.Actions[0].Execute -ne $expected) {throw 'A tarefa A-Fan possui uma ação inesperada.'}
foreach($trigger in $task.Triggers){$trigger.Enabled=($Enabled -eq 1)}
Set-ScheduledTask -TaskName 'A-Fan' -Trigger $task.Triggers | Out-Null
$confirmed=Get-ScheduledTask -TaskName 'A-Fan'
if (@($confirmed.Triggers | Where-Object {$_.Enabled}).Count -ne $Enabled) {throw 'O Windows não confirmou o estado da inicialização.'}
New-Item -Path 'HKCU:\Software\A-Fan' -Force | Out-Null
New-ItemProperty -Path 'HKCU:\Software\A-Fan' -Name AutoStart -Value $Enabled -PropertyType DWord -Force | Out-Null
