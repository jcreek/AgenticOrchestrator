"""Version gate and atomic publication for desktop releases (Python 3.11+)."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def version_tuple(version):
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
        raise ValueError(f"Expected a stable major.minor.patch version, got {version!r}")
    return tuple(map(int, version.split(".")))


def validate_versions(cargo, package, lock):
    version = tomllib.loads(cargo)["package"]["version"]
    version_tuple(version)
    if json.loads(package)["version"] != version:
        raise ValueError("Cargo.toml and package.json versions must match")
    entries = [p for p in tomllib.loads(lock)["package"] if p["name"] == "app" and "source" not in p]
    if len(entries) != 1 or entries[0]["version"] != version:
        raise ValueError("Cargo.lock is stale; run cargo check after bumping the version")
    return version


def increased(current, previous):
    delta = (version_tuple(current) > version_tuple(previous)) - (version_tuple(current) < version_tuple(previous))
    if delta < 0:
        raise ValueError("App version must not decrease")
    return delta > 0


def existing_release(repo, tag):
    # Do not mistake API/authentication failures for an absent release.
    pages = json.loads(run("gh", "api", f"repos/{repo}/releases", "--paginate", "--slurp"))
    return next((r for page in pages for r in page if r["tag_name"] == tag), None)


def verify_tag(tag, sha):
    result = subprocess.run(["git", "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}"], capture_output=True, text=True)
    if result.returncode == 0 and result.stdout.strip() != sha:
        raise ValueError(f"Tag {tag} already points to a different commit")
    return result.returncode == 0


def expected_assets(version):
    return {
        f"Agentic Orchestrator_{version}_x64-setup.exe",
        f"Agentic Orchestrator_{version}_aarch64.dmg",
        f"Agentic Orchestrator_{version}_x64.dmg",
    }


def expected_uploaded_assets(version):
    # GitHub replaces spaces with dots in release asset names. Local bundle
    # filenames retain spaces, so validate each side using its own names.
    return {name.replace(" ", ".") for name in expected_assets(version)}


def previous_version(before):
    # A force-push can leave event.before outside every fetched branch/tag.
    # Fetch that exact commit rather than treating it as an initial release.
    present = subprocess.run(
        ["git", "cat-file", "-e", f"{before}^{{commit}}"],
        capture_output=True, text=True,
    )
    if present.returncode != 0:
        # checkout disables persisted credentials; gh uses the job GH_TOKEN.
        run("git", "-c", "credential.helper=", "-c", "credential.helper=!gh auth git-credential", "fetch", "--no-tags", "origin", before)
    return tomllib.loads(run("git", "show", f"{before}:app/src-tauri/Cargo.toml"))["package"]["version"]


def gate():
    version = validate_versions(Path("app/src-tauri/Cargo.toml").read_text(), Path("app/package.json").read_text(), Path("Cargo.lock").read_text())
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    should_release = os.environ["GITHUB_EVENT_NAME"] == "workflow_dispatch"
    if not should_release:
        before = event.get("before", "")
        if before and set(before) != {"0"}:
            previous = previous_version(before)
            should_release = increased(version, previous)
        else:
            print("No previous commit; use a manual run to publish the initial version.")
    if should_release:
        tag = f"v{version}"
        tag_exists = verify_tag(tag, os.environ["GITHUB_SHA"])
        release = existing_release(os.environ["GITHUB_REPOSITORY"], tag)
        if release and not tag_exists and (not release["draft"] or release["target_commitish"] != os.environ["GITHUB_SHA"]):
            raise ValueError("Existing release does not target the built commit")
        if release and not release["draft"]:
            print(f"{tag} is already published; preserving it.")
            should_release = False
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"version={version}\nrelease={str(should_release).lower()}\n")
    print(f"Version {version}; release={should_release}")


def publish(version, directory):
    version_tuple(version)
    assets = sorted(Path(directory).iterdir())
    if {p.name for p in assets} != expected_assets(version) or any(not p.is_file() or not p.stat().st_size for p in assets):
        raise ValueError("Expected exactly three nonempty installers for this version")
    repo, sha = os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_SHA"]
    tag = f"v{version}"
    # Refresh tags immediately before checking for a collision.
    run("git", "fetch", "origin", "--tags")
    tag_exists = verify_tag(tag, sha)
    release = existing_release(repo, tag)
    if release and not tag_exists and (not release["draft"] or release["target_commitish"] != sha):
        raise ValueError("Existing release does not target the built commit")
    if release and not release["draft"]:
        print(f"{tag} already published; no changes made.")
        return
    if not release:
        run("gh", "release", "create", tag, "--repo", repo, "--target", sha, "--title", tag, "--draft", "--generate-notes")
    run("gh", "release", "upload", tag, *(str(p) for p in assets), "--repo", repo, "--clobber")
    release = existing_release(repo, tag)
    actual = {a["name"] for a in release["assets"]}
    expected = expected_uploaded_assets(version)
    if actual != expected:
        raise ValueError(f"Draft release assets mismatch: missing={sorted(expected - actual)}, unexpected={sorted(actual - expected)}")
    run("gh", "release", "edit", tag, "--repo", repo, "--draft=false", "--latest")


if __name__ == "__main__":
    if sys.argv[1] == "check":
        gate()
    elif sys.argv[1] == "publish":
        publish(*sys.argv[2:])
    else:
        raise ValueError("Expected check or publish")
