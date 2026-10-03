Client SSH sessions now size their pseudo-terminal to the visible
console window and resize it when the window changes size, instead of
running at a fixed 200x50. Line wrapping and full-screen (TUI)
applications now render correctly, and clearing the screen no longer
leaves the prompt line blank.
