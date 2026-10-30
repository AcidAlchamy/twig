# Twig live test, run inside the logged-on desktop session by a scheduled task.
# Drives Twig with synthetic keys and clicks on a made-up project and checks what lands on the clipboard.
$ErrorActionPreference = 'Continue'
$T = 'C:\twig-test'
$log = "$T\log.txt"
$app = "$T\demo\my-app"
Remove-Item $log, "$T\shot-*.png", "$T\clip-*.txt", "$T\DONE" -ErrorAction SilentlyContinue
function Log($m) { "$(Get-Date -Format HH:mm:ss.fff) $m" | Add-Content $log -Encoding UTF8 }

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class U {
  [StructLayout(LayoutKind.Sequential)] public struct LASTINPUTINFO { public uint cbSize; public uint dwTime; }
  [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref LASTINPUTINFO p);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool r);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern uint GetDpiForSystem();
  [DllImport("kernel32.dll")] public static extern uint GetTickCount();
  [DllImport("kernel32.dll")] public static extern bool QueryProcessCycleTime(IntPtr h, out ulong c);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public static RECT Bounds(IntPtr h) { RECT r; GetWindowRect(h, out r); return r; }
  public static uint Idle() { var i = new LASTINPUTINFO(); i.cbSize = 8; GetLastInputInfo(ref i); return GetTickCount() - i.dwTime; }
  public static void Key(byte vk, bool up) { keybd_event(vk, 0, up ? 2u : 0u, UIntPtr.Zero); }
  public static IntPtr Win(string cls) { return FindWindow(cls, null); }
  public static IntPtr Win(string cls, string title) { return FindWindow(cls, title); }
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
  public static IntPtr Starting(string cls, string prefix) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      var c = new System.Text.StringBuilder(256); GetClassName(h, c, 256);
      var t = new System.Text.StringBuilder(256); GetWindowText(h, t, 256);
      if (c.ToString() == cls && t.ToString().StartsWith(prefix)) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@
[U]::SetProcessDPIAware() | Out-Null

$idle = [U]::Idle()
Log "idle ms before start: $idle"
if ($idle -lt 120000) { Log 'Someone used this PC in the last 2 minutes: not testing.'; 'ABORT' | Set-Content "$T\DONE"; exit }

function Combo([byte[]]$keys) {
    foreach ($k in $keys) { [U]::Key($k, $false); Start-Sleep -Milliseconds 25 }
    [array]::Reverse($keys)
    foreach ($k in $keys) { [U]::Key($k, $true); Start-Sleep -Milliseconds 25 }
}
function Hotkey { Combo @(0x11, 0x12, 0x54) }   # Ctrl+Alt+T
function Shot($name) {
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
    $bmp.Save("$T\shot-$name.png")
    $g.Dispose(); $bmp.Dispose()
}
function Click($x, $y) { [U]::SetCursorPos($x, $y) | Out-Null; Start-Sleep -Milliseconds 120; [U]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 60; [U]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero) }
function Mark { [System.Windows.Forms.Clipboard]::SetText('(nothing yet)') }
# Saves the clipboard to clip-<name>.txt and logs its first line and size.
function Clip($name) {
    $text = ''
    foreach ($i in 1..5) { try { $text = [System.Windows.Forms.Clipboard]::GetText(); break } catch { Start-Sleep -Milliseconds 100 } }
    [System.IO.File]::WriteAllText("$T\clip-$name.txt", $text, (New-Object System.Text.UTF8Encoding $false))
    $lines = ($text -split "`r`n")
    Log ("CLIP {0}: first line '{1}' | {2} lines | CRLF {3} | same as expected-{0}: {4}" -f $name, $lines[0], $lines.Count, ($text.Contains("`r`n")),
        $(if (Test-Path "$T\expected-$name.txt") { $text -eq ([System.IO.File]::ReadAllText("$T\expected-$name.txt")) } else { 'n/a' }))
}
# Windows lets a process take the foreground right after an Alt tap. Only when needed: in Explorer itself
# Alt shows the access keys and would eat the next keys.
function Front($h) {
    foreach ($i in 1..5) {
        if ([U]::GetForegroundWindow() -eq $h) { return }
        [U]::Key(0x12, $false); [U]::Key(0x12, $true); [U]::SetForegroundWindow($h) | Out-Null; Start-Sleep -Milliseconds 300
    }
}
# Typed keys only ever go to the test's own Explorer window: anything else in front (a system prompt, say) ends the test.
function InExplorer {
    if ([U]::GetForegroundWindow() -ne $ex) {
        Fg 'instead of Explorer, so no keys were sent'
        Log 'STOPPED: something else is in front'; Finish 'STOPPED'
    }
}
function Keys($keys) { InExplorer; [System.Windows.Forms.SendKeys]::SendWait($keys) }
# Names the window in front: a system prompt there (Windows Security leaves itself out of screenshots) eats the test's keys.
function Fg($when) {
    $f = [U]::GetForegroundWindow()
    $c = New-Object System.Text.StringBuilder 256; [U]::GetClassName($f, $c, 256) | Out-Null
    $t2 = New-Object System.Text.StringBuilder 256; [U]::GetWindowText($f, $t2, 256) | Out-Null
    Log "FRONT ${when}: $c '$t2'"
}
function Away { [U]::SetCursorPos(40, 300) | Out-Null }
function Finish($state) {
    # 9. Uninstall: out of the menu and start-up, settings gone, resident quit.
    $u = Start-Process "$T\Twig.exe" -ArgumentList '--uninstall' -PassThru
    Start-Sleep -Milliseconds 2000
    Shot '07-uninstall'
    $box = [U]::Win('#32770', 'Twig'); if ($box -ne [IntPtr]::Zero) { [U]::PostMessage($box, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
    $u.WaitForExit(5000) | Out-Null
    Log "UNINSTALL: menu left '$(Reg 'Directory')' | settings folder left: $(Test-Path $cfgDir) | twig running: $([bool](Get-Process twig -ErrorAction SilentlyContinue))"

    foreach ($title in 'my-app', 'other', 'src', 'Home') {
        $w = [U]::Starting('CabinetWClass', $title)
        if ($w -ne [IntPtr]::Zero) { [U]::PostMessage($w, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
    }
    Start-Sleep -Milliseconds 500
    $shell.UndoMinimizeALL()
    Get-Process twig -ErrorAction SilentlyContinue | Stop-Process -Force
    Log 'done'
    $state | Set-Content "$T\DONE"
    exit
}
function Reg($place) { (Get-ItemProperty "HKCU:\Software\Classes\$place\shell\Twig\command" -ErrorAction SilentlyContinue).'(default)' }
function Explorer($path, $prefix) {
    Start-Process explorer.exe $path
    $h = [IntPtr]::Zero
    foreach ($i in 1..40) { Start-Sleep -Milliseconds 200; $h = [U]::Starting('CabinetWClass', $prefix); if ($h -ne [IntPtr]::Zero) { break } }
    Front $h
    Start-Sleep -Milliseconds 500
    $h
}

# --- the made-up project -----------------------------------------------------------
$shell = New-Object -ComObject Shell.Application
$shell.MinimizeAll(); Start-Sleep -Milliseconds 800
Get-Process twig -ErrorAction SilentlyContinue | Stop-Process -Force
# Leftovers from earlier test runs (Nab's test folder, this test's own windows).
foreach ($title in 'target', 'my-app', 'other', 'src', 'out') {
    $w = [U]::Starting('CabinetWClass', $title)
    if ($w -ne [IntPtr]::Zero) { [U]::PostMessage($w, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
}
Away
Remove-Item "$T\demo" -Recurse -Force -ErrorAction SilentlyContinue
foreach ($d in 'src\ui', 'tests', 'build\out', 'node_modules\left-pad', '.github\workflows') { New-Item -ItemType Directory -Force "$app\$d" | Out-Null }
$files = @{ 'src\main.rs' = 'fn main() {}'; 'src\ui\menu.rs' = ''; 'src\file2.rs' = ''; 'src\file10.rs' = ''; 'tests\smoke.rs' = '';
    'build\out\app.exe' = 'xxxxxxxx'; 'node_modules\left-pad\index.js' = ''; '.github\workflows\ci.yml' = ''; '.env' = 'SECRET=1';
    '.gitignore' = "build/`n.env`n"; 'README.md' = '# hi'; 'Cargo.toml' = '' }
foreach ($f in $files.Keys) { [System.IO.File]::WriteAllText("$app\$f", $files[$f]) }
New-Item -ItemType Directory -Force "$T\demo\other\docs" | Out-Null
[System.IO.File]::WriteAllText("$T\demo\other\docs\guide.md", '')
$cfgDir = "$env:APPDATA\Twig"
Remove-Item $cfgDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $cfgDir | Out-Null
"hello_card = false" | Set-Content "$cfgDir\settings.toml" -Encoding UTF8

# 1. The right-click menu's command with no resident: copies, shows its card, exits by itself.
Mark
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process "$T\Twig.exe" -ArgumentList '--copy', "`"$app`"" -PassThru
Start-Sleep -Milliseconds 900
Shot '01-once-card'
Clip 'my-app'
$exited = $p.WaitForExit(12000)
Log "ONCE: exited by itself: $exited after $([int]$sw.Elapsed.TotalSeconds) s"

# 2. First run of the resident: the right-click entry goes in.
Remove-Item "$cfgDir\settings.toml" -Force
$r = Start-Process "$T\Twig.exe" -PassThru
Start-Sleep -Milliseconds 3000
Log "FIRST RUN: resident window $([U]::Win('Twig.Resident')) | menu: folders '$(Reg 'Directory')' | inside '$(Reg 'Directory\Background')' | drives '$(Reg 'Drive')'"
Shot '02-first-run'
# The settings window needs OpenGL, which this laptop doesn't have: close what it opened.
Get-CimInstance Win32_Process -Filter "Name='twig.exe'" | Where-Object { $_.CommandLine -like '*--settings*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
"hello_card = false" | Set-Content "$cfgDir\settings.toml" -Encoding UTF8
[U]::PostMessage([U]::Win('Twig.Resident'), 0x8001, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null   # reload

# 3. --copy hands the folder to the running resident and returns at once.
Mark
$sw = [Diagnostics.Stopwatch]::StartNew()
$h = Start-Process "$T\Twig.exe" -ArgumentList '--copy', "`"$app\src`"" -PassThru
$h.WaitForExit(5000) | Out-Null
Log "HAND-OFF: --copy returned after $($sw.ElapsedMilliseconds) ms | twig processes now: $(@(Get-Process twig).Count)"
Start-Sleep -Milliseconds 700
Clip 'src'

# 4. A drive root, as Explorer passes it ("T:\" arrives as T:").
subst T: "$T\demo" | Out-Null
Mark
$psi = New-Object Diagnostics.ProcessStartInfo "$T\Twig.exe", '--copy "T:\"'
[Diagnostics.Process]::Start($psi).WaitForExit(5000) | Out-Null
Start-Sleep -Milliseconds 900
Clip 'drive'
subst T: /D | Out-Null

# 5. The hotkey in File Explorer: the open folder, then a selected folder.
$ex = Explorer $app 'my-app'
Fg 'before the hotkey'
InExplorer
Mark; Hotkey; Start-Sleep -Milliseconds 1200
Shot '03-hotkey-card'
Clip 'hotkey'
# 5b. The card's Markdown button (the first one when the style is Tree).
$s = [U]::GetDpiForSystem() / 96.0
$wa = [System.Windows.Forms.Screen]::FromPoint([System.Windows.Forms.Cursor]::Position).WorkingArea
$cw = [int](420 * $s); $ch = [int](112 * $s); $m = [int](16 * $s)
$cx = $wa.Right - $cw - $m; $cy = $wa.Bottom - $ch - $m
Mark; Click ($cx + [int](187 * $s)) ($cy + [int](90 * $s)); Start-Sleep -Milliseconds 600
Shot '04-markdown-clicked'
Away
Clip 'markdown'
Front $ex
Keys ('src'); Start-Sleep -Milliseconds 600
InExplorer; Mark; Hotkey; Start-Sleep -Milliseconds 1200
Clip 'selected'

# 6. Two tabs: the hotkey follows the tab in front.
Keys ('{ESC}'); Start-Sleep -Milliseconds 200
Keys ('^t'); Start-Sleep -Milliseconds 3000
Keys ('%d'); Start-Sleep -Milliseconds 1000
Keys ("$T\demo\other"); Start-Sleep -Milliseconds 500
Keys ('{ENTER}'); Start-Sleep -Milliseconds 2500
Keys ('{ESC}'); Start-Sleep -Milliseconds 300
InExplorer; Mark; Hotkey; Start-Sleep -Milliseconds 1200
Clip 'tab-other'
Keys ('^{TAB}'); Start-Sleep -Milliseconds 1000
InExplorer; Mark; Hotkey; Start-Sleep -Milliseconds 1200
Clip 'tab-back'
Shot '05-tabs'

# 7. The hotkey with no Explorer in front: a note on the card, nothing copied.
$other = Start-Process charmap.exe -PassThru; Start-Sleep -Milliseconds 1500
Mark; Hotkey; Start-Sleep -Milliseconds 800
Shot '06-no-folder'
Log "NO EXPLORER: clipboard untouched: $([System.Windows.Forms.Clipboard]::GetText() -eq '(nothing yet)')"
$other | Stop-Process -Force -ErrorAction SilentlyContinue   # only the window this test opened

# 8. Idle cost of the resident once its card is gone.
Away
foreach ($i in 1..40) { if (-not [U]::IsWindowVisible([U]::Win('Twig.Card'))) { break }; Start-Sleep -Milliseconds 250 }
Log "card gone after $($i * 250) ms: $(-not [U]::IsWindowVisible([U]::Win('Twig.Card')))"
Start-Sleep -Seconds 2
$proc = Get-Process -Id $r.Id -ErrorAction SilentlyContinue
if ($proc) {
    $c1 = [uint64]0; [U]::QueryProcessCycleTime($proc.Handle, [ref]$c1) | Out-Null
    Start-Sleep -Seconds 10
    $c2 = [uint64]0; [U]::QueryProcessCycleTime($proc.Handle, [ref]$c2) | Out-Null
    $proc.Refresh()
    Log ("IDLE: {0:N0} CPU cycles in 10 s | private {1:N1} MB | working set {2:N1} MB" -f ($c2 - $c1), ($proc.PrivateMemorySize64 / 1MB), ($proc.WorkingSet64 / 1MB))
}

Finish 'DONE'
