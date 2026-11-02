#!/usr/bin/env python3
"""Exercise export-x-timeline.py against a fake hycli; nothing reaches X."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest

EXPORTER = Path(__file__).resolve().parent / "export-x-timeline.py"

FAKE_HYCLI = textwrap.dedent('''\
    #!/usr/bin/env python3
    import hashlib, json, os, sys
    args = sys.argv[1:]
    with open(os.environ["FAKE_LOG"], "a") as log:
        log.write(json.dumps(args) + "\\n")
    accounts = json.loads(os.environ["FAKE_ACCOUNTS"])
    if args == ["accounts"]:
        print(json.dumps(accounts))
    elif args[:2] == ["accounts", "inspect"]:
        print(json.dumps({"saved_recipe": {"identity_url": "fake"}}))
    elif args[:2] == ["accounts", "verify"]:
        account = next(a for a in accounts if a["id"] == args[2])
        path = {"bookmarks":"/i/api/graphql/VjWsfItbiMLeajpZ90NJeg/Bookmarks", "home":"/i/api/graphql/svLglq6U2wlTyZMGDDcLYQ/HomeTimeline", "following":"/i/api/graphql/NmDBT3APtNxDpMqIEauu7A/HomeLatestTimeline"}
        refs = [{"reference":"ro_" + "0" * 32, "method":"POST", "path": value, "query":{}, "body":{"features":"object","variables":"object","fieldToggles":"object"}, "status":200} for value in path.values()]
        print(json.dumps({"verified_identity": True, "user_id": account["user_id"], "name": account["name"], "read_observations": refs}))
    elif args[:2] == ["accounts", "read"]:
        account = next(a for a in accounts if a["id"] == args[2])
        request_index = args.index("--request")
        descriptor = json.load(open(args[request_index + 1]))
        cursor = args[args.index("--cursor") + 1] if "--cursor" in args else None
        with open(os.environ["FAKE_DESCRIPTOR_LOG"], "a") as log:
            log.write(json.dumps({"descriptor": descriptor, "cursor": cursor}) + "\\n")
        with open(os.environ["FAKE_LOG"]) as log:
            reads = sum(1 for line in log if json.loads(line)[:2] == ["accounts", "read"])
        plan = json.loads(os.environ["FAKE_PLAN"])
        step = plan[min(reads, len(plan)) - 1]
        if step == "site_rate_limited":
            sys.stderr.write("hycli: code=1 site_rate_limited\\n")
            sys.exit(1)
        if step == "rate_limited":
            sys.stderr.write("hycli: code=4 rate_limited\\n")
            sys.exit(4)
        next_cursor = [None]
        def scrub(value):
            if isinstance(value, dict):
                if value.get("cursorType") == "Bottom" and isinstance(value.get("value"), str):
                    next_cursor[0] = "rc_" + hashlib.sha256(value["value"].encode()).hexdigest()
                    value["value"] = None
                for child in value.values():
                    scrub(child)
            elif isinstance(value, list):
                for child in value:
                    scrub(child)
        scrub(step)
        print(json.dumps({"account_id": account["id"], "status": 200, "body": step, "next_cursor": next_cursor[0]}))
''')


def account(account_id, name, checked_at):
    return {"id": account_id, "site_url": "https://x.com", "name": name, "user_id": "u-" + name, "verified_identity": True, "status": "connected", "checked_at": checked_at}


def tweet(post_id, promoted=False):
    item = {"tweet_results": {"result": {"__typename": "Tweet", "rest_id": post_id, "core": {"user_results": {"result": {"core": {"screen_name": "author", "name": "Author"}}}}, "legacy": {"full_text": "post " + post_id, "created_at": "Mon Oct 19 10:00:00 +0000 2026"}}}}
    if promoted:
        item["promotedMetadata"] = {"advertiser": "ad"}
    return {"entryId": ("promoted-tweet-" if promoted else "tweet-") + post_id, "content": {"entryType": "TimelineTimelineItem", "itemContent": item}}


FOLLOW_SUGGESTION = {"entryId": "who-to-follow-1", "content": {"entryType": "TimelineTimelineModule", "items": [{"item": {"itemContent": {"user_results": {"result": {"__typename": "User"}}}}}]}}


def home_page(entries, cursor=None):
    if cursor:
        entries = entries + [{"entryId": "cursor-bottom-" + cursor, "content": {"entryType": "TimelineTimelineCursor", "cursorType": "Bottom", "value": cursor}}]
    return {"data": {"home": {"home_timeline_urt": {"instructions": [{"type": "TimelineAddEntries", "entries": entries}]}}}}


class ExportTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="x-timeline-test-")
        self.root = Path(self.temporary.name)
        self.hycli = self.root / "hycli"
        self.hycli.write_text(FAKE_HYCLI)
        self.hycli.chmod(0o700)
        self.log = self.root / "calls.log"
        self.descriptor_log = self.root / "descriptors.log"
        self.log.touch()
        self.descriptor_log.touch()
        self.accounts = [account("old", "alice", "2026-10-18T00:00:00Z"), account("new", "alice", "2026-10-19T00:00:00Z"), account("bob-id", "bob", "2026-10-19T00:00:00Z")]

    def tearDown(self):
        self.temporary.cleanup()

    def run_export(self, plan, *extra):
        env = dict(os.environ, FAKE_LOG=str(self.log), FAKE_DESCRIPTOR_LOG=str(self.descriptor_log), FAKE_ACCOUNTS=json.dumps(self.accounts), FAKE_PLAN=json.dumps(plan))
        args = [sys.executable, "-I", str(EXPORTER), "--hycli", str(self.hycli), "--output", str(self.root / "out"), "--local-wait", "0", *extra]
        return subprocess.run(args, env=env, capture_output=True, text=True, timeout=60)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def reads(self):
        return [c for c in self.calls() if c[:2] == ["accounts", "read"]]

    def descriptors(self):
        return [json.loads(line) for line in self.descriptor_log.read_text().splitlines()]

    def manifest(self):
        return json.loads((self.root / "out" / "03-manifest.json").read_text())

    def test_stops_on_x_rate_limit_without_retrying(self):
        result = self.run_export(["site_rate_limited", home_page([tweet("1")])], "--account", "alice", "--timeline", "home", "--max-posts", "10")
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(len(self.reads()), 1)
        self.assertIn("429", self.manifest()["error"])
        self.assertFalse(self.manifest()["complete"])

    def test_waits_when_hycli_budget_refuses_locally(self):
        result = self.run_export(["rate_limited", home_page([tweet("1")])], "--account", "alice", "--timeline", "home", "--max-posts", "10")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.reads()), 2)
        self.assertIn("waiting_for_local_rate_limit", result.stdout)

    def test_max_posts_trims_and_skips_ads_and_suggestions(self):
        first = home_page([tweet("1"), tweet("9", promoted=True), FOLLOW_SUGGESTION, tweet("2"), tweet("3")], cursor="c1")
        second = home_page([tweet("4"), tweet("5"), tweet("6")], cursor="c2")
        result = self.run_export([first, second], "--account", "alice", "--timeline", "home", "--max-posts", "4")
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = self.manifest()
        self.assertEqual((manifest["unique_posts"], manifest["terminal_evidence"]), (4, "max_posts_reached"))
        self.assertEqual((manifest["excluded_promotions"], manifest["non_post_entries"]), (1, 1))
        posts = json.loads((self.root / "out" / "02-home.json").read_text())["home"]
        self.assertEqual([p["id"] for p in posts], ["1", "2", "3", "4"])
        self.assertIsNone(self.descriptors()[0]["descriptor"]["request"]["json"]["variables"].get("cursor"))
        self.assertEqual(self.descriptors()[1]["cursor"], "rc_" + __import__("hashlib").sha256(b"c1").hexdigest())
        self.assertEqual(sorted(self.descriptors()[0]["descriptor"]["request"]["json"]), ["features", "fieldToggles", "variables"])

    def test_uses_most_recently_verified_account_for_handle(self):
        result = self.run_export([home_page([tweet("1")])], "--account", "@Alice", "--timeline", "home", "--max-posts", "1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual({c[2] for c in self.calls() if len(c) > 2}, {"new"})

    def test_bookmarks_omit_field_toggles(self):
        page = {"data": {"bookmark_timeline_v2": {"timeline": {"instructions": [{"type": "TimelineAddEntries", "entries": [tweet("1")]}]}}}}
        result = self.run_export([page], "--account", "bob", "--timeline", "bookmarks")
        self.assertEqual(result.returncode, 0, result.stderr)
        descriptor = self.descriptors()[0]["descriptor"]
        self.assertEqual(descriptor["request"]["method"], "POST")
        self.assertNotIn("?", descriptor["request"]["url"])
        self.assertEqual(sorted(descriptor["request"]["json"]), ["features", "variables"])
        self.assertTrue(descriptor["evidence"]["observation"].startswith("ro_"))

    def test_endless_feed_requires_max_posts(self):
        result = self.run_export([], "--account", "alice", "--timeline", "following")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(self.calls(), [])


if __name__ == "__main__":
    unittest.main()
