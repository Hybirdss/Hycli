#!/usr/bin/env python3
"""Export the Reddit home feed or saved items through Hycli's verified browser session.

Needs a Hycli build with `hycli accounts read`. Routes and the identity check live in
reddit.json. See README.md.
"""
import argparse
import html
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.parse
from datetime import datetime, timezone

HERE = Path(__file__).resolve().parent


class HycliError(RuntimeError):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def write_json(destination, data):
    temporary = destination.with_name("." + destination.name + ".tmp")
    temporary.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    os.chmod(temporary, 0o600)
    temporary.replace(destination)


def media_of(post):
    items = []
    metadata = post.get("media_metadata") or {}
    for entry in (post.get("gallery_data") or {}).get("items", []):
        source = (metadata.get(entry.get("media_id")) or {}).get("s") or {}
        if source.get("u") or source.get("gif"):
            items.append({"type": "image", "url": source.get("u") or source.get("gif"), "caption": entry.get("caption")})
    if not items:
        for image in (post.get("preview") or {}).get("images", []):
            if image.get("source", {}).get("url"):
                items.append({"type": "image", "url": image["source"]["url"], "caption": None})
    video = ((post.get("secure_media") or post.get("media") or {}).get("reddit_video") or {}).get("fallback_url")
    if video:
        items.append({"type": "video", "url": video, "caption": None})
    return items


def normalize(child, base_url):
    kind = child.get("kind")
    data = child.get("data") or {}
    if kind not in ("t3", "t1") or not data.get("name"):
        return None
    created = data.get("created_utc")
    published = datetime.fromtimestamp(created, timezone.utc).isoformat() if isinstance(created, (int, float)) else None
    url = base_url + data["permalink"] if data.get("permalink") else None
    subreddit = data.get("subreddit_name_prefixed") or ("r/" + data["subreddit"] if data.get("subreddit") else None)
    if kind == "t1":
        return {"id": data["name"], "kind": "comment", "title": data.get("link_title"), "text": data.get("body") or "", "author": data.get("author"), "subreddit": subreddit, "published_at": published, "url": url, "external_url": None, "score": data.get("score"), "num_comments": None, "over_18": bool(data.get("over_18")), "media": []}
    external = data.get("url_overridden_by_dest") or (None if data.get("is_self") else data.get("url"))
    return {
        "id": data["name"],
        "kind": "post",
        "title": data.get("title"),
        "text": data.get("selftext") or "",
        "author": data.get("author"),
        "subreddit": subreddit,
        "published_at": published,
        "url": url,
        "external_url": external if external and external != url else None,
        "score": data.get("score"),
        "num_comments": data.get("num_comments"),
        "over_18": bool(data.get("over_18")),
        "flair": data.get("link_flair_text"),
        "media": media_of(data),
    }


def main():
    definitions = json.loads((HERE / "reddit.json").read_text(encoding="utf-8"))
    base_url = definitions["base_url"]
    parser = argparse.ArgumentParser(description="Export the Reddit home feed or saved items through Hycli.")
    parser.add_argument("--account", help="Reddit username of a Hycli account, without u/")
    parser.add_argument("--account-id", help="Hycli account id, when the username alone is not enough")
    parser.add_argument("--timeline", required=True, choices=sorted(definitions["timelines"]))
    parser.add_argument("--max-posts", type=int, help="Stop once this many unique items are collected (required for the endless home feed)")
    parser.add_argument("--output", type=Path, help="Exact output directory")
    parser.add_argument("--output-base", type=Path, default=Path.home() / "Downloads", help="Parent of a new timestamped output directory")
    parser.add_argument("--hycli", default="hycli")
    parser.add_argument("--local-wait", type=float, default=30, help="Seconds to wait when Hycli's own request budget refuses a read")
    parser.add_argument("--check", action="store_true", help="Verify the account and read one page, printing counts only")
    args = parser.parse_args()
    if not args.account and not args.account_id:
        parser.error("--account or --account-id is required")
    timeline = args.timeline
    spec = definitions["timelines"][timeline]
    if spec.get("endless") and not args.max_posts and not args.check:
        parser.error(f"--max-posts is required for the endless {timeline} feed")
    exe = shutil.which(args.hycli)
    if not exe:
        raise SystemExit("Hycli executable not found")
    wanted = (args.account or "").removeprefix("u/").lstrip("@")

    def call(arguments):
        while True:
            result = subprocess.run([exe] + arguments, capture_output=True, text=True, timeout=90)
            if result.returncode == 0:
                return json.loads(result.stdout)
            if re.search(r"\bsite_rate_limited\b", result.stderr):
                # Reddit itself answered 429: stop instead of knocking again
                raise RuntimeError("Reddit returned a rate limit (429); stopped without retrying. Try again later.")
            if re.search(r"\brate_limited\b", result.stderr):
                # Hycli's own request budget refused locally; nothing reached Reddit
                print(json.dumps({"status": "waiting_for_local_rate_limit", "at": now()}), flush=True)
                time.sleep(args.local_wait)
                continue
            code = re.search(r"code=\d+ (\w+)", result.stderr)
            raise HycliError(code.group(1) if code else "", f"Hycli exit {result.returncode}: {result.stderr.strip()}")

    def with_evidence(arguments, account_id, url):
        # hycli reads only routes it has observed; record a new route once, then read it
        try:
            return call(arguments)
        except HycliError as error:
            if error.code != "unsupported_evidence":
                raise
        print(json.dumps({"status": "recording_route", "path": urllib.parse.urlparse(url).path}), flush=True)
        observed = call(["accounts", "observe", account_id, url])
        if not (observed.get("observation") or {}).get("successful"):
            raise RuntimeError(f"Recording {urllib.parse.urlparse(url).path} failed with status {(observed.get('observation') or {}).get('status')}")
        return call(arguments)

    def verify(account):
        recipe = call(["accounts", "inspect", account["id"]]).get("saved_recipe") or definitions["identity_recipe"]
        with tempfile.TemporaryDirectory(prefix="hycli-reddit-identity-") as temporary:
            recipe_file = Path(temporary) / "recipe.json"
            write_json(recipe_file, recipe)
            return with_evidence(["accounts", "verify", account["id"], "--recipe", str(recipe_file)], account["id"], recipe["identity_url"])

    def confirm(account):
        # An already verified account only needs its own session read back. `accounts verify` also sends
        # a cookie-less baseline request, which Reddit throttles (429 or Retry-After).
        url = definitions["identity_recipe"]["identity_url"]
        me = with_evidence(["accounts", "read", account["id"], url], account["id"], url).get("data") or {}
        return {"verified_identity": bool(me.get("name")), "name": me.get("name"), "user_id": me.get("id")}

    connected = [a for a in call(["accounts"]) if a.get("site_url") == base_url and a.get("status") == "connected"]
    if args.account_id:
        connected = [a for a in connected if a.get("id") == args.account_id]

    def matches(name):
        return bool(name) and (not wanted or name.lower() == wanted.lower())

    candidates = [a for a in connected if a.get("verified_identity") and matches(a.get("name"))]
    verified = None
    if not candidates:
        # Sessions imported from browser profiles start unverified; check them until the user is found.
        for account in [a for a in connected if not a.get("verified_identity")]:
            print(json.dumps({"status": "verifying_profile", "profile": account.get("profile")}), flush=True)
            try:
                identity = verify(account)
            except HycliError as error:
                print(json.dumps({"status": "profile_not_verified", "profile": account.get("profile"), "code": error.code}), flush=True)
                continue
            if identity.get("verified_identity") and matches(identity.get("name")):
                candidates, verified = [dict(account, name=identity["name"], user_id=identity.get("user_id"))], identity
                break
    if not candidates:
        raise SystemExit(f"No verified Hycli Reddit account matches {'u/' + wanted if wanted else args.account_id}. Check `hycli accounts`.")
    # The same Reddit user can be connected from several browser profiles; use the most recently verified one.
    account = max(candidates, key=lambda a: a.get("checked_at") or "")
    if verified is None:
        verified = confirm(account)
    if not verified.get("verified_identity") or verified.get("name") != account.get("name") or (account.get("user_id") and verified.get("user_id") != account.get("user_id")):
        raise RuntimeError("The verified account does not match the requested account")
    name = verified["name"]
    path = spec["path"].format(name=urllib.parse.quote(name))

    def page_url(after):
        params = dict(definitions["params"], after=after or "")
        return base_url + path + "?" + urllib.parse.urlencode(params)

    def read(after):
        url = page_url(after)
        return with_evidence(["accounts", "read", account["id"], url], account["id"], url)

    if args.check:
        listing = read(None)["data"]
        print(json.dumps({"account": "u/" + name, "timeline": timeline, "verified_identity": True, "items_on_first_page": len(listing.get("children", [])), "has_next_cursor": bool(listing.get("after"))}))
        return 0
    if args.output:
        output = args.output
        output.mkdir(parents=True, exist_ok=True)
    else:
        stamp = datetime.now().astimezone().strftime("%Y-%m-%d-%H%M%S")
        output = args.output_base / f"reddit-{timeline}-{name}-{stamp}"
        output.mkdir(parents=True, exist_ok=False)
    os.chmod(output, 0o700)
    if (output / "03-manifest.json").exists():
        raise RuntimeError("An export already exists here; select a new output directory")
    shutil.copy2(__file__, output / "05-export.py")
    write_json(output / "06-request.json", {"timeline": timeline, "path": path, "params": definitions["params"], "account": {"name": name, "user_id": verified.get("user_id"), "hycli_account_id": account["id"], "profile": account.get("profile")}})
    pages = output / "04-pages"
    pages.mkdir(exist_ok=True)
    os.chmod(pages, 0o700)
    manifest = {"timeline": timeline, "path": path, "account": "u/" + name, "account_user_id": verified.get("user_id"), "verified_identity": True, "max_posts": args.max_posts, "started_at": now(), "complete": False, "pages": [], "next_cursor": None}
    items = {}
    duplicates = 0
    excluded_promotions = 0
    non_post_entries = 0
    after = None
    seen_cursors = set()
    failure = None
    try:
        while True:
            data = read(after)
            listing = data["data"]
            number = len(manifest["pages"]) + 1
            raw_file = pages / f"page-{number:04d}.json"
            if raw_file.exists():
                raise RuntimeError("Raw page already exists; refusing to overwrite")
            write_json(raw_file, data)
            children = listing.get("children", [])
            page_ids = []
            for child in children:
                if (child.get("data") or {}).get("promoted"):
                    excluded_promotions += 1
                    continue
                item = normalize(child, base_url)
                if item is None:
                    non_post_entries += 1
                    continue
                page_ids.append(item["id"])
                if item["id"] in items:
                    duplicates += 1
                else:
                    items[item["id"]] = item
            next_cursor = listing.get("after")
            manifest["pages"].append({"page": number, "file": str(raw_file.relative_to(output)), "children": len(children), "items": len(page_ids), "unique_items_total": len(items), "has_next_cursor": bool(next_cursor)})
            manifest.update({"unique_items": len(items), "duplicates": duplicates, "excluded_promotions": excluded_promotions, "non_post_entries": non_post_entries, "next_cursor": next_cursor, "updated_at": now()})
            if args.max_posts and len(items) >= args.max_posts:
                manifest["complete"] = True
                manifest["terminal_evidence"] = "max_posts_reached"
            elif not next_cursor:
                manifest["complete"] = True
                manifest["terminal_evidence"] = "no_after_cursor"
            write_json(output / "03-manifest.json", manifest)
            print(json.dumps({"page": number, "items": len(items), "complete": manifest["complete"]}), flush=True)
            if manifest["complete"]:
                break
            if next_cursor == after or next_cursor in seen_cursors:
                raise RuntimeError("Pagination cursor repeated before a verified end")
            seen_cursors.add(next_cursor)
            after = next_cursor
            time.sleep(1)
    except (Exception, KeyboardInterrupt) as error:
        failure = str(error) or "Interrupted"
        manifest["error"] = failure
    if args.max_posts:
        items = dict(list(items.items())[:args.max_posts])
    manifest["finished_at"] = now()
    manifest.update({"unique_items": len(items), "duplicates": duplicates, "excluded_promotions": excluded_promotions, "non_post_entries": non_post_entries})
    write_json(output / "03-manifest.json", manifest)
    write_json(output / f"02-{timeline}.json", {"metadata": manifest, timeline: list(items.values())})
    lines = [f"# u/{name} {spec['title']}", "", f"항목 {len(items)}개 / 원본 응답 {len(manifest['pages'])}페이지", "", "수집 완료" if manifest["complete"] else "수집 미완료", "", f"수집 시각: {manifest['started_at']} - {manifest['finished_at']}", ""]
    for index, item in enumerate(items.values(), 1):
        title = html.escape(item["title"] or "제목 없음", quote=False)
        label = "댓글: " if item["kind"] == "comment" else ""
        lines.extend([f"## {index}. {label}{title} ({item['subreddit'] or '?'})", ""])
        stats = [f"u/{item['author'] or '?'}", f"점수 {item['score']}"] + ([f"댓글 {item['num_comments']}"] if item["num_comments"] is not None else [])
        lines.extend([f"[{item['published_at'] or item['id']}]({item['url']}) / " + " / ".join(stats), ""])
        if item["text"]:
            lines.extend("> " + html.escape(line, quote=False) for line in item["text"].splitlines())
            lines.append("")
        if item["external_url"]:
            lines.extend([f"- {item['external_url']}", ""])
    (output / f"01-{timeline}.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    os.chmod(output / f"01-{timeline}.md", 0o600)
    print(json.dumps({"output": str(output), "items": len(items), "pages": len(manifest["pages"]), "complete": manifest["complete"], "error": failure}, ensure_ascii=False), flush=True)
    return 0 if manifest["complete"] else 1


if __name__ == "__main__":
    sys.exit(main())
