Download the following 10 VST2 DLLs from https://www.airwindows.com/vsts/ (WinVST64s.zip) and place them in this directory:

- Air.dll
- BlockParty.dll
- DeEss.dll
- Density.dll
- Gatelope.dll
- Pressure4.dll
- PurestConsoleChannel.dll
- PurestDrive.dll
- ToVinyl4.dll
- Verbity.dll

All plugins are MIT licensed. See AIRWINDOWS_LICENSE.txt.

After placing the DLLs here, add this to the `bundle.resources` array in `tauri.conf.json`:
  "resources/vst/*.dll"
