"""Safety-logic checks, no hardware or network needed:  python tests/test_guard.py"""
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "gateway"))
import gateway as gw  # noqa: E402


class FakeLink:
    def __init__(self):
        self.sent = []

    def send(self, cmd):
        self.sent.append(cmd)


class FakeLLM:
    configured = True
    model = "fake"

    def __init__(self, out):
        self.out = out

    def analyze(self, evidence):
        return self.out, "fake"


def sample(up, pct):
    return {"uptime_s": up, "heap_free": int(pct * 1e5), "heap_total": 10_000_000, "free_pct": pct, "temp_c": 39.0, "reset_cause": "hard"}


def make(out):
    link = FakeLink()
    g = gw.Guard(link, FakeLLM(out), Path(tempfile.mkdtemp()) / "audit.jsonl", spawn=lambda f: f())
    return g, link


GOOD = {"diagnosis": "leak", "root_cause_hypothesis": "x", "severity": "high", "confidence": 0.9,
        "proposed_action": "restart_device", "reasoning": "r", "risks": "k"}


def run():
    # 1. rule needs 3 consecutive low samples
    g, link = make(GOOD)
    for i, p in enumerate([90, 30, 30]):
        g.on_sample(sample(i, p))
    assert g.incident is None
    g.on_sample(sample(3, 30))
    assert g.incident and g.incident["state"] == "awaiting_approval"

    # 2. wrong digest, deny path never touch the device
    ok, _ = g.decide("approve", "bad")
    assert not ok and link.sent == []

    # 3. right digest sends exactly one reboot; incident stays open until real recovery
    ok, _ = g.decide("approve", g.incident["proposal"]["digest"])
    assert ok and link.sent == ["reboot"] and g.incident["state"] == "verifying"
    for i in range(10):  # healthy numbers but no reboot observed -> must NOT resolve
        g.on_sample(sample(10 + i, 95))
    assert g.incident["state"] == "verifying"
    g.on_sample(sample(0, 99))  # uptime reset proves a reboot
    for i in range(1, 7):
        g.on_sample(sample(i, 99))
    assert g.incident["state"] == "resolved"

    # 4. replaying the old digest is rejected once the incident is closed
    assert not g.decide("approve", "anything")[0]

    # 5. model output outside the allowlist is rejected, not executed
    for bad in ('{"proposed_action": "flash_firmware"}', "no json here"):
        try:
            gw.parse_analysis(bad)
            raise SystemExit("accepted bad model output: " + bad)
        except ValueError:
            pass

    # 6. think-tags and prose around the JSON are tolerated
    assert gw.parse_analysis('<think>hmm</think> sure {"proposed_action":"monitor_only","severity":"nope"}')["severity"] == "medium"

    # 7. monitor_only creates nothing to approve
    g2, link2 = make(dict(GOOD, proposed_action="monitor_only"))
    for i in range(4):
        g2.on_sample(sample(i, 20))
    assert g2.incident["proposal"] is None and not g2.decide("approve", "x")[0] and link2.sent == []

    # 8. no API key -> honest "analysis unavailable", human can still decide
    g3, _ = make(GOOD)
    g3.llm.configured = False
    for i in range(4):
        g3.on_sample(sample(i, 20))
    assert g3.incident["analysis"] is None and "NEBIUS_API_KEY" in g3.incident["analysis_error"]
    print("all guard checks passed")


if __name__ == "__main__":
    run()
