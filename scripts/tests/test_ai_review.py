import json
import importlib.util
import os
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

_MODULE_PATH = Path(__file__).resolve().parent.parent / "ai-review.py"
_spec = importlib.util.spec_from_file_location("ai_review", _MODULE_PATH)
ai_review = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ai_review)


FAKE_KEY = "fake-test-key-not-a-secret"


def make_cfg(diff_text="", pr_number="42", title="feat: test", body="test body"):
    tmp = tempfile.TemporaryDirectory()
    diff_path = Path(tmp.name) / "diff.txt"
    diff_path.write_text(diff_text)
    return SimpleNamespace(
        base="unused",
        head="unused",
        pr_number=pr_number,
        diff_file=str(diff_path),
        files_file="",
        context_dir=tmp.name,
        output="",
        title=title,
        body=body,
        dry_run=False,
        endpoint="https://example.invalid/inference",
        model="test-model",
        api_key=FAKE_KEY,
    ), tmp


def fake_model(verdict_json):
    def caller(endpoint, api_key, model, messages):
        return {"choices": [{"message": {"content": verdict_json}}]}

    return caller


class ExtractJsonTests(unittest.TestCase):
    def test_fenced_json(self):
        text = '```json\n{"verdict": "approve"}\n```'
        self.assertEqual(ai_review.extract_json(text), {"verdict": "approve"})

    def test_json_embedded_in_text(self):
        text = 'Sure! Here is the result:\n{"verdict": "approve", "x": {"y": [1, 2]}}\nDone.'
        self.assertEqual(
            ai_review.extract_json(text),
            {"verdict": "approve", "x": {"y": [1, 2]}},
        )

    def test_missing_json_raises(self):
        with self.assertRaises(ai_review.ParseError):
            ai_review.extract_json("no json here")

    def test_unbalanced_json_raises(self):
        with self.assertRaises(ai_review.ParseError):
            ai_review.extract_json('{"verdict": "approve"')

    def test_missing_content_raises(self):
        with self.assertRaises(ai_review.ParseError):
            ai_review.extract_json(None)


class VerdictBlockingTests(unittest.TestCase):
    def test_approve_not_blocking(self):
        self.assertFalse(
            ai_review.verdict_blocking(
                {"verdict": "approve", "blockers": [], "acceptance": {}}
            )
        )

    def test_request_changes_with_blockers_blocking(self):
        self.assertTrue(
            ai_review.verdict_blocking(
                {"verdict": "request_changes", "blockers": ["src/main.rs:1 bug"]}
            )
        )

    def test_request_changes_without_blockers_not_blocking(self):
        self.assertFalse(
            ai_review.verdict_blocking({"verdict": "request_changes", "blockers": []})
        )

    def test_acceptance_gap_for_touched_item_blocking(self):
        verdict = {
            "verdict": "approve",
            "blockers": [],
            "acceptance": {"touched": ["A-16"], "gaps": ["A-16 evidence missing"]},
        }
        self.assertTrue(ai_review.verdict_blocking(verdict))

    def test_gap_without_touched_not_blocking(self):
        verdict = {
            "verdict": "approve",
            "blockers": [],
            "acceptance": {"touched": [], "gaps": ["A-16 evidence missing"]},
        }
        self.assertFalse(ai_review.verdict_blocking(verdict))


class TruncateTests(unittest.TestCase):
    def test_short_text_untouched(self):
        text, truncated = ai_review.truncate("abc", 10)
        self.assertEqual(text, "abc")
        self.assertFalse(truncated)

    def test_long_text_marked(self):
        text, truncated = ai_review.truncate("a" * 100, 10)
        self.assertTrue(truncated)
        self.assertLess(len(text), 100)
        self.assertIn("truncated", text)


class RunReviewTests(unittest.TestCase):
    def test_approve_exits_zero(self):
        cfg, tmp = make_cfg(diff_text="@@ -1 +1 @@\n-old\n+new\n")
        self.addCleanup(tmp.cleanup)
        caller = fake_model(json.dumps({"verdict": "approve", "blockers": [], "acceptance": {}}))
        code = ai_review.run_review(cfg, model_caller=caller)
        self.assertEqual(code, 0)

    def test_blocking_exits_one(self):
        cfg, tmp = make_cfg(diff_text="@@ -1 +1 @@\n-old\n+new\n")
        self.addCleanup(tmp.cleanup)
        verdict = {
            "verdict": "request_changes",
            "blockers": ["hardcoded credential in src/auth.rs:3"],
        }
        code = ai_review.run_review(cfg, model_caller=fake_model(json.dumps(verdict)))
        self.assertEqual(code, 1)

    def test_infra_error_exits_two(self):
        cfg, tmp = make_cfg(diff_text="@@ -1 +1 @@\n-old\n+new\n")
        self.addCleanup(tmp.cleanup)

        def failing_caller(endpoint, api_key, model, messages):
            raise ai_review.RateLimitError("HTTP 429: rate limited")

        code = ai_review.run_review(cfg, model_caller=failing_caller)
        self.assertEqual(code, 2)

    def test_unparseable_output_exits_two(self):
        cfg, tmp = make_cfg(diff_text="@@ -1 +1 @@\n-old\n+new\n")
        self.addCleanup(tmp.cleanup)
        code = ai_review.run_review(
            cfg, model_caller=fake_model("I have no JSON for you")
        )
        self.assertEqual(code, 2)

    def test_unexpected_response_shape_exits_two(self):
        cfg, tmp = make_cfg(diff_text="@@ -1 +1 @@\n-old\n+new\n")
        self.addCleanup(tmp.cleanup)
        code = ai_review.run_review(
            cfg, model_caller=lambda endpoint, api_key, model, messages: {"choices": []}
        )
        self.assertEqual(code, 2)

    def test_api_key_never_in_output(self):
        cfg, tmp = make_cfg(diff_text="@@ -1 +1 @@\n-old\n+new\n")
        self.addCleanup(tmp.cleanup)
        report_path = Path(tmp.name) / "report.md"
        cfg.output = str(report_path)
        verdict = {"verdict": "approve", "blockers": [], "acceptance": {}}
        code = ai_review.run_review(cfg, model_caller=fake_model(json.dumps(verdict)))
        self.assertEqual(code, 0)
        self.assertNotIn(FAKE_KEY, report_path.read_text())

    def test_dry_run_never_calls_model(self):
        cfg, tmp = make_cfg(diff_text="@@ -1 +1 @@\n-old\n+new\n")
        self.addCleanup(tmp.cleanup)
        cfg.dry_run = True

        def should_not_run(endpoint, api_key, model, messages):
            raise AssertionError("model must not be called in dry-run")

        code = ai_review.run_review(cfg, model_caller=should_not_run)
        self.assertEqual(code, 0)

    def test_files_file_is_included_in_model_prompt(self):
        cfg, tmp = make_cfg(diff_text="@@ -1 +1 @@\n-old\n+new\n")
        self.addCleanup(tmp.cleanup)
        files_path = Path(tmp.name) / "files.txt"
        files_path.write_text("src/main.rs\nsrc/lib.rs\n")
        cfg.files_file = str(files_path)
        seen = {}

        def caller(endpoint, api_key, model, messages):
            seen["prompt"] = messages[1]["content"]
            return {
                "choices": [
                    {
                        "message": {
                            "content": '{"verdict":"approve","blockers":[],"acceptance":{}}'
                        }
                    }
                ]
            }

        code = ai_review.run_review(cfg, model_caller=caller)
        self.assertEqual(code, 0)
        self.assertIn("Changed files (2):", seen["prompt"])
        self.assertIn("- src/main.rs", seen["prompt"])

    def test_unconfigured_model_skips_fail_open(self):
        cfg, tmp = make_cfg(diff_text="@@ -1 +1 @@\n-old\n+new\n")
        self.addCleanup(tmp.cleanup)
        cfg.endpoint = ""
        cfg.api_key = ""

        def should_not_run(endpoint, api_key, model, messages):
            raise AssertionError("model must not be called without configuration")

        code = ai_review.run_review(cfg, model_caller=should_not_run)
        self.assertEqual(code, 2)

    def test_dry_run_works_without_configuration(self):
        cfg, tmp = make_cfg(diff_text="@@ -1 +1 @@\n-old\n+new\n")
        self.addCleanup(tmp.cleanup)
        cfg.endpoint = ""
        cfg.api_key = ""
        cfg.dry_run = True

        def should_not_run(endpoint, api_key, model, messages):
            raise AssertionError("model must not be called in dry-run")

        code = ai_review.run_review(cfg, model_caller=should_not_run)
        self.assertEqual(code, 0)


class CallModelTests(unittest.TestCase):
    def test_invalid_http_json_is_parse_error(self):
        class Response:
            def __enter__(self):
                return self

            def __exit__(self, exc_type, exc_value, traceback):
                return False

            def read(self):
                return b"not json"

        with mock.patch.object(ai_review.urllib.request, "urlopen", return_value=Response()):
            with self.assertRaises(ai_review.ParseError):
                ai_review.call_model("https://example.invalid", FAKE_KEY, "test-model", [])

    def test_timeout_is_network_error(self):
        with mock.patch.object(
            ai_review.urllib.request, "urlopen", side_effect=TimeoutError
        ):
            with self.assertRaises(ai_review.NetworkError):
                ai_review.call_model("https://example.invalid", FAKE_KEY, "test-model", [])


if __name__ == "__main__":
    unittest.main()
