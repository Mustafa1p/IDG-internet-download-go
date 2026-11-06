"""Pin publisher metadata to the exact benchmark artifact, never a nearby file."""
import base64
import copy
import json
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from benchmark_sources import HTTPS_SOURCES, check_reference, verify_sources


class SourcesTests(unittest.TestCase):
    def test_sha256_list_requires_exact_filename_and_pinned_digest(self):
        source = HTTPS_SOURCES["F"]
        filename = source["url"].rsplit("/", 1)[1]
        correct = f"{source['sha256']}  {filename}\n".encode()
        self.assertEqual(check_reference(source, correct), source["sha256"])
        for payload in [correct.replace(filename.encode(), b"other.zip"),
                        correct.replace(source["sha256"].encode(), b"0" * 64), correct + correct]:
            with self.subTest(payload=payload), self.assertRaises(ValueError):
                check_reference(source, payload)

    def test_sigstore_metadata_requires_sha256_and_exact_digest(self):
        source = HTTPS_SOURCES["G"]
        bundle = dict(messageSignature=dict(messageDigest=dict(
            algorithm="SHA2_256", digest=base64.b64encode(bytes.fromhex(source["sha256"])).decode())))
        self.assertEqual(check_reference(source, json.dumps(bundle).encode()), source["sha256"])
        for key, value in [("algorithm", "SHA2_512"), ("digest", base64.b64encode(b"wrong").decode()),
                           ("digest", "!")]:
            changed = copy.deepcopy(bundle)
            changed["messageSignature"]["messageDigest"][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                check_reference(source, json.dumps(changed).encode())

    def test_local_workloads_make_no_publisher_requests(self):
        self.assertEqual(verify_sources(["A", "B", "C", "D"]), {})


if __name__ == "__main__":
    unittest.main()
