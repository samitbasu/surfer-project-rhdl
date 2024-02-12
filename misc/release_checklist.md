# Release procedure

## Preparation

A few days before: write a post on blog.surfer-project.org highlighting important changes. Possibly also
take the opportunity to highlight other stuff that normally doesn't fit in a blog post, like new projects, publications? etc.

## Pre release procedure

- [ ] Surfer
    - [ ] Update changelog
        - [ ] Copy content from changelog wiki to CHANGELOG.md
        - [ ] Update unreleased compare link to latest version
        - [ ] Make sure the version header links to the diff between this and the previous version
    - [ ] Bump Cargo.toml version
    - [ ] Build and add Cargo.lock

## Release

- [ ] Merge changelog update MRs
- [ ] Tag resulting commit as `vX.Y.Z`
- [ ] Push tags
- [ ] Do a release on gitlab
- [ ] Upload Surfer release to zenodo
- [ ] Update release blog post MR with link to relevant changelog section. Merge blog

## Post release

- [ ] Announcements
    - [ ] Discord
    - [ ] Mastodon
