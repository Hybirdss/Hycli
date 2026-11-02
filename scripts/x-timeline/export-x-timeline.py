#!/usr/bin/env python3
"""Export X bookmarks or the home feed through Hycli's verified browser session.

Needs a Hycli build with `hycli accounts read` and a recorded observation of every
GraphQL route in x-graphql.json. See README.md.
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
from datetime import datetime, timezone

HERE = Path(__file__).resolve().parent


def now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def write_json(destination, data):
    temporary = destination.with_name("." + destination.name + ".tmp")
    temporary.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    os.chmod(temporary, 0o600)
    temporary.replace(destination)


def timeline_instructions(data, keys):
    node = data["body"]["data"]
    for key in keys:
        node = node[key]
    return node["instructions"]


def tweet_results(value):
    if isinstance(value, dict):
        result = value.get("tweet_results", {}).get("result")
        if isinstance(result, dict):
            yield result
            return
        for child in value.values():
            yield from tweet_results(child)
    elif isinstance(value, list):
        for child in value:
            yield from tweet_results(child)


def contains_key(value, key):
    if isinstance(value, dict):
        return key in value or any(contains_key(v, key) for v in value.values())
    return isinstance(value, list) and any(contains_key(v, key) for v in value)


def promoted(value):
    if isinstance(value, dict):
        return bool(value.get("promotedMetadata")) or any(promoted(v) for v in value.values())
    return isinstance(value, list) and any(promoted(v) for v in value)


def normalize(result, entry):
    tweet = result.get("tweet", result)
    legacy = tweet.get("legacy", {})
    post_id = tweet.get("rest_id") or legacy.get("id_str")
    if not post_id or tweet.get("__typename") not in (None, "Tweet"):
        return None
    author = tweet.get("core", {}).get("user_results", {}).get("result", {})
    author_core = author.get("core", {})
    author_legacy = author.get("legacy", {})
    handle = author_core.get("screen_name") or author_legacy.get("screen_name")
    display = author_core.get("name") or author_legacy.get("name")
    note = tweet.get("note_tweet", {}).get("note_tweet_results", {}).get("result", {})
    note_text = note.get("text")
    text = note_text or legacy.get("full_text") or tweet.get("text") or ""
    created = legacy.get("created_at")
    try:
        created = datetime.strptime(created, "%a %b %d %H:%M:%S %z %Y").isoformat()
    except (ValueError, TypeError):
        pass
    entity_urls = legacy.get("entities", {}).get("urls", [])
    media = legacy.get("extended_entities", {}).get("media", [])
    return {
        "id": str(post_id),
        "text": text,
        "text_source": "note_tweet.text" if note_text else "legacy.full_text",
        "text_truncated": bool(legacy.get("truncated")) and not bool(note_text),
        "author_name": display,
        "author_handle": handle,
        "published_at": created,
        "url": f"https://x.com/{handle}/status/{post_id}" if handle else f"https://x.com/i/web/status/{post_id}",
        "external_urls": list(dict.fromkeys(u.get("expanded_url") or u.get("url") for u in entity_urls if u.get("expanded_url") or u.get("url"))),
        "media": [{"type": m.get("type"), "url": m.get("media_url_https"), "alt_text": m.get("ext_alt_text"), "post_url": m.get("expanded_url")} for m in media],
        "entry_id": entry.get("entryId"),
        "sort_index": entry.get("sortIndex"),
    }


def resolve_account(accounts, base_url, handle, account_id):
    candidates = [a for a in accounts if a.get("site_url") == base_url and a.get("verified_identity") and a.get("status") == "connected"]
    if account_id:
        candidates = [a for a in candidates if a.get("id") == account_id]
    if handle:
        candidates = [a for a in candidates if (a.get("name") or "").lower() == handle.lower()]
    if not candidates:
        raise SystemExit(f"No verified, connected Hycli account matches {'@' + handle if handle else account_id} on {base_url}. Check `hycli accounts`.")
    # The same X user can be connected from several browser profiles; use the most recently verified one.
    return max(candidates, key=lambda a: a.get("checked_at") or "")


def observation_for(verified, spec):
    observations = [
        item
        for item in verified.get("read_observations", [])
        if item.get("method") == "POST"
        and item.get("path") == spec["path"]
        and 200 <= item.get("status", 0) < 300
    ]
    if not observations:
        raise RuntimeError(
            f"No successful POST observation for {spec['path']}; run browser observation first."
        )
    return observations[0]


def request_descriptor(definitions, spec, variables, observation):
    body = {
        "variables": variables,
        "features": definitions["features"],
    }
    if spec.get("field_toggles"):
        body["fieldToggles"] = spec["field_toggles"]
    observed_body = observation.get("body", {})
    if "query" in observed_body:
        body["query"] = None
    if "operationName" in observed_body:
        body["operationName"] = spec["path"].rstrip("/").rsplit("/", 1)[-1]
    if "extensions" in observed_body:
        body["extensions"] = {}
    descriptor = {
        "version": 1,
        "request": {
            "method": "POST",
            "url": definitions["base_url"] + spec["path"],
            "json": body,
        },
        "evidence": {"observation": observation["reference"]},
    }
    if spec.get("cursor_pointer"):
        descriptor["pagination"] = {
            "variable": "cursor",
            "next": spec["cursor_pointer"],
        }
    return descriptor


def main():

    definitions_default = HERE / "x-graphql.json"
    definitions = json.loads(definitions_default.read_text(encoding="utf-8"))
    parser = argparse.ArgumentParser(description="Export X bookmarks or the home feed through Hycli.")
    parser.add_argument("--account", help="X handle of a verified Hycli account, without @")
    parser.add_argument("--account-id", help="Hycli account id, when the handle alone is not enough")
    parser.add_argument("--timeline", required=True, choices=sorted(definitions["timelines"]))
    parser.add_argument("--max-posts", type=int, help="Stop once this many unique posts are collected (required for endless feeds)")
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

    def call(arguments):
        while True:
            result = subprocess.run([exe] + arguments, capture_output=True, text=True, timeout=90)
            if result.returncode == 0:
                return json.loads(result.stdout)
            if re.search(r"\bsite_rate_limited\b", result.stderr):
                # X itself answered 429: stop instead of knocking again
                raise RuntimeError("X returned a rate limit (429); stopped without retrying. Try again later.")
            if re.search(r"\brate_limited\b", result.stderr):
                # Hycli's own request budget refused locally; nothing reached X
                print(json.dumps({"status": "waiting_for_local_rate_limit", "at": now()}), flush=True)
                time.sleep(args.local_wait)
                continue
            raise RuntimeError(f"Hycli exit {result.returncode}: {result.stderr.strip()}")

    account = resolve_account(call(["accounts"]), definitions["base_url"], (args.account or "").lstrip("@"), args.account_id)
    saved = call(["accounts", "inspect", account["id"]]).get("saved_recipe")
    if not saved:
        raise RuntimeError("No verified current-account recipe is saved")
    with tempfile.TemporaryDirectory(prefix="hycli-x-identity-") as temporary:
        recipe_file = Path(temporary) / "recipe.json"
        write_json(recipe_file, saved)
        verified = call(["accounts", "verify", account["id"], "--recipe", str(recipe_file)])
    if not verified.get("verified_identity") or verified.get("user_id") != account.get("user_id") or verified.get("name") != account.get("name"):
        raise RuntimeError("The verified account does not match the requested account")
    handle = verified["name"]
    observation = observation_for(verified, spec)
    request_directory = tempfile.TemporaryDirectory(prefix="hycli-x-request-")
    request_root = Path(request_directory.name)
    request_number = 0

    def read_page(cursor_handle=None):
        nonlocal request_number
        request_number += 1
        descriptor_file = request_root / f"request-{request_number:04d}.json"
        write_json(
            descriptor_file,
            request_descriptor(definitions, spec, spec["variables"], observation),
        )
        command = ["accounts", "read", account["id"], "--request", str(descriptor_file)]
        if cursor_handle:
            command.extend(["--cursor", cursor_handle])
        return call(command)

    if args.check:
        data = read_page()
        entries = [
            e
            for i in timeline_instructions(data, spec["response_keys"])
            for e in i.get("entries", [])
        ]
        print(json.dumps({"account": "@" + handle, "timeline": timeline, "verified_identity": True, "posts_on_first_page": sum(len(list(tweet_results(e.get("content", {})))) for e in entries), "has_next_cursor": bool(data.get("next_cursor"))}))
        request_directory.cleanup()
        return 0
    if args.output:
        output = args.output
        output.mkdir(parents=True, exist_ok=True)
    else:
        stamp = datetime.now().astimezone().strftime("%Y-%m-%d-%H%M%S")
        output = args.output_base / f"x-{timeline}-{handle}-{stamp}"
        output.mkdir(parents=True, exist_ok=False)
    os.chmod(output, 0o700)
    if (output / "03-manifest.json").exists():
        raise RuntimeError("An export already exists here; select a new output directory")
    shutil.copy2(__file__, output / "05-export.py")
    write_json(
        output / "06-request.json",
        request_descriptor(definitions, spec, spec["variables"], observation),
    )
    pages = output / "04-pages"
    pages.mkdir(exist_ok=True)
    os.chmod(pages, 0o700)
    manifest = {"timeline": timeline, "query_path": spec["path"], "account": "@" + handle, "account_user_id": account["user_id"], "verified_identity": True, "max_posts": args.max_posts, "started_at": now(), "complete": False, "pages": [], "next_cursor": None}
    posts = {}
    unavailable = []
    duplicates = 0
    excluded_promotions = 0
    non_post_entries = 0
    cursor = None
    seen_cursors = set()
    stop_on_empty = False
    failure = None
    try:
        while True:
            data = read_page(cursor)
            instructions = timeline_instructions(data, spec["response_keys"])
            number = len(manifest["pages"]) + 1
            raw_file = pages / f"page-{number:04d}.json"
            if raw_file.exists():
                raise RuntimeError("Raw page already exists; refusing to overwrite")
            write_json(raw_file, data)
            entries = [entry for instruction in instructions for entry in instruction.get("entries", [])]
            next_cursor = data.get("next_cursor")
            next_stop = False
            item_count = 0
            page_ids = []
            for entry in entries:
                content = entry.get("content", {})
                if content.get("cursorType") == "Bottom":
                    next_stop = bool(content.get("stopOnEmptyResponse"))
                    continue
                if content.get("entryType") == "TimelineTimelineCursor":
                    continue
                item_count += 1
                if promoted(content):
                    excluded_promotions += 1
                    continue
                if not contains_key(content, "tweet_results"):
                    # home feed modules such as follow suggestions carry no post
                    non_post_entries += 1
                    continue
                results = list(tweet_results(content))
                if not results:
                    unavailable.append({"entry_id": entry.get("entryId"), "reason": "No readable tweet result", "page": number})
                for result in results:
                    post = normalize(result, entry)
                    if post is None:
                        unavailable.append({"entry_id": entry.get("entryId"), "reason": result.get("reason") or result.get("__typename") or "Unavailable result", "page": number})
                        continue
                    post_id = post["id"]
                    page_ids.append(post_id)
                    if post_id in posts:
                        duplicates += 1
                        if len(post["text"]) > len(posts[post_id]["text"]):
                            posts[post_id] = post
                    else:
                        posts[post_id] = post
            termination = any(i.get("type") == "TimelineTerminateTimeline" and i.get("direction") == "Bottom" for i in instructions)
            manifest["pages"].append({"page": number, "file": str(raw_file.relative_to(output)), "item_entries": item_count, "post_results": len(page_ids), "unique_posts_total": len(posts), "has_next_cursor": bool(next_cursor), "instruction_types": [i.get("type") for i in instructions]})
            manifest.update({"unique_posts": len(posts), "unavailable_entries": len(unavailable), "duplicates": duplicates, "excluded_promotions": excluded_promotions, "non_post_entries": non_post_entries, "next_cursor": next_cursor, "updated_at": now()})
            if args.max_posts and len(posts) >= args.max_posts:
                manifest["complete"] = True
                manifest["terminal_evidence"] = "max_posts_reached"
            elif termination or not next_cursor or (item_count == 0 and stop_on_empty):
                manifest["complete"] = True
                manifest["terminal_evidence"] = "terminate_bottom" if termination else "no_bottom_cursor" if not next_cursor else "empty_response_with_stop_on_empty_response"
                manifest["next_cursor"] = None
            write_json(output / "03-manifest.json", manifest)
            print(json.dumps({"page": number, "posts": len(posts), "unavailable": len(unavailable), "complete": manifest["complete"]}), flush=True)
            if manifest["complete"]:
                break
            if next_cursor == cursor or next_cursor in seen_cursors:
                raise RuntimeError("Pagination cursor repeated before a verified end")
            seen_cursors.add(next_cursor)
            cursor, stop_on_empty = next_cursor, next_stop
            time.sleep(1)
    except (Exception, KeyboardInterrupt) as error:
        failure = str(error) or "Interrupted"
        manifest["error"] = failure
    if args.max_posts:
        posts = dict(list(posts.items())[:args.max_posts])
    manifest["finished_at"] = now()
    manifest.update({"unique_posts": len(posts), "unavailable_entries": len(unavailable), "duplicates": duplicates, "excluded_promotions": excluded_promotions, "non_post_entries": non_post_entries})
    write_json(output / "03-manifest.json", manifest)
    write_json(output / f"02-{timeline}.json", {"metadata": manifest, timeline: list(posts.values()), "unavailable": unavailable})
    lines = [f"# @{handle} {spec['title']}", "", f"게시물 {len(posts)}개 / 원본 응답 {len(manifest['pages'])}페이지", "", "수집 완료" if manifest["complete"] else "수집 미완료", "", f"수집 시각: {manifest['started_at']} - {manifest['finished_at']}", ""]
    if unavailable:
        lines.extend([f"읽을 수 없는 항목: {len(unavailable)}개. JSON과 원본 응답에 기록했습니다.", ""])
    for index, post in enumerate(posts.values(), 1):
        name = html.escape(post["author_name"] or post["author_handle"] or "작성자 정보 없음", quote=False)
        lines.extend([f"## {index}. {name} (@{post['author_handle'] or '?'})", "", f"[{post['published_at'] or post['id']}]({post['url']})", ""])
        lines.extend("> " + html.escape(line, quote=False) for line in (post["text"] or "본문이 응답에 없습니다.").splitlines())
        lines.append("")
        for link in post["external_urls"]:
            lines.extend([f"- {link}", ""])
    (output / f"01-{timeline}.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    os.chmod(output / f"01-{timeline}.md", 0o600)
    request_directory.cleanup()
    print(json.dumps({"output": str(output), "posts": len(posts), "pages": len(manifest["pages"]), "complete": manifest["complete"], "error": failure}, ensure_ascii=False), flush=True)
    return 0 if manifest["complete"] else 1


if __name__ == "__main__":
    sys.exit(main())
