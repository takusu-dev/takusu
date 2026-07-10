"""FunASR WebSocket server.

Protocol:
  Client -> Server:
    Text:   {"type": "start", "language": "ja", "hotwords": "Resonite ProtoFlux", "mode": "offline"}
    Binary:  float32 PCM audio data (16kHz, mono)
    Text:   {"type": "end"}

  Server -> Client:
    Text:   {"type": "result", "text": "transcribed text"}
    Text:   {"type": "error", "message": "..."}

Note: the "mode" field is accepted for protocol compatibility but only
"offline" behaviour is implemented; "2pass" partial results are not sent.
"""

from __future__ import annotations

import argparse
import asyncio
import contextlib
import json
import logging
import signal

import numpy as np
import websockets

from funasr_server import (
    DEFAULT_HOST,
    DEFAULT_LANGUAGE,
    DEFAULT_MODEL,
    DEFAULT_PORT,
    DEFAULT_VAD_MODEL,
)
from funasr_server.config import ServerConfig, TranscriptionRequest

logger = logging.getLogger(__name__)

_LOG_FORMAT = "%(asctime)s %(levelname)s %(name)s: %(message)s"


def _postprocess(text: str) -> str:
    try:
        from funasr.utils.postprocess_utils import rich_transcription_postprocess

        return rich_transcription_postprocess(text)
    except Exception:
        return text


def _extract_text(result: list) -> str:
    if not result:
        return ""
    item = result[0]
    if isinstance(item, dict):
        return item.get("text", "")
    text = getattr(item, "text", None)
    if text:
        return text
    return str(item)


def _transcribe(audio, hotwords, language):
    return _model.generate(
        input=audio,
        language=language,
        hotwords=hotwords,
    )


_model: AutoModel | None = None  # noqa: F821
# Serializes access to _model.generate(). AutoModel is not documented as
# thread-safe, so concurrent run_in_executor calls could crash or corrupt
# results (#283). The lock wraps the entire executor call.
_model_lock: asyncio.Lock  # initialized in _run()


def _load_model(config: ServerConfig) -> AutoModel:  # noqa: F821
    from funasr import AutoModel

    vad = config.vad_model
    if config.onnx and not vad.endswith("-onnx"):
        vad = vad.replace("-pytorch", "-onnx")
        if not vad.endswith("-onnx"):
            vad = vad + "-onnx"

    hub = config.hub
    logger.info("Loading model=%s vad_model=%s hub=%s ...", config.model, vad, hub)
    model = AutoModel(
        model=config.model, vad_model=vad, hub=hub, disable_update=True, trust_remote_code=True
    )
    logger.info("Model loaded.")
    global _model
    _model = model
    return model


class _Session:
    __slots__ = ("buf", "request")

    def __init__(self) -> None:
        self.request: TranscriptionRequest | None = None
        self.buf = bytearray()


async def _handle(websocket: websockets.ServerConnection) -> None:
    global _model
    session = _Session()

    async for message in websocket:
        try:
            if isinstance(message, str):
                data = json.loads(message)
                msg_type = data.get("type")

                if msg_type == "start":
                    session.request = TranscriptionRequest(
                        language=data.get("language", DEFAULT_LANGUAGE),
                        hotwords=data.get("hotwords", ""),
                        mode=data.get("mode", "offline"),
                    )
                    session.buf = bytearray()

                elif msg_type == "end":
                    if session.request is None or not session.buf:
                        await websocket.send(
                            json.dumps({"type": "error", "message": "no audio data"})
                        )
                        continue

                    audio = np.frombuffer(bytes(session.buf), dtype=np.float32)
                    loop = asyncio.get_running_loop()
                    hotwords = session.request.hotwords
                    language = session.request.language
                    async with _model_lock:
                        result = await loop.run_in_executor(
                            None,
                            _transcribe,
                            audio,
                            hotwords,
                            language,
                        )

                    text = ""
                    raw = _extract_text(result)
                    if raw:
                        text = _postprocess(raw)

                    await websocket.send(json.dumps({"type": "result", "text": text}))
                    session.buf = bytearray()

            elif isinstance(message, bytes):
                session.buf.extend(message)

        except Exception:
            logger.debug("Connection closed or error", exc_info=True)
            with contextlib.suppress(websockets.ConnectionClosed):
                await websocket.send(json.dumps({"type": "error", "message": "internal error"}))


async def _run(config: ServerConfig) -> None:
    global _model_lock
    _model_lock = asyncio.Lock()
    model = _load_model(config)
    global _model
    _model = model

    server = await websockets.serve(
        _handle,
        config.host,
        config.port,
        max_size=None,
    )
    logger.info("FunASR server listening on ws://%s:%d", config.host, config.port)

    stop = asyncio.Event()

    def _signal_handler() -> None:
        logger.info("Shutting down...")
        stop.set()

    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, _signal_handler)

    await stop.wait()
    server.close()
    await server.wait_closed()
    logger.info("Server stopped.")


def main() -> None:
    parser = argparse.ArgumentParser(description="FunASR WebSocket server for takusu")
    parser.add_argument("--host", default=DEFAULT_HOST, help="bind address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="bind port")
    parser.add_argument("--model", default=DEFAULT_MODEL, help="model id")
    parser.add_argument("--vad-model", default=DEFAULT_VAD_MODEL, help="vad model id")
    parser.add_argument("--language", default=DEFAULT_LANGUAGE, help="default language")
    parser.add_argument(
        "--onnx", action="store_true", help="use ONNX models for faster CPU inference"
    )
    parser.add_argument(
        "--hub",
        default="hf",
        choices=["hf", "ms"],
        help="model hub: hf (HuggingFace) or ms (ModelScope)",
    )
    parser.add_argument("--verbose", "-v", action="store_true", help="enable debug logging")
    args = parser.parse_args()

    logging.basicConfig(level=logging.DEBUG if args.verbose else logging.INFO, format=_LOG_FORMAT)

    config = ServerConfig(
        host=args.host,
        port=args.port,
        model=args.model,
        vad_model=args.vad_model,
        language=args.language,
        onnx=args.onnx,
        hub=args.hub,
    )

    asyncio.run(_run(config))
