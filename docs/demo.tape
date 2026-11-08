# README demo. Render from the repo root: `mise run demo` (needs vhs, ttyd, ffmpeg and a Nerd Font).
# Default settings, read-only: no seed/mnemonic, throwaway config/data dirs, no transaction is signed.
Output docs/demo.gif

Set Shell "bash"
Set FontFamily "HackGen Console NF"
Set FontSize 15
Set Width 1500
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 60ms
Set PlaybackSpeed 1
Set Framerate 15

# Setup stays out of the recording: scrubbed env, empty config/data dirs.
Hide
Type "export D=$(mktemp -d); clear" Enter
Type "alias lazyxrp='env -i HOME=$HOME PATH=$PATH TERM=xterm-256color LANG=en_US.UTF-8 LAZYXRP_CONFIG=$D/config LAZYXRP_DATA=$D/data ./target/debug/lazyxrp 2>/dev/null'" Enter
Type "clear" Enter
Show

Type "lazyxrp" Sleep 400ms Enter
Sleep 7s

# Overview: move the selection, open the validator detail.
Type "j" Sleep 700ms
Type "k" Sleep 1s
Enter Sleep 3s
Escape Sleep 1s

# Account, Market (b loads the order book), Assets (o loads ledger objects).
Type "2" Sleep 4s
Type "j" Sleep 600ms
Type "j" Sleep 1s
Type "3" Sleep 2s
Type "b" Sleep 6s
Type "j" Sleep 600ms
Type "j" Sleep 1s
Type "4" Sleep 2s
Type "o" Sleep 6s

# Keymap help, then back to Overview.
Type "?" Sleep 3s
Escape Sleep 800ms
Type "1" Sleep 2s

# Quit outside the recording: no shell prompt or leftover splash in the last frame.
Hide
Type "q" Sleep 1s
