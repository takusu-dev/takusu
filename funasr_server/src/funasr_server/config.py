"""Configuration for FunASR server."""

from __future__ import annotations

from dataclasses import dataclass

from funasr_server import (
    DEFAULT_HOST,
    DEFAULT_LANGUAGE,
    DEFAULT_MODEL,
    DEFAULT_PORT,
    DEFAULT_VAD_MODEL,
)


@dataclass
class ServerConfig:
    host: str = DEFAULT_HOST
    port: int = DEFAULT_PORT
    model: str = DEFAULT_MODEL
    vad_model: str = DEFAULT_VAD_MODEL
    language: str = DEFAULT_LANGUAGE
    onnx: bool = False
    hub: str = "hf"


@dataclass
class TranscriptionRequest:
    language: str = DEFAULT_LANGUAGE
    hotwords: str = ""
    mode: str = "offline"  # "offline" or "2pass"
