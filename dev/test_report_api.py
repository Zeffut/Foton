#!/usr/bin/env python3
"""Focused checks for the report-to-GitHub boundary, with no network calls."""

import hashlib
import hmac
import importlib.util
import pathlib
import unittest
from unittest import mock


API_PATH = pathlib.Path(__file__).resolve().parent.parent / "api" / "report.py"
SPEC = importlib.util.spec_from_file_location("report_api", API_PATH)
report_api = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(report_api)


class IssueStatus(unittest.TestCase):
    def test_open_issue_reopens_the_site_report(self):
        self.assertEqual(report_api._issue_status({"state": "open", "labels": []}), "open")

    def test_fixed_and_not_a_bug_are_distinct(self):
        self.assertEqual(
            report_api._issue_status({"state": "closed", "labels": [{"name": "fixed"}]}),
            "fixed",
        )
        self.assertEqual(
            report_api._issue_status({"state": "closed", "labels": [{"name": "not-a-bug"}]}),
            "closed",
        )

    def test_ambiguous_closure_is_not_guessed(self):
        self.assertIsNone(report_api._issue_status({
            "state": "closed", "labels": [{"name": "fixed"}, {"name": "not-a-bug"}],
        }))
        self.assertIsNone(report_api._issue_status({"state": "closed", "labels": []}))


class ReportIdentity(unittest.TestCase):
    def test_two_identical_reports_with_different_local_numbers_are_distinct(self):
        report = {"number": 4, "description": "same repro", "player": "Alex"}
        next_report = {**report, "number": 5}
        self.assertNotEqual(report_api._report_key(report), report_api._report_key(next_report))

    def test_issue_search_walks_past_the_first_page(self):
        record = {"report_key": "needle"}
        first_page = [{"body": "unrelated"}] * 100
        matching_issue = {"number": 101, "body": report_api._issue_marker(record)}
        with mock.patch.object(report_api, "_github", side_effect=[
            (200, first_page), (200, [matching_issue]),
        ]) as github:
            status, found = report_api._find_existing_issue(record, "token", "Zeffut/Foton")
        self.assertEqual(status, 200)
        self.assertEqual(found, matching_issue)
        self.assertIn("page=2", github.call_args_list[1].args[1])

    def test_description_limit_leaves_room_for_the_github_issue_context(self):
        self.assertLess(report_api.MAX_ISSUE_DESCRIPTION, report_api.MAX_REPORT_BODY)


class WebhookSignature(unittest.TestCase):
    def test_only_the_exact_body_and_secret_validate(self):
        body = b'{"issue": 1}'
        signature = "sha256=" + hmac.new(b"secret", body, hashlib.sha256).hexdigest()
        self.assertTrue(report_api._valid_signature("secret", body, signature))
        self.assertFalse(report_api._valid_signature("secret", body + b" ", signature))
        self.assertFalse(report_api._valid_signature("other", body, signature))


class SiteSynchronization(unittest.TestCase):
    def test_webhook_updates_the_report_with_its_issue_number(self):
        records = [{"number": 8, "issue_number": 42, "status": "open"}]
        with mock.patch.object(report_api, "_read_reports", return_value=(200, records, "sha")), \
             mock.patch.object(report_api, "_write_reports", return_value=200) as write:
            result = report_api._sync_status(
                {"number": 42, "state": "closed", "labels": [{"name": "fixed"}]},
                "token", "Zeffut/Foton",
            )
        self.assertEqual(result, 202)
        self.assertEqual(records[0]["status"], "fixed")
        self.assertIn("sync issue #42 as fixed", write.call_args.args[-1])

    def test_unclassified_closure_does_not_write_a_false_status(self):
        with mock.patch.object(report_api, "_read_reports") as read:
            self.assertEqual(report_api._sync_status({"number": 42, "state": "closed", "labels": []}, "t", "r"), 202)
        read.assert_not_called()

    def test_historical_report_is_linked_when_its_matching_issue_arrives(self):
        records = [{"number": 5, "status": "open"}]
        issue = {
            "number": 5, "state": "open", "labels": [{"name": "foton-report"}],
            "html_url": "https://github.com/Zeffut/Foton/issues/5",
        }
        with mock.patch.object(report_api, "_read_reports", return_value=(200, records, "sha")), \
             mock.patch.object(report_api, "_write_reports", return_value=200):
            self.assertEqual(report_api._sync_status(issue, "token", "Zeffut/Foton"), 202)
        self.assertEqual(records[0]["issue_number"], 5)
        self.assertEqual(records[0]["issue_url"], issue["html_url"])

    def test_linked_report_reads_the_current_issue_state_after_linking(self):
        record = {"number": 8, "issue_number": 42}
        with mock.patch.object(report_api, "_github", return_value=(200, {
            "number": 42, "state": "closed", "labels": [{"name": "fixed"}],
        })), mock.patch.object(report_api, "_sync_status", return_value=202) as sync:
            self.assertEqual(report_api._refresh_issue_status(record, "token", "Zeffut/Foton"), 202)
        sync.assert_called_once()


if __name__ == "__main__":
    unittest.main()
