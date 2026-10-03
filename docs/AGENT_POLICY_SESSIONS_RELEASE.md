# Release gate for issue #33

Do not merge this slice until both the focused Supervisor Console workflow and repository-wide CI are green at the exact PR head.

After merge, require post-merge `main` CI to pass before calling the operator surface delivered.
