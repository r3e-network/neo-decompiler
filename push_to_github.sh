#!/bin/bash

# Navigate to project directory
cd /home/neo/git/neo-decompiler

# Check git status
echo "=== Git Status ==="
git status

# Add remote (ignore if exists)
echo "=== Adding Remote ==="
git remote add origin git@github.com:r3e-network/neo-decompiler.git 2>/dev/null || echo "Remote already exists"

# Check remotes
echo "=== Remotes ==="
git remote -v

# Add and commit changes
echo "=== Committing Changes ==="
git add .
git commit -m "Update repository configuration for r3e-network

- Updated Cargo.toml homepage and repository URLs
- Updated README.md clone instructions  
- Changed organization from neo-project to r3e-network
- Binary name: neo-decompile → neo-decompiler
- Maintains all production functionality"

# Push to GitHub
echo "=== Pushing to GitHub ==="
git push -u origin master

echo "=== Push Complete ==="