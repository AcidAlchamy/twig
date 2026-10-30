# Asks Twig's Explorer lookup about a real window, without keys or focus: Explorer's own scripting interface sets the
# selection, then the live_window test answers. Run it in the signed-in desktop session, after harness.ps1 made the
# demo folder, with the test exe copied to C:\twig-test\twig-check.exe (cargo test --release --no-run prints its path).
$T = 'C:\twig-test'
$out = "$T\explorer-check.txt"
Remove-Item $out, "$T\CHECKED" -ErrorAction SilentlyContinue
function Say($m) { $m | Add-Content $out -Encoding UTF8 }
$app = "$T\demo\my-app"
function Check($what) {
    Say "--- $what"
    $env:TWIG_HWND = "$($win.HWND)"
    & "$T\twig-check.exe" --ignored --nocapture live_window 2>&1 | Where-Object { $_ -match '^selection' } | ForEach-Object { Say "$_" }
}
$shell = New-Object -ComObject Shell.Application
Start-Process explorer.exe $app
$win = $null
foreach ($i in 1..40) {
    Start-Sleep -Milliseconds 250
    $win = @($shell.Windows()) | Where-Object { $_.LocationURL -like '*/my-app' } | Select-Object -First 1
    if ($win) { break }
}
Start-Sleep -Milliseconds 800
$doc = $win.Document
Check 'nothing selected: my-app both times'
$doc.SelectItem($doc.Folder.ParseName('src'), 29); Start-Sleep -Milliseconds 400
Check 'src selected: my-app, then src'
$doc.SelectItem($doc.Folder.ParseName('README.md'), 29); Start-Sleep -Milliseconds 400
Check 'a file selected: my-app both times'
$doc.SelectItem($doc.Folder.ParseName('src'), 29); $doc.SelectItem($doc.Folder.ParseName('tests'), 1); Start-Sleep -Milliseconds 400
Check 'two folders selected: my-app both times'
$win.Navigate2('shell:MyComputerFolder'); Start-Sleep -Milliseconds 1500
Check 'This PC: not on disk'
$win.Quit()
'DONE' | Set-Content "$T\CHECKED"
