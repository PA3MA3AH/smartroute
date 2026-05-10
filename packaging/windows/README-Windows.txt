SmartRoute for Windows
======================

SmartRoute is a local smart proxy router built on top of sing-box.

Installed files
---------------

Default install directory:
  C:\Program Files\SmartRoute

Runtime directory:
  C:\ProgramData\SmartRoute\run

Suggested config directory:
  C:\ProgramData\SmartRoute\config

Requirements
------------

1. Windows 10/11 x64.
2. sing-box.exe must be installed and available in PATH.

You can also point SmartRoute to a custom sing-box.exe path:

  setx SMARTROUTE_SINGBOX "C:\path\to\sing-box.exe" /M

Basic usage
-----------

Open PowerShell and run:

  smartroute.exe --help
  smartroute.exe doctor C:\ProgramData\SmartRoute\config\imported.toml
  smartroute.exe start C:\ProgramData\SmartRoute\config\imported.toml
  smartroute.exe status
  smartroute.exe stop

Administrator features
----------------------

Run PowerShell or Windows Terminal as Administrator for these commands:

  smartroute.exe kill-switch enable C:\ProgramData\SmartRoute\config\imported.toml
  smartroute.exe kill-switch status
  smartroute.exe kill-switch disable

  smartroute.exe autostart enable C:\ProgramData\SmartRoute\config\imported.toml
  smartroute.exe autostart status
  smartroute.exe autostart disable

Notes
-----

The installer does not bundle sing-box.exe. Install sing-box separately or set SMARTROUTE_SINGBOX.
