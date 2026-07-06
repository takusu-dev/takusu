"""Tests that `_model_lock` serializes concurrent `_model.generate()` calls.

These tests stub the global `_model` with a fake that records concurrency so
we can verify the asyncio.Lock prevents overlapping inference without needing
the real FunASR runtime (which is heavy and not available in CI).
"""

from __future__ import annotations

import asyncio
import json

import numpy as np
import pytest

import funasr_server.server as srv


class _FakeModel:
    """Records whether `generate` ever ran concurrently with itself."""

    def __init__(self) -> None:
        self.in_flight = 0
        self.max_concurrent = 0
        self.calls = 0
        self.delay = 0.05

    def generate(self, input, language, hotwords):
        self.calls += 1
        self.in_flight += 1
        self.max_concurrent = max(self.max_concurrent, self.in_flight)
        # Simulate inference work in a worker thread.
        import time

        time.sleep(self.delay)
        self.in_flight -= 1
        return [{"text": f"ok-{self.calls}"}]


@pytest.fixture
def fake_model(monkeypatch):
    model = _FakeModel()
    monkeypatch.setattr(srv, "_model", model)
    monkeypatch.setattr(srv, "_model_lock", asyncio.Lock())
    return model


async def _send_one() -> dict:
    """Drive a single transcription through `_handle` with a fake websocket."""
    received: list[str] = []

    class _FakeWs:
        def __aiter__(self):
            return self

        async def __anext__(self):
            if not received:
                received.append("start")
                return json.dumps(
                    {"type": "start", "language": "ja", "hotwords": "", "mode": "offline"}
                )
            if received[-1] == "start":
                received.append("audio")
                return np.zeros(1600, dtype=np.float32).tobytes()
            if received[-1] == "audio":
                received.append("end")
                return json.dumps({"type": "end"})
            raise StopAsyncIteration

        async def send(self, payload: str) -> None:
            received.append(payload)

    ws = _FakeWs()
    await srv._handle(ws)  # type: ignore[attr-defined]
    # Find the result JSON among received payloads.
    for p in received:
        try:
            obj = json.loads(p)
        except (json.JSONDecodeError, TypeError):
            continue
        if isinstance(obj, dict) and obj.get("type") == "result":
            return obj
    pytest.fail("no result message received")


@pytest.mark.asyncio
async def test_concurrent_clients_are_serialized(fake_model):
    # Launch 4 concurrent transcriptions; without the lock they would overlap
    # in the executor threads and `max_concurrent` would exceed 1.
    await asyncio.gather(*(_send_one() for _ in range(4)))
    assert fake_model.calls == 4
    assert fake_model.max_concurrent == 1, (
        f"expected serialized inference, max_concurrent={fake_model.max_concurrent}"
    )


@pytest.mark.asyncio
async def test_single_client_returns_result(fake_model):
    result = await _send_one()
    assert result["type"] == "result"
    assert result["text"].startswith("ok-")
