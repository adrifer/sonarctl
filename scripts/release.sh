#!/usr/bin/env bash
set -euo pipefail

mode="${1:-}"
shift || true

if [[ "$mode" != "stable" && "$mode" != "preview" ]]; then
  echo "Usage: $0 stable|preview [--patch|--minor|--major] [--dry-run]" >&2
  exit 1
fi

bump=""
dry_run=false
for arg in "$@"; do
  case "$arg" in
    --patch|--minor|--major)
      if [[ -n "$bump" ]]; then
        echo "Use only one version bump flag." >&2
        exit 1
      fi
      bump="${arg#--}"
      ;;
    --dry-run)
      dry_run=true
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

parse_version() {
  local version="$1"
  if [[ ! "$version" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)(-preview\.([0-9]+))?$ ]]; then
    return 1
  fi
  version_major="${BASH_REMATCH[1]}"
  version_minor="${BASH_REMATCH[2]}"
  version_patch="${BASH_REMATCH[3]}"
  version_preview="${BASH_REMATCH[5]:--1}"
}

version_gt() {
  local left="$1"
  local right="$2"
  local left_major left_minor left_patch left_preview
  local right_major right_minor right_patch right_preview

  parse_version "$left"
  left_major="$version_major"
  left_minor="$version_minor"
  left_patch="$version_patch"
  left_preview="$version_preview"
  parse_version "$right"
  right_major="$version_major"
  right_minor="$version_minor"
  right_patch="$version_patch"
  right_preview="$version_preview"

  local left_parts=("$left_major" "$left_minor" "$left_patch")
  local right_parts=("$right_major" "$right_minor" "$right_patch")
  for index in 0 1 2; do
    if ((left_parts[index] > right_parts[index])); then
      return 0
    fi
    if ((left_parts[index] < right_parts[index])); then
      return 1
    fi
  done

  if ((left_preview == -1 && right_preview != -1)); then
    return 0
  fi
  if ((left_preview != -1 && right_preview == -1)); then
    return 1
  fi
  ((left_preview > right_preview))
}

bump_version() {
  local version="$1"
  local part="$2"
  parse_version "$version"
  case "$part" in
    major) printf '%d.0.0\n' "$((version_major + 1))" ;;
    minor) printf '%d.%d.0\n' "$version_major" "$((version_minor + 1))" ;;
    patch) printf '%d.%d.%d\n' "$version_major" "$version_minor" "$((version_patch + 1))" ;;
  esac
}

git fetch --tags --quiet

branch="$(git branch --show-current)"
if [[ "$mode" == "stable" && "$branch" != "main" ]]; then
  echo "Stable releases must be created from main. Current branch is ${branch:-"(detached HEAD)"}." >&2
  exit 1
fi
if [[ -n "$(git status --porcelain)" ]]; then
  echo "Working tree must be clean before creating a release." >&2
  exit 1
fi

package_version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
if ! parse_version "$package_version"; then
  echo "Cargo.toml must contain a stable or preview semver package version." >&2
  exit 1
fi

versions=("$package_version")
while IFS= read -r tag; do
  version="${tag#v}"
  if parse_version "$version"; then
    versions+=("$version")
  fi
done < <(git tag --list "v*")

latest="${versions[0]}"
latest_stable=""
for version in "${versions[@]}"; do
  if version_gt "$version" "$latest"; then
    latest="$version"
  fi
  parse_version "$version"
  if ((version_preview == -1)) && { [[ -z "$latest_stable" ]] || version_gt "$version" "$latest_stable"; }; then
    latest_stable="$version"
  fi
done

parse_version "$latest"
if [[ "$mode" == "stable" ]]; then
  if ((version_preview != -1)) && [[ -z "$bump" ]]; then
    next_version="$version_major.$version_minor.$version_patch"
  else
    next_version="$(bump_version "${latest_stable:-$latest}" "${bump:-patch}")"
  fi
else
  if ((version_preview != -1)) && [[ -z "$bump" ]]; then
    next_version="$version_major.$version_minor.$version_patch-preview.$((version_preview + 1))"
  else
    preview_base="$(bump_version "${latest_stable:-$latest}" "${bump:-patch}")"
    next_version="$preview_base-preview.0"
  fi
fi

next_tag="v$next_version"
if git rev-parse "$next_tag" >/dev/null 2>&1; then
  echo "Tag $next_tag already exists." >&2
  exit 1
fi

echo "Creating $mode release $next_tag"
if [[ "$dry_run" == true ]]; then
  echo "Dry run only; no files, commits, tags, or remotes were changed."
  exit 0
fi

cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked

if [[ "$mode" == "stable" ]]; then
  sed -i "0,/^version = \".*\"/s//version = \"$next_version\"/" Cargo.toml
  cargo metadata --format-version 1 --no-deps >/dev/null
  git add Cargo.toml Cargo.lock
  git commit -m "release: $next_tag"
fi

git tag -a "$next_tag" -m "$next_tag"
if [[ "$mode" == "stable" ]]; then
  git push origin "HEAD:$branch"
fi
git push origin "$next_tag"

echo "Release tag $next_tag pushed. GitHub Actions will publish its Windows assets."
