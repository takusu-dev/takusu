"""Moonshine WebSocket server.

Uses moonshine-voice for on-device speech-to-text transcription.

Protocol:
  Client -> Server:
    Text:   {"type": "start", "language": "ja", "model_arch": 1}
    Binary:  float32 PCM audio data (16kHz, mono)
    Text:   {"type": "end"}

  Server -> Client:
    Text:   {"type": "result", "text": "transcribed text"}
    Text:   {"type": "error", "message": "..."}
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

from moonshine_server import DEFAULT_HOST, DEFAULT_LANGUAGE, DEFAULT_MODEL_ARCH, DEFAULT_PORT
from moonshine_server.config import ServerConfig, TranscriptionRequest

logger = logging.getLogger(__name__)

_LOG_FORMAT = "%(asctime)s %(levelname)s %(name)s: %(message)s"

_transcriber: object | None = None


def _load_model(config: ServerConfig) -> object:
    from moonshine_voice import ModelArch, Transcriber, get_model_for_language

    if config.model_path:
        model_path = config.model_path
        model_arch = (
            ModelArch(config.model_arch) if config.model_arch is not None else ModelArch.BASE
        )
        logger.info("Loading model from %s (arch=%s) ...", model_path, model_arch)
        model = Transcriber(model_path=model_path, model_arch=model_arch)
    else:
        logger.info(
            "Downloading/loading model for language=%s arch=%s ...",
            config.language,
            config.model_arch,
        )
        kw = {}
        if config.models_dir:
            from pathlib import Path

            kw["cache_root"] = Path(config.models_dir)
        model_path, model_arch = get_model_for_language(config.language, config.model_arch, **kw)
        logger.info("Model downloaded to %s", model_path)
        model = Transcriber(model_path=model_path, model_arch=model_arch)

    logger.info("Model loaded.")
    global _transcriber
    _transcriber = model
    return model


class _Session:
    __slots__ = ("buf", "request")

    def __init__(self) -> None:
        self.request: TranscriptionRequest | None = None
        self.buf = bytearray()


async def _handle(websocket: websockets.ServerConnection) -> None:
    global _transcriber
    session = _Session()

    async for message in websocket:
        try:
            if isinstance(message, str):
                data = json.loads(message)
                msg_type = data.get("type")

                if msg_type == "start":
                    session.request = TranscriptionRequest(
                        language=data.get("language", DEFAULT_LANGUAGE),
                        model_arch=data.get("model_arch"),
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

                    result = await loop.run_in_executor(
                        None,
                        lambda audio=audio: _transcriber.transcribe_without_streaming(
                            audio.tolist(), sample_rate=16000
                        ),
                    )

                    text = ""
                    if result and result.lines:
                        text = " ".join(line.text for line in result.lines if line.text)

                    await websocket.send(json.dumps({"type": "result", "text": text}))
                    session.buf = bytearray()

            elif isinstance(message, bytes):
                session.buf.extend(message)

        except Exception:
            logger.debug("Connection closed or error", exc_info=True)
            with contextlib.suppress(websockets.ConnectionClosed):
                await websocket.send(json.dumps({"type": "error", "message": "internal error"}))


async def _run(config: ServerConfig) -> None:
    model = _load_model(config)
    global _transcriber
    _transcriber = model

    server = await websockets.serve(
        _handle,
        config.host,
        config.port,
        max_size=None,
    )
    logger.info("Moonshine server listening on ws://%s:%d", config.host, config.port)

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
    parser = argparse.ArgumentParser(description="Moonshine STT WebSocket server for takusu")
    parser.add_argument("--host", default=DEFAULT_HOST, help="bind address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="bind port")
    parser.add_argument("--language", default=DEFAULT_LANGUAGE, help="language code (en, ja, etc.)")
    parser.add_argument(
        "--model-arch",
        type=int,
        default=None,
        help="model architecture: 0=tiny, 1=base, 2=tiny-streaming, 3=base-streaming, 4=small-streaming, 5=medium-streaming",
    )
    parser.add_argument(
        "--model-path",
        type=str,
        default=None,
        help="path to pre-downloaded model directory (overrides auto-download)",
    )
    parser.add_argument(
        "--models-dir",
        type=str,
        default=None,
        help="directory for model caching (default: ~/.cache/moonshine)",
    )
    parser.add_argument("--verbose", "-v", action="store_true", help="enable debug logging")
    args = parser.parse_args()

    logging.basicConfig(level=logging.DEBUG if args.verbose else logging.INFO, format=_LOG_FORMAT)

    config = ServerConfig(
        host=args.host,
        port=args.port,
        language=args.language,
        model_arch=args.model_arch or DEFAULT_MODEL_ARCH,
        model_path=args.model_path,
        models_dir=args.models_dir,
    )

    asyncio.run(_run(config))
