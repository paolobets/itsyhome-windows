# Misura RAM/CPU di un eseguibile e di tutto il suo albero di processi, secondo per secondo.
# Ricorda ogni PID mai visto nell'albero, cosi' vede anche i figli (es. WebView2) rimasti orfani.
# Chiude SOLO i processi nati dal proprio avvio (per PID, con controllo di identita'), mai per nome.
# Uso: .\tools\misura.ps1 -Exe <percorso.exe> [-Argomenti 'a','b'] [-Log <file>] [-Secondi 50]
# -Log NON viene passato all'app: se l'app deve scrivere un log, il percorso va dato in -Argomenti;
# qui serve solo per svuotarlo prima della prova e stamparlo a fine prova.
# Il renderer e' scelto dall'app stessa (nessuna variabile d'ambiente impostata qui).
param(
  [Parameter(Mandatory)][string]$Exe,
  [string[]]$Argomenti = @(),
  [string]$Log = '',
  [int]$Secondi = 50
)
if ($Log -and (Test-Path $Log)) { Remove-Item $Log }
$cores = [Environment]::ProcessorCount
$avvio = @{ FilePath = $Exe; PassThru = $true }
if ($Argomenti.Count -gt 0) { $avvio.ArgumentList = $Argomenti }
$t0 = Get-Date
$root = Start-Process @avvio
$visti = @{}   # pid -> nome processo
$nasc = @{}    # pid -> data di creazione (identita': un PID riusato ha un'altra data)
$visti[$root.Id] = if ($root.HasExited) { $root.Id.ToString() } else { $root.ProcessName }
try { $nasc[$root.Id] = $root.StartTime } catch { $nasc[$root.Id] = $t0 }
$cpuPrima = 0.0; $tPrima = $t0
$righe = @()
# Vero solo se il PID e' ancora il processo che avevamo visto nascere (stessa data di creazione).
function Mio($id) {
  try { $p = Get-Process -Id $id -ErrorAction Stop
    if ([math]::Abs(($p.StartTime - $nasc[$id]).TotalSeconds) -gt 2) { return $null }
    return $p } catch { return $null }
}
for ($s = 1; $s -le $Secondi; $s++) {
  Start-Sleep -Milliseconds 1000
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name, CreationDate
  $cambiato = $true
  while ($cambiato) { $cambiato = $false
    foreach ($c in $all) {
      $pid_ = [int]$c.ProcessId; $ppid = [int]$c.ParentProcessId
      if (-not $visti.ContainsKey($ppid) -or $visti.ContainsKey($pid_)) { continue }
      # Adotta solo chi e' nato dopo il nostro avvio e non prima del suo genitore (PID padre riusato = estraneo).
      if ($null -eq $c.CreationDate -or $c.CreationDate -lt $t0 -or $c.CreationDate -lt $nasc[$ppid]) { continue }
      $visti[$pid_] = $c.Name; $nasc[$pid_] = $c.CreationDate; $cambiato = $true
    } }
  $cpu = 0.0; $priv = 0; $n = 0; $nomi = @{}
  foreach ($id in @($visti.Keys)) {
    $p = Mio $id; if (-not $p) { continue }
    if (($p.ProcessName + '.exe') -ne $visti[$id] -and $p.ProcessName -ne $visti[$id]) { continue }
    try { $cpu += $p.TotalProcessorTime.TotalMilliseconds; $priv += $p.PrivateMemorySize64; $n++; $nomi[$p.ProcessName] = 1 } catch {}
  }
  $ora = Get-Date
  $pct = if ($s -gt 1) { [math]::Round(($cpu - $cpuPrima) / ($ora - $tPrima).TotalMilliseconds / $cores * 100, 2) } else { 0 }
  $cpuPrima = $cpu; $tPrima = $ora
  $righe += [pscustomobject]@{ s = [int]($ora - $t0).TotalSeconds; Processi = $n; 'RAM MB' = [math]::Round($priv/1MB,1); 'CPU %' = $pct; Chi = (($nomi.Keys | Sort-Object) -join ',') }
}
$righe | Format-Table -AutoSize | Out-String -Width 200
"--- ancora vivi a fine prova (oltre al processo avviato):"
$vivi = @()
foreach ($id in @($visti.Keys)) { if ($id -ne $root.Id) { $p = Mio $id
  if ($p -and (($p.ProcessName + '.exe') -eq $visti[$id])) { "  $id $($p.ProcessName)"; $vivi += $p } } }
if ($Log) { "--- log:"; if (Test-Path $Log) { Get-Content $Log } }
foreach ($p in $vivi) { try { $p.Kill(); "chiuso $($p.Id) $($p.ProcessName)" } catch {} }
if (-not $root.HasExited) { $root.Kill(); "chiuso root $($root.Id)" } else { "root gia' terminato" }