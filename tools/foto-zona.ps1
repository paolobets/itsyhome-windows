# Foto SOLO di una zona dello schermo (mai lo schermo intero), in pixel LOGICI (scalati col DPI).
# Non avvia ne' chiude processi: lancia l'app a parte, poi esegui questo script.
# Uso: .\tools\foto-zona.ps1 -X 900 -Y 60 -L 560 -A 440 -Out foto.png
param(
  [Parameter(Mandatory)][int]$X,
  [Parameter(Mandatory)][int]$Y,
  [Parameter(Mandatory)][int]$L,
  [Parameter(Mandatory)][int]$A,
  [Parameter(Mandatory)][string]$Out
)
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public class D { [DllImport("user32.dll")] public static extern int GetDpiForSystem(); [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }'
[D]::SetProcessDPIAware() | Out-Null
$scala = [D]::GetDpiForSystem() / 96.0
$px = [int]($X * $scala); $py = [int]($Y * $scala)
$w = [int]($L * $scala); $h = [int]($A * $scala)
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($px, $py, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
"scala $scala, zona ${w}x${h} a ($px,$py) -> $Out"