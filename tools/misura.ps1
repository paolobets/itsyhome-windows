# Misura RAM/CPU di un eseguibile e di tutto il suo albero di processi, secondo per secondo.
# Ricorda ogni PID mai visto nell'albero, cosi' vede anche i figli (es. WebView2) rimasti orfani.
# Chiude SOLO i processi che ha visto nascere dal proprio avvio (per PID), mai per nome.
# Uso: .\tools\misura.ps1 -Exe <percorso.exe> [-Argomenti 'a','b'] -Log <file> [-Secondi 50]
# Il renderer e' scelto dall'app stessa (nessuna variabile d'ambiente impostata qui).
param(
  [Parameter(Mandatory)][string]$Exe,
  [string[]]$Argomenti = @(),
  [Parameter(Mandatory)][string]$Log,
  [int]$Secondi = 50
)
if (Test-Path $Log) { Remove-Item $Log }
$cores = [Environment]::ProcessorCount
$avvio = @{ FilePath = $Exe; PassThru = $true }
if ($Argomenti.Count -gt 0) { $avvio.ArgumentList = $Argomenti }
$root = Start-Process @avvio
$visti = @{}   # pid -> nome
$visti[$root.Id] = $root.ProcessName
$t0 = Get-Date; $cpuPrima = 0.0; $tPrima = $t0
$righe = @()
for ($s = 1; $s -le $Secondi; $s++) {
  Start-Sleep -Milliseconds 1000
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
  $cambiato = $true
  while ($cambiato) { $cambiato = $false
    foreach ($c in $all) { if ($visti.ContainsKey([int]$c.ParentProcessId) -and -not $visti.ContainsKey([int]$c.ProcessId)) { $visti[[int]$c.ProcessId] = $c.Name; $cambiato = $true } } }
  $cpu = 0.0; $priv = 0; $n = 0; $nomi = @{}
  foreach ($id in @($visti.Keys)) {
    try { $p = Get-Process -Id $id -ErrorAction Stop
      if (($p.ProcessName + '.exe') -ne $visti[$id] -and $p.ProcessName -ne $visti[$id]) { continue }
      $cpu += $p.TotalProcessorTime.TotalMilliseconds; $priv += $p.PrivateMemorySize64; $n++; $nomi[$p.ProcessName] = 1 } catch {}
  }
  $ora = Get-Date
  $pct = if ($s -gt 1) { [math]::Round(($cpu - $cpuPrima) / ($ora - $tPrima).TotalMilliseconds / $cores * 100, 2) } else { 0 }
  $cpuPrima = $cpu; $tPrima = $ora
  $righe += [pscustomobject]@{ s = [int]($ora - $t0).TotalSeconds; Processi = $n; 'RAM MB' = [math]::Round($priv/1MB,1); 'CPU %' = $pct; Chi = (($nomi.Keys | Sort-Object) -join ',') }
}
$righe | Format-Table -AutoSize | Out-String -Width 200
"--- ancora vivi a fine prova (oltre al processo avviato):"
foreach ($id in @($visti.Keys)) { if ($id -ne $root.Id) { try { $p = Get-Process -Id $id -ErrorAction Stop; if (($p.ProcessName + '.exe') -eq $visti[$id]) { "  $id $($p.ProcessName)" } } catch {} } }
"--- log:"; if (Test-Path $Log) { Get-Content $Log }
foreach ($id in @($visti.Keys)) { if ($id -ne $root.Id) { try { $p = Get-Process -Id $id -ErrorAction Stop; if (($p.ProcessName + '.exe') -eq $visti[$id]) { Stop-Process -Id $id -Force } } catch {} } }
Stop-Process -Id $root.Id -Force -ErrorAction SilentlyContinue