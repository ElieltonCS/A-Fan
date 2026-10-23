param([string]$OutputPath = (Join-Path $PSScriptRoot '..\artifacts\diagnostics\hardware.json'))
$ErrorActionPreference = 'Stop'
$result = [ordered]@{
    timestamp = (Get-Date).ToUniversalTime().ToString('o')
    elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    model = (Get-CimInstance Win32_ComputerSystem).Model
    bios = (Get-CimInstance Win32_BIOS).SMBIOSBIOSVersion
    cpu = (Get-CimInstance Win32_Processor).Name.Trim()
    power_scheme = (& powercfg /getactivescheme | Out-String).Trim()
    reads = @()
}
foreach ($className in @('AcerGamingFunction','APGeAction','AcerGenericMethod')) {
    try {
        $instance = Get-CimInstance -Namespace root/WMI -ClassName $className | Select-Object -First 1
        if ($null -eq $instance) { throw 'Nenhuma instancia WMI encontrada.' }
        $queries = if ($className -eq 'AcerGamingFunction') {
            @(@('GetGamingSysInfo',0),@('GetGamingSysInfo',0x101),@('GetGamingSysInfo',0x201),@('GetGamingFanBehavior',1),@('GetGamingFanSpeed',1),@('GetGamingMiscSetting',0xB))
        } else { @(@('GetFunction',0x107),@('GetFunction',0x307)) }
        foreach ($query in $queries) {
            $method = $query[0]
            $inputValue = [uint32]$query[1]
            try {
                $inputName = if ($className -eq 'AcerGamingFunction') {'gmInput'} else {'uiInput'}
                $outputName = if ($className -eq 'AcerGamingFunction') {'gmOutput'} else {'uiOutput'}
                $response = Invoke-CimMethod -InputObject $instance -MethodName $method -Arguments @{$inputName=$inputValue}
                $value = [uint64]$response.$outputName
                $result.reads += [ordered]@{class=$className;method=$method;input=('0x{0:X}' -f $inputValue);output=('0x{0:X16}' -f $value);status=($value -band 255)}
            } catch {
                $result.reads += [ordered]@{class=$className;method=$method;input=('0x{0:X}' -f $inputValue);error=$_.Exception.Message}
            }
        }
    } catch { $result.reads += [ordered]@{class=$className;error=$_.Exception.Message} }
}
$OutputPath = [IO.Path]::GetFullPath($OutputPath)
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($OutputPath)) | Out-Null
[IO.File]::WriteAllText($OutputPath,($result | ConvertTo-Json -Depth 8),(New-Object Text.UTF8Encoding($false)))
