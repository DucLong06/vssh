vssh for Windows: the terminal app
==================================

Run it
  Unzip, then from PowerShell or Windows Terminal, in that folder:

      .\vssh.exe

  Windows Terminal draws it best; the old console window has fewer colours.
  .\vssh.exe --help lists the options, and ? inside vssh lists the keys.
  To start it from anywhere, put the folder on your PATH.

Where things are
  Hosts       %USERPROFILE%\.ssh\config        (shared with OpenSSH for Windows)
  Host keys   %USERPROFILE%\.ssh\known_hosts
  Data        %APPDATA%\vssh\vssh.db and %APPDATA%\vssh\vault
  Settings    %APPDATA%\vssh\config.toml       (optional)

  Paste %APPDATA%\vssh into the address bar of Explorer to open that folder.

Not on Windows yet
  The ssh agent and ProxyCommand. Log in with a key file that has no
  passphrase, or with a password kept in the vault. ProxyJump works.

Everything else: https://github.com/DucLong06/vssh
