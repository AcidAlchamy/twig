# Opens Twig's settings with a throwaway config, previewing a made-up project behind a spare drive letter
# (so no real path shows), captures only that window, then removes everything again.
param([string]$Exe, [string]$OutDir, [string]$Drive = 'Q:')
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class S {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  public static IntPtr Win(string cls) { return FindWindow(cls, null); }
  public static IntPtr Titled(string t) { return FindWindow(null, t); }
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, int d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
  public static RECT Bounds(IntPtr h) { RECT r; DwmGetWindowAttribute(h, 9, out r, 16); return r; }
}
"@
[S]::SetProcessDPIAware() | Out-Null
$tmp = Join-Path $env:TEMP 'twig-settings-shot'
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
$app = "$tmp\demo\my-app"
foreach ($d in 'src\ui', 'tests', 'docs', 'build\out', 'node_modules\left-pad', '.github\workflows') { New-Item -ItemType Directory -Force "$app\$d" | Out-Null }
foreach ($f in 'src\main.rs', 'src\config.rs', 'src\tree.rs', 'src\ui\menu.rs', 'src\ui\theme.rs', 'tests\smoke.rs', 'docs\guide.md',
    'build\out\app.exe', 'node_modules\left-pad\index.js', '.github\workflows\ci.yml', '.env', 'Cargo.toml', 'LICENSE', 'README.md') {
    [System.IO.File]::WriteAllText("$app\$f", 'x')
}
[System.IO.File]::WriteAllText("$app\.gitignore", "build/`n.env`n")
subst $Drive "$tmp\demo" | Out-Null

New-Item -ItemType Directory -Force "$tmp\appdata\Twig" | Out-Null
$env:APPDATA = "$tmp\appdata"
"hello_card = false" | Set-Content "$tmp\appdata\Twig\settings.toml" -Encoding UTF8
[System.IO.File]::WriteAllText("$tmp\appdata\Twig\last.txt", "$Drive\my-app")

# The right-click entry shows as on while the window is open; taken out again below.
$menu = 'HKCU:\Software\Classes\Directory\shell\Twig'
New-Item -Path "$menu\command" -Force | Out-Null
Set-Item -Path "$menu\command" -Value "`"$Exe`" --copy `"%V`""

$cur = New-Object S+POINT; [S]::GetCursorPos([ref]$cur) | Out-Null
$res = Start-Process $Exe -ArgumentList '--autostart', '--no-hello' -PassThru
Start-Sleep -Milliseconds 900
$set = Start-Process $Exe -ArgumentList '--settings' -PassThru
$h = [IntPtr]::Zero
foreach ($i in 1..40) { Start-Sleep -Milliseconds 100; $h = [S]::Titled('Twig Settings'); if ($h -ne [IntPtr]::Zero) { break } }
Start-Sleep -Milliseconds 1800
function Grab($name) {
    [S]::keybd_event(0x10, 0, 0, [UIntPtr]::Zero); [S]::keybd_event(0x10, 0, 2, [UIntPtr]::Zero)
    [S]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 350
    $r = [S]::Bounds($h)
    $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
    $bmp.Save((Join-Path $OutDir $name))
    $g.Dispose(); $bmp.Dispose()
    "$name $($r.R - $r.L)x$($r.B - $r.T)"
}
if ($h -ne [IntPtr]::Zero) {
    Grab 'settings-top.png'
    $r = [S]::Bounds($h)
    [S]::SetCursorPos(($r.L + $r.R) / 2, ($r.T + $r.B) / 2) | Out-Null
    foreach ($i in 1..12) { [S]::mouse_event(0x0800, 0, 0, -120, [UIntPtr]::Zero); Start-Sleep -Milliseconds 40 }
    Start-Sleep -Milliseconds 500
    Grab 'settings-scrolled.png'
    [S]::SetCursorPos($cur.X, $cur.Y) | Out-Null
    [S]::PostMessage($h, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
} else { 'settings window not found' }
Start-Sleep -Milliseconds 400
[S]::PostMessage([S]::Win('Twig.Resident'), 0x8004, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 600
Get-Process -Id $res.Id, $set.Id -ErrorAction SilentlyContinue | Stop-Process -Force
Remove-Item $menu -Recurse -Force -ErrorAction SilentlyContinue
subst $Drive /D | Out-Null
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
'closed'
