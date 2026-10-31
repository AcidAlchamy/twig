# Live test

Drives a real Twig on a **test PC** (it minimises every window, opens File Explorer on a made-up project, sends keys
and a click, and takes Twig out of Windows again when done):

1. Copy `Twig.exe`, `harness.ps1`, `run.vbs` and the `expected-*.txt` files to `C:\twig-test` on the test PC.
2. Run `run.vbs` inside the signed-in desktop session (an interactive scheduled task works: `schtasks /Create /TN TwigTest /TR "wscript.exe C:\twig-test\run.vbs" /SC ONCE /SD 01/01/2035 /ST 12:00 /IT /F`, then `schtasks /Run /TN TwigTest`, and `schtasks /Delete /TN TwigTest /F` when done; give it a far-off date, because a nearer one runs the test again by itself).
3. Read `C:\twig-test\log.txt`. Every `CLIP` line says whether the clipboard matched its `expected-*.txt` exactly. The `FRONT` line names the window in front before the hotkey: anything but File Explorer means something on that PC took the focus, not a Twig bug.

It refuses to start if the PC was used in the last two minutes (its own keys count, so wait two minutes between
runs). It checks the right-click command with no Twig running (copies, then exits by itself), the first run putting
the menu entry in, `--copy` handing a folder to the running Twig, a drive root (`subst T:`), the hotkey on the open
folder, the card's Markdown button, a selected folder, two Explorer tabs, the hotkey with no Explorer in front,
idle CPU and memory, and `--uninstall`.

Its screenshots (`shot-*.png`) show whatever is on that PC's screen: delete them when done.

If a system prompt keeps the focus, the harness stops rather than type into it. `explorer-check.ps1` needs no keys or
focus: it sets the selection through Explorer's own scripting interface and asks Twig's lookup about the window.

`scale-check.ps1` (started by `scale.vbs`, same copy-and-schedule steps) opens the settings window, the hello card and
the copy card and logs where each lands against the work area. For other display scales, build
`cargo build --release --features scale-test` and copy that exe in as `Twig.exe`: it reads `TWIG_TEST_SCALE`, draws as
if Windows were set to it, and saves what it really drew to `frame.bmp`, because a screen capture shows OpenGL windows
as blank white on a PC whose panel isn't showing. A test build stands in for layout and sizes only: the title bar
stays at the real scale.

A notification in the corner can sit over the card, and it stays there while nobody uses the PC (Macrium's update toast
did on the Carbon, seconds after an Explorer window opened). A real click would land on it and take the focus, so when
something covers the card the harness sends the click straight to the card's window and logs that it did.

`settings-shot.ps1` opens the settings window with a throwaway config and a made-up project and captures just that
window (for the README).
