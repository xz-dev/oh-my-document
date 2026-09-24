#!/usr/bin/env python3
"""Replay the exact tutorial Bash blocks in isolated, retained fixtures."""
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
import tomllib

REPO = Path(__file__).resolve().parents[2]
binary = os.environ.get("OMD_BIN", str(REPO / "target/debug/omd"))
binary = Path(shutil.which(binary) or binary).resolve(strict=True)
output = Path(tempfile.mkdtemp(prefix="omd-tutorial-check-"))
(output / "bin").mkdir()
(output / "bin" / "omd").symlink_to(binary)


def load(project, name):
    return json.loads((project / name).read_text())


for language, filename, requirement in [
    ("en", "retry.md", "spec.md"),
    ("zh", "retry.zh-CN.md", "需求.md"),
]:
    run = output / language
    run.mkdir()
    (run / "home").mkdir()
    document = (REPO / "docs/tutorials" / filename).read_text()
    blocks = re.findall(r"```bash\n(.*?)```", document, re.S)
    assert blocks, filename
    script = "set -euo pipefail\n"
    for index, block in enumerate(blocks):
        script += f"\nprintf '\\n--- block {index} ---\\n'\n" + block
        # Preserve each visible observation before the next block overwrites it.
        script += (
            f"\nif [ -f observation.json ]; then cp observation.json "
            f"{shlex.quote(str(run / f'observation-{index}.json'))}; fi\n"
        )
    script += f"printf '%s' \"$DEMO\" > {shlex.quote(str(run / 'demo-path'))}\n"
    (run / "workflow.sh").write_text(script)
    env = dict(os.environ)
    for key in ("OMD_META", "OMD_CONFIG_PATH", "OMD_CACHE_PATH", "BASH_ENV", "ENV"):
        env.pop(key, None)
    env.update(HOME=str(run / "home"), TMPDIR=str(run), PATH=str(output / "bin") + os.pathsep + env["PATH"])
    result = subprocess.run(["bash", str(run / "workflow.sh")], cwd=run, env=env, capture_output=True, text=True)
    (run / "stdout.log").write_text(result.stdout)
    (run / "stderr.log").write_text(result.stderr)
    assert result.returncode == 0, f"{language}: exit {result.returncode}; logs: {run}"
    project = Path((run / "demo-path").read_text()) / "project"
    spec = load(project, "spec-range.json")["data"]["object"]
    code = load(project, "code-range.json")["data"]["object"]
    link = load(project, "code-range.json")["data"]["link_records"][0]
    observations = [json.loads(p.read_text()) for p in run.glob("observation-*.json")]
    assert any("range:" + spec["chain_root_commit_id"] in o["data"]["dirty"] for o in observations)
    assert link["source"]["object"]["root_commit_id"] == spec["chain_root_commit_id"]
    assert link["target"]["object"]["root_commit_id"] == code["chain_root_commit_id"]
    assert code["position"]["start"] == "0" and code["position"]["end"] == "15"
    assert spec["position"]["end"] == ("22" if language == "en" else "9")
    changed = load(project, "spec-five.json")["data"]
    updated = load(project, "code-five.json")["data"]
    adapted = load(project, "adapted.json")["data"]
    assert changed["object"]["chain_root_commit_id"] == spec["chain_root_commit_id"]
    assert updated["object"]["chain_root_commit_id"] == code["chain_root_commit_id"]
    assert updated["object"]["tip_commit_id"] != code["tip_commit_id"]
    assert adapted["kind"] == "Adapt"
    assert adapted["object"]["chain_root_commit_id"] == code["chain_root_commit_id"]
    record = tomllib.loads((project / ".omd/commits" / (adapted["commit"] + ".toml")).read_text())
    assert link["link_id"] in json.dumps(record["payload"])
    assert changed["commit"] in json.dumps(record["payload"])
    final = load(project, "observation.json")["data"]
    assert final["ok"] and all(not final[k] for k in ("dirty", "locate", "missing", "unverified", "obligations", "open_blocks"))
    assert (project / "retry.py").read_text() == "MAX_RETRIES = 5\n"
    assert "5" in (project / requirement).read_text()
    history = load(project, "history.json")["data"]
    assert adapted["object"]["tip_commit_id"] in history["chain"]
    assert code["chain_root_commit_id"] in history["chain"]
    print(f"PASS {language}: {len(blocks)} exact Bash blocks; dirty detection, link endpoints, stable roots, selected adaptation and final review checked")

print(f"Retained evidence: {output}")
