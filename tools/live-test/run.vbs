' Starts the Twig test harness with no console window.
CreateObject("WScript.Shell").Run "powershell.exe -NoProfile -ExecutionPolicy Bypass -STA -File C:\twig-test\harness.ps1", 0, True
