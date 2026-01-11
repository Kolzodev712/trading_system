# Proposal: Private Crate Registry

**Version**: 1.0  
**Date**: December 2024  
**Status**: Draft

## Executive Summary

This proposal outlines the implementation of a **private crate registry** for publishing and distributing Rust crates within the organization. The primary implementation uses **Git-based registry** (Option 1) as it provides the best balance of simplicity, security, and performance for our current needs. The proposal also includes notes on alternative approaches (HTTP registry and hybrid) for future consideration as the project scales.

The registry will enable secure distribution of internal crates like `trading_core`, `exchange_engine`, `trading_engine`, etc., without exposing them to the public crates.io registry.

## Objectives

1. **Secure crate distribution** - Publish internal crates privately without exposing to crates.io
2. **Version control** - Use Git tags for versioning and release management
3. **Access control** - Leverage GitHub/GitLab private repository access control
4. **Performance** - Efficient crate fetching and dependency resolution
5. **CI/CD integration** - Seamless integration with build pipelines
6. **Future scalability** - Design allows migration to HTTP registry if needed

## Key Design Principles

1. **Simplicity First**: Start with Git-based approach (minimal infrastructure)
2. **Security**: Private repositories with proper access control
3. **Versioning**: Use semantic versioning with Git tags
4. **Performance**: Optimize for incremental updates and caching
5. **Migration Path**: Design allows transition to HTTP registry later
6. **Backward Compatibility**: Existing path dependencies continue to work

## Option 1: Private Git Registry (Implementation Focus)

### Overview

Use private Git repositories to distribute crates. Each crate has its own repository, and versions are tagged using Git tags.

**Architecture:**
```
Private GitHub/GitLab Organization
├── trading-core/          (private repo, crates as tags)
├── exchange-engine/       (private repo)
├── trading-engine/        (private repo)
├── market-data-engine/    (private repo)
├── indicator-engine/      (private repo)
├── strategy-engine/       (private repo)
└── ...                    (other internal crates)
```

### Performance Characteristics

| Operation | Time | Notes |
|-----------|------|-------|
| First clone | 1-5 seconds | Depends on repo size |
| Incremental update | 10-100ms | Git incremental fetch |
| Cargo cache | Persistent | Cached at `~/.cargo/git/` |
| CI/CD builds | Fast | With proper caching |

### Setup Steps

#### Step 1: Create Private Repositories

For each crate to be published:

```bash
# Example: trading-core
cd libs/trading-core

# Initialize if needed, or use existing repo
git init
git add .
git commit -m "Initial commit"

# Create private repository on GitHub/GitLab, then:
git remote add publish https://github.com/your-org/trading-core.git
git branch -M main
git push -u publish main
```

#### Step 2: Version and Tag Releases

```bash
# Update version in Cargo.toml
# Version: 0.1.0

# Create and push tag
git tag v0.1.0
git push publish v0.1.0

# For patch releases
git tag v0.1.1
git push publish v0.1.1
```

#### Step 3: Configure Cargo for Git Dependencies

**Option A: Direct Git Dependencies (Simpler)**

```toml
# Cargo.toml (consumer crate)
[dependencies]
trading_core = { git = "https://github.com/your-org/trading-core.git", tag = "v0.1.0" }
exchange_engine = { git = "https://github.com/your-org/exchange-engine.git", tag = "v0.1.0" }
```

**Option B: Custom Registry (More crates.io-like)**

Create a registry index repository:

```bash
# Create registry index repo
git init rust-registry
cd rust-registry

# Add registry config
# (Cargo will auto-generate index structure when publishing)
```

Configure Cargo:

```toml
# .cargo/config.toml (in project root or ~/.cargo/config.toml)
[registries.private]
index = "https://github.com/your-org/rust-registry.git"
```

Then in `Cargo.toml`:

```toml
[package]
publish = ["private"]  # Allow publishing to private registry

[dependencies]
trading_core = { version = "0.1.0", registry = "private" }
```

### Phase 1: Repository Setup

#### 1.1 Create Repository Structure

**Action Items:**
- [ ] Create private organization/group on GitHub/GitLab
- [ ] Create private repository for each crate:
  - `trading-core`
  - `exchange-engine`
  - `trading-engine`
  - `market-data-engine`
  - `indicator-engine`
  - `strategy-engine`
  - `asset-engine`
  - `live-config`
  - `runner`
  - (others as needed)

#### 1.2 Configure Repository Access

- [ ] Set up organization-level access control
- [ ] Configure team permissions (read/write)
- [ ] Set up SSH keys or personal access tokens
- [ ] Document access setup for team members

#### 1.3 Initial Version Tagging

For each crate:

```bash
# Update version in Cargo.toml to 0.1.0
# Commit changes
git add Cargo.toml
git commit -m "Release v0.1.0"

# Create and push tag
git tag -a v0.1.0 -m "Release version 0.1.0"
git push origin v0.1.0
```

### Phase 2: Cargo Configuration

#### 2.1 Choose Dependency Style

**Recommendation**: Start with direct Git dependencies (Option A) for simplicity.

Update `Cargo.toml` files to use Git dependencies:

```toml
# Example: src/runner/Cargo.toml
[dependencies]
trading_core = { git = "https://github.com/your-org/trading-core.git", tag = "v0.1.0" }
trading_engine = { git = "https://github.com/your-org/trading-engine.git", tag = "v0.1.0" }
```

#### 2.2 Authentication Setup

**For HTTPS (GitHub/GitLab):**

```bash
# Option 1: Personal Access Token
git config --global credential.helper store
# Add token to ~/.git-credentials:
# https://USERNAME:TOKEN@github.com

# Option 2: SSH (Recommended)
git remote set-url origin git@github.com:your-org/crate-name.git
# Ensure SSH keys are configured
```

**For CI/CD:**

```yaml
# .github/workflows/ci.yml
- name: Configure Git credentials
  run: |
    git config --global credential.helper store
    echo "https://${{ secrets.GITHUB_TOKEN }}@github.com" > ~/.git-credentials
```

#### 2.3 Workspace Dependency Migration

Gradually migrate from path dependencies to Git dependencies:

```toml
# Before (path dependencies)
[dependencies]
trading_core = { path = "../../libs/trading_core" }

# After (Git dependencies)
[dependencies]
trading_core = { git = "https://github.com/your-org/trading-core.git", tag = "v0.1.0" }
```

**Migration Strategy:**
1. Keep path dependencies for active development
2. Use Git dependencies for stable/released versions
3. Can mix both during transition

### Phase 3: Publishing Workflow

#### 3.1 Release Process

**Manual Release Workflow:**

```bash
# 1. Update version in Cargo.toml
# 2. Commit changes
git add Cargo.toml
git commit -m "Bump version to v0.2.0"

# 3. Create and push tag
git tag -a v0.2.0 -m "Release version 0.2.0"
git push origin v0.2.0
git push origin main  # Push commits

# 4. Update consumer crates to new version
```

#### 3.2 Automated Release (CI/CD)

```yaml
# .github/workflows/release.yml
name: Release Crate

on:
  push:
    tags:
      - 'v*'

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Configure Git
        run: |
          git config user.name "GitHub Actions"
          git config user.email "actions@github.com"
      
      - name: Verify version matches tag
        run: |
          TAG_VERSION=${GITHUB_REF#refs/tags/v}
          CARGO_VERSION=$(grep "^version" Cargo.toml | cut -d'"' -f2)
          if [ "$TAG_VERSION" != "$CARGO_VERSION" ]; then
            echo "Version mismatch: tag=$TAG_VERSION, cargo=$CARGO_VERSION"
            exit 1
          fi
```

### Phase 4: Performance Optimization

#### 4.1 Cargo Cache Configuration

```toml
# .cargo/config.toml
[net]
git-fetch-with-cli = true  # Use system git for better performance

[build]
incremental = true  # Enable incremental compilation
```

#### 4.2 CI/CD Optimization

**Shallow Clones:**
```yaml
- name: Checkout with shallow clone
  uses: actions/checkout@v3
  with:
    fetch-depth: 1  # Shallow clone for faster checkout
```

**Cache Git Repositories:**
```yaml
- name: Cache Cargo Git
  uses: actions/cache@v3
  with:
    path: ~/.cargo/git
    key: ${{ runner.os }}-cargo-git-${{ hashFiles('**/Cargo.lock') }}
    restore-keys: |
      ${{ runner.os }}-cargo-git-
```

**Cache Cargo Registry:**
```yaml
- name: Cache Cargo Registry
  uses: actions/cache@v3
  with:
    path: ~/.cargo/registry
    key: ${{ runner.os }}-cargo-registry-${{ hashFiles('**/Cargo.lock') }}
```

### Phase 5: Documentation and Tooling

#### 5.1 Documentation

- [ ] Document repository structure
- [ ] Document release process
- [ ] Document authentication setup
- [ ] Create contributor guide
- [ ] Document versioning strategy

#### 5.2 Helper Scripts

**Release script:**
```bash
#!/bin/bash
# scripts/release.sh

CRATE_NAME=$1
VERSION=$2

cd "$CRATE_NAME" || exit 1

# Update version in Cargo.toml
sed -i "s/^version = .*/version = \"$VERSION\"/" Cargo.toml

# Commit and tag
git add Cargo.toml
git commit -m "Release v$VERSION"
git tag -a "v$VERSION" -m "Release version $VERSION"
git push origin "v$VERSION"
git push origin main
```

**Update dependencies script:**
```bash
#!/bin/bash
# scripts/update-deps.sh

# Update all Git dependencies to latest tags
cargo update
```

## Alternative Options (Future Consideration)

### Option 2: Private HTTP Registry

**When to Consider:**
- 50+ internal crates
- Need faster CI/CD builds (microseconds vs milliseconds)
- Want crates.io-like experience
- Have infrastructure resources

**Implementation Options:**

**A. GitHub Packages (Easiest)**
```toml
# .cargo/config.toml
[registries.github]
index = "https://github.com/your-org/rust-registry.git"

# Publish
cargo publish --registry github
```

**B. Self-Hosted Cargo Registry**
- Use cargo-registry (same codebase as crates.io)
- Requires: Server, PostgreSQL, S3/storage
- More control, more maintenance

**C. Cloud Solutions**
- Cloudsmith: Managed Rust registry
- JFrog Artifactory: Enterprise artifact management
- GitLab Package Registry: Built-in support

**Performance Benefits:**
- HTTP downloads: ~100-500ms first fetch
- Incremental index updates: ~50-200ms
- Better caching support
- CDN integration possible

**Migration Path from Git Registry:**
1. Set up HTTP registry infrastructure
2. Publish existing crates to HTTP registry
3. Update Cargo.toml to use HTTP registry
4. Keep Git repos as backup/source of truth

### Option 3: Hybrid Approach

**When to Consider:**
- Want fast development (Git) + fast production (HTTP)
- Different requirements for dev vs release
- Gradual migration path

**Setup:**

**Development (fast iteration):**
```toml
[dependencies]
trading_core = { git = "https://github.com/your-org/trading-core.git", branch = "main" }
```

**Production (stable versions):**
```toml
[dependencies]
trading_core = { version = "0.1.0", registry = "private" }
```

**Benefits:**
- Fast development workflow (Git incremental updates)
- Fast production builds (HTTP downloads)
- Clear separation: dev vs release
- Can test new versions in dev before publishing

**Considerations:**
- Need to maintain both systems
- More complex dependency management
- Requires discipline to keep versions in sync

## Security Considerations

### Access Control

1. **Repository Access:**
   - Private repositories only
   - Organization-level access control
   - Team-based permissions

2. **Authentication:**
   - SSH keys (recommended for developers)
   - Personal access tokens (for CI/CD)
   - Rotate tokens regularly

3. **Secrets Management:**
   - Store tokens in CI/CD secrets
   - Never commit credentials
   - Use separate tokens for different purposes

### Version Security

1. **Immutable Tags:**
   - Once published, tags should not change
   - Use new version for fixes (semver)

2. **Verification:**
   - Verify tag signatures (optional but recommended)
   - CI/CD can verify version matches tag

3. **Audit Trail:**
   - Git history provides audit trail
   - Track who published which version

## Versioning Strategy

### Semantic Versioning

Follow [SemVer](https://semver.org/):

- **MAJOR.MINOR.PATCH** (e.g., 0.1.0)
- **MAJOR**: Breaking changes
- **MINOR**: New features, backward compatible
- **PATCH**: Bug fixes, backward compatible

### Pre-release Versions

Support alpha/beta/rc versions:

```bash
git tag v0.2.0-alpha.1
git tag v0.2.0-beta.1
git tag v0.2.0-rc.1
git tag v0.2.0  # Final release
```

### Version in Cargo.toml

```toml
[package]
name = "trading_core"
version = "0.1.0"  # Must match Git tag (without 'v' prefix)
```

## Integration with Existing Workflow

### Development Workflow

**Local Development:**
- Continue using path dependencies for active development
- Switch to Git dependencies for integration testing
- Use Git dependencies in CI/CD

**Example:**
```toml
# For local development (faster iteration)
[dependencies]
trading_core = { path = "../../libs/trading_core" }

# For CI/CD and releases (stable versions)
[dependencies]
trading_core = { git = "https://github.com/your-org/trading-core.git", tag = "v0.1.0" }
```

### CI/CD Integration

**Benefits:**
- Reproducible builds (locked versions)
- Faster builds with caching
- Easy to test against specific versions
- Clear version history

**Example Workflow:**
```yaml
- name: Cache dependencies
  uses: actions/cache@v3
  with:
    path: |
      ~/.cargo/git
      ~/.cargo/registry
    key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

- name: Build
  run: cargo build --release
```

## Migration Plan

### Phase 1: Setup (Week 1)
- [ ] Create private repositories for all crates
- [ ] Configure access control
- [ ] Tag initial versions (v0.1.0)

### Phase 2: Configuration (Week 1-2)
- [ ] Update Cargo.toml files with Git dependencies
- [ ] Set up authentication
- [ ] Test local builds

### Phase 3: CI/CD Integration (Week 2)
- [ ] Update CI/CD workflows
- [ ] Configure caching
- [ ] Test CI/CD builds

### Phase 4: Documentation (Week 2-3)
- [ ] Document release process
- [ ] Create helper scripts
- [ ] Update contributor guide

### Phase 5: Team Rollout (Week 3)
- [ ] Train team on new workflow
- [ ] Monitor for issues
- [ ] Gather feedback

## Testing Strategy

### Local Testing

```bash
# Test Git dependency resolution
cargo update

# Test build with Git dependencies
cargo build

# Test release process
./scripts/release.sh trading-core 0.1.0
```

### CI/CD Testing

- Test builds with Git dependencies
- Test version resolution
- Test cache effectiveness
- Test release automation

## Monitoring and Maintenance

### Metrics to Track

1. **Build Performance:**
   - Time to fetch dependencies
   - Cache hit rates
   - Build times

2. **Release Frequency:**
   - Versions published per week
   - Version adoption rate
   - Dependency update frequency

3. **Issues:**
   - Authentication failures
   - Version conflicts
   - Build failures

### Maintenance Tasks

- Regular dependency updates
- Version tag cleanup (if needed)
- Access token rotation
- Performance monitoring

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Git repo access issues | High | Use SSH keys, document setup clearly |
| Version conflicts | Medium | Use Cargo.lock, pin versions |
| Build performance degradation | Low | Implement caching, monitor metrics |
| Migration complexity | Medium | Gradual migration, keep path deps during transition |
| Repository bloat | Low | Use shallow clones, Git GC periodically |

## Success Criteria

1. ✅ All internal crates published to private repositories
2. ✅ Build times remain acceptable (< 10% increase)
3. ✅ CI/CD builds work reliably
4. ✅ Team can release versions easily
5. ✅ Documentation is clear and complete
6. ✅ Authentication setup is straightforward

## Future Enhancements

1. **Automated Version Bumping:** Script to bump versions across dependencies
2. **Dependency Update Bot:** Automatically update dependencies
3. **Release Automation:** Automated release on tag push
4. **Version Dashboard:** Track version usage across projects
5. **Migration to HTTP Registry:** When scale requires it

## Appendix: Quick Reference

### Common Commands

```bash
# Create new version
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0

# Update dependency version
# Edit Cargo.toml, change tag = "v0.1.1"
cargo update

# Check dependency versions
cargo tree

# Clear cache (if issues)
rm -rf ~/.cargo/git
cargo update
```

### Repository Structure Example

```
your-org/
├── trading-core/          (libs/trading_core → separate repo)
│   ├── Cargo.toml
│   ├── src/
│   └── tags: v0.1.0, v0.1.1, v0.2.0
├── exchange-engine/       (src/exchange_engine → separate repo)
└── ...
```

---

**Next Steps:**
1. Review and approve proposal
2. Set up private organization/group
3. Create repositories for each crate
4. Tag initial versions
5. Update Cargo.toml configurations
6. Test locally and in CI/CD
7. Document and train team

