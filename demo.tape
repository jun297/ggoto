# VHS Tape for ggoto demo
# Run: vhs demo.tape

Output assets/demo.gif

Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"

# Start the app in demo mode
Type "cargo run --release -- --demo"
Enter
Sleep 5s

# Navigate down through servers
Sleep 500ms
Type "j"
Sleep 300ms
Type "j"
Sleep 300ms
Type "j"
Sleep 300ms

# Show search/filter
Type "/"
Sleep 500ms
Type "prod"
Sleep 1s
Escape
Sleep 500ms

# Clear and show all
Type "/"
Escape
Sleep 500ms

# Navigate with vim keys
Type "k"
Sleep 200ms
Type "k"
Sleep 200ms
Type "G"
Sleep 500ms

# Toggle favorite
Type "f"
Sleep 800ms

# Cycle sort order
Type "s"
Sleep 600ms
Type "s"
Sleep 600ms
Type "s"
Sleep 600ms

# Show server details
Type "d"
Sleep 1.5s
Escape
Sleep 500ms

# Show help
Type "?"
Sleep 2s
Escape
Sleep 500ms

# Quit
Type "q"
Sleep 500ms
