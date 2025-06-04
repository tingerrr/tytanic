# Contributing
Thank you for considering to contribute to Tytanic!
Any contributions are welcome, from implementing large features to fixing small typos.
If there's an issue you're working on let others know by leaving a comment, don't be afraid to ask for help if you're new to Tytanic or Rust programming.

**If you're contributing for the first time to Tytanic, please familiarize yourself with the workflow below.**

## Opening a PR
When you open a PR, it can be as messy as you want, but once you request review make sure you've brought it into a state at which you would merge it yourself.
The guidelines below outline what we'd like to see, but don't sweat it if you don't get the details right every time!
These guidelines are mostly there to make reviewing and bug hunting in the future easier, they're not hard requirements.

### Link To Issues
Link to issues that are related to a PR or commit, ideally in both.

Github picks thse up automatically when you place `Closes #42` or `Resolves #42, #45 and #46.` in your PR/commit descriptions.

### Linear History
Try to keep your history linear, avoid merges.

Rebase on main to update your branch instead of merging main into it.
You can avoid annoying conflicts by doing this frequently keeping each conflict resolution small and self contained.

### Atomic Commits
Try to keep commits atomic and self contained, each commit should ideally do one thing and pass CI on its own.

For example, if you need to refactor a function in order to add a new feature cleanly, put the refactoring in one commit and the new feature in a different commit.
Include tests and documentation in the same commit as the code they test and document.

Each commit should pass the CI (see the justfile `ci` recipe for running CI locally).

Squash any fixup commits before marking your PR as ready for review.

### Commit Descriptions
Start your commits with a topic like `cli: add:` or `runner:`.

Take a look at the other commits or ask a contributor if you're unsure.
Document your commits, add a short summary and describe the what and why, not the how of a patch.
Here's a good writeup of [how to write good commit messages](https://cbea.ms/git-commit/).

---

If you have trouble squashing, amending or reordering commits try out [jj].
It makes history rewriting very easy and is useful for git beginners and power-users alike.
If you prefer a GUI or TUI, there's also [gg] and [lazyjj] among others.

[jj]: https://github.com/martinvonz/jj
[gg]: https://github.com/gulbanana/gg
[lazyjj]: https://github.com/Cretezy/lazyjj
