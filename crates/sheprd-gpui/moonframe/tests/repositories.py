"""Exercise the process provider against real, disposable checkouts."""

import json
from pathlib import Path
import selectors
import subprocess
import sys
import tempfile
import unittest

PROVIDER = Path(__file__).resolve().parents[1] / "app/providers/repositories/repositories.py"


class RepositoryProviderTest(unittest.TestCase):
    def test_switch_between_three_checkouts(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory).resolve()
            project = folder / 'project "suite"'
            outside = folder / "outside"
            for repo in (project / "chrome", project / "desktop" / "macOS",
                         project / "linux", outside):
                repo.mkdir(parents=True)
                subprocess.run(["git", "init", "-q", "-b", repo.name, repo], check=True)
                subprocess.run(["git", "-C", repo, "-c", "user.name=Fixture",
                                "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false",
                                "commit", "-qm", f"{repo.name} initial", "--allow-empty"], check=True)
            (project / "external").symlink_to(outside, target_is_directory=True)
            path = project / "desktop" / "macOS" / "new tab\tfile.txt"
            path.write_text("test\n")
            subprocess.run(["git", "-C", path.parent, "add", "--", path.name], check=True)
            with subprocess.Popen([sys.executable, str(PROVIDER)], stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, text=True) as provider, selectors.DefaultSelector() as watch:
                reader, writer = provider.stdout, provider.stdin
                assert reader is not None and writer is not None
                watch.register(reader, selectors.EVENT_READ)

                def read():
                    self.assertTrue(watch.select(5), "provider did not publish")
                    return json.loads(reader.readline())

                def send(scope, repository=None):
                    writer.write(json.dumps({"from": "details", "message": {
                        "git_scope": str(scope), "repository": repository,
                    }}) + "\n")
                    writer.flush()
                    return read()

                try:
                    self.assertEqual(read()["repos"], [])
                    initial = send(project, str(outside))
                    self.assertEqual({repo["name"] for repo in initial["repos"]}, {"chrome", "desktop/macOS", "linux"})
                    self.assertEqual(initial["selected"]["path"], str(project / "chrome"))
                    current = send(project, str(path.parent))
                    self.assertEqual(current["selected"]["branch"], "macOS")
                    self.assertEqual(current["selected"]["changes"], [{"path": path.name, "status": "A", "added": "1", "removed": "0"}])
                    self.assertEqual(current["selected"]["commits"][0]["subject"], "macOS initial")
                    checkout = send(project / "linux")
                    self.assertEqual(len(checkout["repos"]), 1)
                    self.assertEqual(checkout["selected"]["changes"], [])
                finally:
                    writer.close()
                    provider.wait(timeout=5)


if __name__ == "__main__":
    unittest.main()
