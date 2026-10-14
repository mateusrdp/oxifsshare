# First-time GitHub setup

Replace `OWNER` with your GitHub user or organisation everywhere it appears
(`Cargo.toml` `repository`, README links).

## 1. Create the repository

1. On GitHub: **New repository**, name `oxifsshare`, **Public**.
   Do **not** tick "Add a README", ".gitignore" or "licence" (the project already has them).
2. Locally, in the project folder:

   ```
   git init -b main
   git add .
   git status        # read this list carefully: no PDFs, TIFFs, .msg or fs-share.xml
   git commit -m "Initial scaffold"
   git remote add origin git@github.com:OWNER/oxifsshare.git
   git push -u origin main
   ```

   Check `git status` before the first commit. If anything archive-like shows up, stop.

## 2. Enable Actions

- **Settings > Actions > General > Actions permissions**: allow actions and reusable workflows.
- **Workflow permissions**: choose **Read repository contents and packages permissions**.
  The release workflow (milestone 4) requests `contents: write` for its release job only.
- Leave "Allow GitHub Actions to create and approve pull requests" off.

## 3. Branch protection

**Settings > Rules > Rulesets > New branch ruleset**, target `main`:

- Require a pull request before merging.
- Require status checks to pass: select `fmt, clippy, test, build` and `MSRV build`
  (they appear after the first CI run).
- Block force pushes and deletion.

Note: once pushes to `main` require pull requests, the release workflow runs on merge.

## 4. Dependabot and security

- **Settings > Code security**: enable Dependency graph, Dependabot alerts and
  Dependabot security updates. `.github/dependabot.yml` handles version updates.
- Enable **Private vulnerability reporting** (used by `SECURITY.md`, milestone 4).
