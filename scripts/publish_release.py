"""Publish verified CI assets. GitHub credentials are used only by the gh CLI."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
from urllib.parse import quote
import zipfile


def run_gh(args):
    return subprocess.run(
        ["gh", *args], check=True, text=True, capture_output=True
    ).stdout


def file_digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def validate_assets(directory, tag):
    exe_name = "butter-manager.exe"
    zip_name = f"butter-manager-{tag}-windows-x64-portable.zip"
    release_names = [zip_name, "SHA256SUMS.txt"]
    # The EXE is required for CI's byte-for-byte verification, but never uploaded.
    if {p.name for p in directory.iterdir()} != {exe_name, *release_names}:
        raise ValueError("Expected a build EXE, Portable ZIP and SHA256SUMS.txt.")
    built = {}
    for name in [exe_name, *release_names]:
        path = directory / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"Invalid build/release asset: {name}")
        built[name] = {"digest": "sha256:" + file_digest(path), "size": path.stat().st_size}
    lines = (directory / "SHA256SUMS.txt").read_text(encoding="utf-8-sig").splitlines()
    if len(lines) != 1:
        raise ValueError("Expected exactly one ZIP checksum.")
    match = re.fullmatch(r"([a-f0-9]{64})  (.+)", lines[0])
    if not match or match[2] != zip_name:
        raise ValueError("Invalid checksum entry; only the Portable ZIP may be listed.")
    if "sha256:" + match[1] != built[zip_name]["digest"]:
        raise ValueError("ZIP checksum mismatch.")
    with zipfile.ZipFile(directory / zip_name) as archive:
        entries = archive.infolist()
        if (
            len(entries) != 1
            or entries[0].filename != "butter-manager/butter-manager.exe"
            or entries[0].file_size != built[exe_name]["size"]
        ):
            raise ValueError("ZIP must contain only the matching executable.")
        with archive.open(entries[0]) as stream:
            digest = "sha256:" + hashlib.file_digest(stream, "sha256").hexdigest()
        if digest != built[exe_name]["digest"]:
            raise ValueError("ZIP executable differs from the build EXE.")
    return {name: built[name] for name in release_names}

def publish_release(directory, tag, version, repository, commit, linux_commit, create=False, gh=run_gh, sleep=time.sleep):
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", tag):
        raise ValueError("Expected a v-prefixed version tag, such as v1.0.0.")
    if tag != f"v{version}":
        raise ValueError("Release tag must match the checked-out application version.")
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or linux_commit != commit:
        raise ValueError("Linux and Windows must verify the same full commit SHA.")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("Invalid repository.")
    assets = validate_assets(Path(directory), tag)
    prefix = f"repos/{repository}"

    def api(path):
        return json.loads(gh(["api", f"{prefix}/{path}"]))

    def verify_tag():
        obj = api(f"git/ref/tags/{quote(tag, safe='')}")["object"]
        for _ in range(20):
            if obj["type"] != "tag":
                break
            obj = api(f"git/tags/{obj['sha']}")["object"]
        if obj["type"] != "commit" or obj["sha"] != commit:
            raise ValueError("Tag no longer points to the verified build commit.")

    def release():
        try:
            return api(f"releases/tags/{quote(tag, safe='')}")
        except subprocess.CalledProcessError as error:
            if "(HTTP 404)" not in (error.stderr or ""):
                raise
            # The tag endpoint may omit drafts. List authenticated releases too;
            # an API/auth failure must never be mistaken for a missing Release.
            remaining = gh(["api", "--paginate", f"{prefix}/releases?per_page=100"]).strip()
            matches = []
            while remaining:
                page, end = json.JSONDecoder().raw_decode(remaining)
                matches.extend(item for item in page if item["tag_name"] == tag)
                remaining = remaining[end:].strip()
            if len(matches) > 1:
                raise ValueError("Multiple Releases use the same tag.")
            return matches[0] if matches else None

    verify_tag()
    current = release()
    if current is None:
        if not create:
            raise ValueError("Manual/Release-event uploads require an existing Release.")
        args = [
            "release", "create", tag, "--repo", repository, "--verify-tag", "--draft",
            "--title", tag, "--generate-notes",
        ]
        if "-" in tag.split("+", 1)[0]:
            args.append("--prerelease")
        gh(args)
        # A newly created draft can take a few seconds to appear in the list.
        # Retry reads only; never repeat creation or publish an unverified draft.
        for attempt in range(6):
            current = release()
            if current is not None:
                break
            if attempt < 5:
                sleep(2)
        if current is None or not current["draft"]:
            raise ValueError("Expected the newly created Release to be a draft.")
    release_id = current["id"]
    remote = {asset["name"]: asset for asset in current["assets"]}
    if create and current["draft"] and remote.keys() - assets.keys():
        raise ValueError("Draft contains unexpected assets; refusing to publish them.")
    upload = []
    for name, expected in assets.items():
        if current["draft"] or name not in remote:
            upload.append(str(Path(directory) / name))
        elif any(remote[name].get(key) != value for key, value in expected.items()):
            raise ValueError(f"Refusing to replace a different published asset: {name}")
    if upload:
        args = ["release", "upload", tag, *upload, "--repo", repository]
        if current["draft"]:
            args.append("--clobber")
        gh(args)
    verified = release()
    if verified is None or verified["id"] != release_id:
        raise ValueError("Release changed during upload.")
    remote = {asset["name"]: asset for asset in verified["assets"]}
    if create and verified["draft"] and remote.keys() - assets.keys():
        raise ValueError("Draft contains unexpected assets; refusing to publish them.")
    for name, expected in assets.items():
        if name not in remote or any(remote[name].get(key) != value for key, value in expected.items()):
            raise ValueError(f"Uploaded asset verification failed: {name}")
    verify_tag()
    if create and verified["draft"]:
        prerelease = "true" if "-" in tag.split("+", 1)[0] else "false"
        gh([
            "release", "edit", tag, "--repo", repository,
            "--draft=false", f"--prerelease={prerelease}",
        ])
        published = release()
        if published is None or published["id"] != release_id or published["draft"]:
            raise ValueError("Release did not become public after publishing.")
    print(f"Verified release assets: {repository} {tag} ({commit})")


if __name__ == "__main__":
    try:
        root = Path(__file__).resolve().parent.parent
        publish_release(
            root / "artifacts/windows",
            os.environ["RELEASE_TAG"],
            json.loads((root / "package.json").read_text())["version"],
            os.environ["GH_REPO"],
            os.environ["EXPECTED_COMMIT"],
            os.environ["LINUX_COMMIT"],
            create=os.environ.get("CREATE_RELEASE") == "true",
        )
    except (ValueError, OSError, KeyError, zipfile.BadZipFile, subprocess.CalledProcessError) as error:
        print(f"Release publishing failed: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError):
            print(error.stderr, file=sys.stderr)
        sys.exit(1)
