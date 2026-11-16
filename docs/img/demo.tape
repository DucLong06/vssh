# README media, recorded from the real app. Run through docs/img/demo.sh.
Output docs/img/demo.gif
Set Shell "bash"
Set FontSize 16
Set Width 1280
Set Height 680
Set Padding 16
Set Theme "Catppuccin Mocha"
Set TypingSpeed 70ms
Env HOME "/tmp/vssh-demo/home"
Env SSH_AUTH_SOCK ""

# Off camera: two real sessions so the usage stats are real, then the demo
# groups and tags, then start again.
Hide
Type "cd /tmp/vssh-demo/work && clear && /tmp/vssh-demo/vssh" Enter
Sleep 1s
Type "/web-01" Enter Enter
Sleep 3s
Type "uptime" Enter
Sleep 500ms
Ctrl+A
Type "x"
Enter
Sleep 3s
Ctrl+A
Type "x"
Escape
Type "q"
Sleep 500ms
Type "python3 /tmp/vssh-demo/meta.py && clear && /tmp/vssh-demo/vssh" Enter
Sleep 1500ms
Show

# Host list: check reachability, move around.
Sleep 1s
Type "r"
Sleep 2500ms
Type "j"
Sleep 1500ms
Screenshot docs/img/hosts.png
Sleep 500ms
Type "jjj"
Sleep 800ms

# Filter by tag.
Type "/#web"
Sleep 2s
Escape
Sleep 500ms

# Start a port forward.
Type "j"
Type "f"
Sleep 800ms
Enter
Sleep 2500ms
Escape
Sleep 800ms

# Quick connect: nothing is saved.
Type "c"
Sleep 500ms
Type "root@web-01.localhost:2222"
Tab
Type "demo-password"
Sleep 1s
Screenshot docs/img/quick-connect.png
Sleep 500ms
Enter
Sleep 2500ms
Type "ls -la"
Enter
Sleep 1s
Type "head -2 /etc/os-release"
Enter
Sleep 2s
Screenshot docs/img/session.png
Sleep 500ms

# Back to the list, then SFTP on the same host.
Ctrl+A
Type "h"
Sleep 1s
Type "s"
Sleep 2500ms
Tab
Type "jj"
Sleep 1500ms
Screenshot docs/img/sftp.png
Sleep 500ms
Ctrl+A
Type "h"
Sleep 2s
