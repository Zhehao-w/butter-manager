"""Choose expensive CI work without treating a pending/failed code run as validated."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess

ROOT_DOCS = {"readme.md", "contributing.md", "changelog.md", "agents.md", "roadmap.md"}
DOC_EXTENSIONS = {".md", ".rst", ".txt", ".png", ".jpg", ".jpeg", ".svg", ".webp", ".gif", ".pdf"}


def documentation(path):
    name = PurePosixPath(path)
    return (
        path.lower() in ROOT_DOCS
        or (path.startswith("docs/") and name.suffix.lower() in DOC_EXTENSIONS)
        or (path.startswith("src/assets/") and name.name == "README.md")
        or path == ".github/PULL_REQUEST_TEMPLATE.md"
        or (path.startswith(".github/ISSUE_TEMPLATE/") and name.suffix == ".md")
    )


def git(*args):
    return subprocess.check_output(["git", *args], stderr=subprocess.PIPE)


def source_key():
    # Hash tracked paths, blob IDs and modes from the actual checkout (including a PR merge).
    # Version this scheme when changing either the gate or the meaning of its success cache.
    digest = hashlib.sha256(b"butter-manager-ci-source-v1\0")
    for record in git("ls-tree", "-rz", "--full-tree", "HEAD").split(b"\0"):
        if record:
            _, path = record.split(b"\t", 1)
            if not documentation(os.fsdecode(path)):
                digest.update(record + b"\0")
    return digest.hexdigest()


def docs_between(before, after, *, merge_base=False):
    if not all(isinstance(sha, str) and re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", sha)
               and set(sha) != {"0"} for sha in (before, after)):
        return False
    try:
        paths = git("diff", "--name-only", "--no-renames", "-z",
                    before + ("..." if merge_base else "..") + after, "--").split(b"\0")
    except subprocess.CalledProcessError:
        # Force-push history, missing bases and unknown events must fail open to full checks.
        return False
    paths = [os.fsdecode(path) for path in paths if path]
    return bool(paths) and all(documentation(path) for path in paths)


def plan(event_name, event, ref):
    force = event_name not in {"push", "pull_request"} or ref.startswith("refs/tags/")
    whole_docs = commit_docs = False
    if not force and event_name == "push":
        whole_docs = docs_between(event.get("before"), event.get("after"))
    elif not force and event_name == "pull_request":
        pr = event.get("pull_request", {})
        head = pr.get("head", {}).get("sha")
        whole_docs = docs_between(pr.get("base", {}).get("sha"), head, merge_base=True)
        if event.get("action") == "synchronize":
            commit_docs = docs_between(event.get("before"), head)
    return {"source_key": source_key(), "commit": git("rev-parse", "HEAD").decode().strip(), "forced": force,
            "docs_only": whole_docs, "docs_commit": commit_docs}


def decision(scope, cache_hit):
    if scope["forced"]:
        return True, "Tag, Release or manual run: full validation and a fresh build"
    if scope["docs_only"]:
        return False, "Only documentation changed: no application build"
    if scope["docs_commit"] and cache_hit:
        return False, "Documentation commit: identical checkout source already passed Linux and Windows"
    return True, "Code/configuration changed, or matching successful validation is unavailable"


def outputs(values):
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
        for key, value in values.items():
            stream.write(f"{key}={str(value).lower() if isinstance(value, bool) else value}\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["plan", "decide"])
    parser.add_argument("--plan-file", type=Path, required=True)
    parser.add_argument("--cache-hit", default="false", choices=["true", "false", ""])
    args = parser.parse_args()
    if args.operation == "plan":
        with open(os.environ["GITHUB_EVENT_PATH"], encoding="utf-8") as stream:
            scope = plan(os.environ["GITHUB_EVENT_NAME"], json.load(stream), os.environ["GITHUB_REF"])
        args.plan_file.parent.mkdir(parents=True, exist_ok=True)
        args.plan_file.write_text(json.dumps(scope), encoding="utf-8")
        outputs(scope)
    else:
        scope = json.loads(args.plan_file.read_text(encoding="utf-8"))
        run, reason = decision(scope, args.cache_hit == "true")
        outputs({"run_checks": run, "reason": reason})
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as stream:
            stream.write(f"### CI scope\n\n{reason}.\n\n")
            if not run:
                stream.write("No new EXE is built or uploaded. Use the artifact from the last successful code build; "
                             "its embedded commit identifies that build. A manual workflow run forces a fresh build.\n")
        print(reason)


if __name__ == "__main__":
    main()
