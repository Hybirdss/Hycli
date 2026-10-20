#!/usr/bin/env python3
"""Exercise export-reddit-timeline.py against a fake hycli; nothing reaches Reddit."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest
import urllib.parse

EXPORTER = Path(__file__).resolve().parent / "export-reddit-timeline.py"

FAKE_HYCLI = textwrap.dedent('''\
    #!/usr/bin/env python3
    import json, os, sys, urllib.parse
    args = sys.argv[1:]
    log_path = os.environ["FAKE_LOG"]
    with open(log_path) as log:
        history = [json.loads(line) for line in log]
    with open(log_path, "a") as log:
        log.write(json.dumps(args) + "\\n")
    accounts = json.loads(os.environ["FAKE_ACCOUNTS"])
    identities = json.loads(os.environ["FAKE_IDENTITIES"])
    observed = {urllib.parse.urlparse(c[3]).path for c in history if c[:2] == ["accounts", "observe"]}
    observed |= set(json.loads(os.environ["FAKE_OBSERVED"]))

    def need(url):
        if urllib.parse.urlparse(url).path not in observed:
            sys.stderr.write("hycli: code=1 unsupported_evidence\\n")
            sys.exit(1)

    if args == ["accounts"]:
        print(json.dumps(accounts))
    elif args[:2] == ["accounts", "inspect"]:
        print(json.dumps({"saved_recipe": None}))
    elif args[:2] == ["accounts", "observe"]:
        print(json.dumps({"observation": {"status": 200, "successful": True}}))
    elif args[:2] == ["accounts", "verify"]:
        recipe = json.load(open(args[4]))
        need(recipe["identity_url"])
        name = identities.get(args[2])
        if name is None:
            sys.stderr.write("hycli: code=3 auth_required\\n")
            sys.exit(3)
        print(json.dumps({"verified_identity": True, "user_id": "id-" + name, "name": name}))
    elif args[:2] == ["accounts", "read"]:
        need(args[3])
        if urllib.parse.urlparse(args[3]).path == "/api/me.json":
            name = identities[args[2]]
            print(json.dumps({"data": {"name": name, "id": "id-" + name}}))
            sys.exit(0)
        reads = sum(1 for c in history if c[:2] == ["accounts", "read"] and urllib.parse.urlparse(c[3]).path != "/api/me.json") + 1
        plan = json.loads(os.environ["FAKE_PLAN"])
        step = plan[min(reads, len(plan)) - 1]
        if step in ("site_rate_limited", "rate_limited"):
            sys.stderr.write("hycli: code=1 " + step + "\\n")
            sys.exit(1)
        print(json.dumps(step))
''')


def account(account_id, profile, name=None, checked_at=""):
    return {"id": account_id, "site_url": "https://www.reddit.com", "profile": profile, "name": name, "user_id": "id-" + name if name else None, "verified_identity": bool(name), "status": "connected", "checked_at": checked_at}


def post(post_id, **extra):
    data = {"name": "t3_" + post_id, "title": "title " + post_id, "selftext": "", "author": "someone", "subreddit": "python", "subreddit_name_prefixed": "r/python", "permalink": f"/r/python/comments/{post_id}/x/", "url": "https://example.com/" + post_id, "created_utc": 1792400000, "score": 5, "num_comments": 2}
    data.update(extra)
    return {"kind": "t3", "data": data}


def comment(comment_id):
    return {"kind": "t1", "data": {"name": "t1_" + comment_id, "body": "comment body", "link_title": "parent post", "author": "someone", "subreddit": "python", "permalink": f"/r/python/comments/abc/x/{comment_id}/", "created_utc": 1792400000, "score": 3}}


def listing(children, after=None):
    return {"kind": "Listing", "data": {"after": after, "children": children}}


class ExportTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="reddit-timeline-test-")
        self.root = Path(self.temporary.name)
        self.hycli = self.root / "hycli"
        self.hycli.write_text(FAKE_HYCLI)
        self.hycli.chmod(0o700)
        self.log = self.root / "calls.log"
        self.log.touch()
        self.accounts = [account("verified", "Personal", "alice", "2026-10-19T00:00:00Z"), account("fresh-1", "jax"), account("fresh-2", "work")]
        self.identities = {"verified": "alice", "fresh-2": "bob"}
        self.observed = ["/api/me.json", "/.json", "/user/alice/saved.json"]

    def tearDown(self):
        self.temporary.cleanup()

    def run_export(self, plan, *extra):
        env = dict(os.environ, FAKE_LOG=str(self.log), FAKE_ACCOUNTS=json.dumps(self.accounts), FAKE_IDENTITIES=json.dumps(self.identities), FAKE_OBSERVED=json.dumps(self.observed), FAKE_PLAN=json.dumps(plan))
        args = [sys.executable, "-I", str(EXPORTER), "--hycli", str(self.hycli), "--output", str(self.root / "out"), "--local-wait", "0", *extra]
        return subprocess.run(args, env=env, capture_output=True, text=True, timeout=60)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def reads(self):
        return [c for c in self.calls() if c[:2] == ["accounts", "read"] and urllib.parse.urlparse(c[3]).path != "/api/me.json"]

    def read_query(self, index):
        return urllib.parse.parse_qs(urllib.parse.urlparse(self.reads()[index][3]).query, keep_blank_values=True)

    def manifest(self):
        return json.loads((self.root / "out" / "03-manifest.json").read_text())

    def test_stops_on_reddit_rate_limit_without_retrying(self):
        result = self.run_export(["site_rate_limited", listing([post("a")])], "--account", "alice", "--timeline", "home", "--max-posts", "10")
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(len(self.reads()), 1)
        self.assertIn("429", self.manifest()["error"])

    def test_waits_when_hycli_budget_refuses_locally(self):
        result = self.run_export(["rate_limited", listing([post("a")])], "--account", "alice", "--timeline", "home", "--max-posts", "10")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.reads()), 2)

    def test_pages_with_fixed_query_keys_and_skips_promotions(self):
        first = listing([post("a"), post("ad", promoted=True), {"kind": "more", "data": {}}, post("b")], after="t3_b")
        second = listing([post("c"), post("d")], after="t3_d")
        result = self.run_export([first, second], "--account", "alice", "--timeline", "home", "--max-posts", "3")
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = self.manifest()
        self.assertEqual((manifest["unique_items"], manifest["terminal_evidence"]), (3, "max_posts_reached"))
        self.assertEqual((manifest["excluded_promotions"], manifest["non_post_entries"]), (1, 1))
        # hycli only reads a route whose query keys match the observed request, so `after` is always sent
        self.assertEqual(self.read_query(0)["after"], [""])
        self.assertEqual(self.read_query(1)["after"], ["t3_b"])
        items = json.loads((self.root / "out" / "02-home.json").read_text())["home"]
        self.assertEqual([i["id"] for i in items], ["t3_a", "t3_b", "t3_c"])
        self.assertEqual(items[0]["external_url"], "https://example.com/a")

    def test_saved_records_new_route_and_keeps_comments(self):
        self.observed = ["/api/me.json"]
        result = self.run_export([listing([post("a"), comment("c1")])], "--account", "alice", "--timeline", "saved")
        self.assertEqual(result.returncode, 0, result.stderr)
        observes = [c for c in self.calls() if c[:2] == ["accounts", "observe"]]
        self.assertEqual([urllib.parse.urlparse(c[3]).path for c in observes], ["/user/alice/saved.json"])
        items = json.loads((self.root / "out" / "02-saved.json").read_text())["saved"]
        self.assertEqual([(i["kind"], i["title"]) for i in items], [("post", "title a"), ("comment", "parent post")])
        self.assertEqual(self.manifest()["terminal_evidence"], "no_after_cursor")

    def test_verifies_unverified_profiles_until_user_is_found(self):
        result = self.run_export([listing([post("a")])], "--account", "u/bob", "--timeline", "home", "--max-posts", "1")
        self.assertEqual(result.returncode, 0, result.stderr)
        verified = [c[2] for c in self.calls() if c[:2] == ["accounts", "verify"]]
        self.assertEqual(verified, ["fresh-1", "fresh-2"])
        self.assertEqual({r[2] for r in self.reads()}, {"fresh-2"})
        self.assertIn("profile_not_verified", result.stdout)

    def test_verified_account_skips_cookie_less_baseline(self):
        result = self.run_export([listing([post("a")])], "--account", "alice", "--timeline", "home", "--max-posts", "1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([c for c in self.calls() if c[:2] == ["accounts", "verify"]], [])
        identity_reads = [c for c in self.calls() if c[:2] == ["accounts", "read"] and "/api/me.json" in c[3]]
        self.assertEqual([c[2] for c in identity_reads], ["verified"])

    def test_home_requires_max_posts(self):
        result = self.run_export([], "--account", "alice", "--timeline", "home")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(self.calls(), [])


if __name__ == "__main__":
    unittest.main()
