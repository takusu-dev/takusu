"""Configuration for Moonshine server."""

from __future__ import annotations

from dataclasses import dataclass, field

from moonshine_server import DEFAULT_HOST, DEFAULT_LANGUAGE, DEFAULT_MODEL_ARCH, DEFAULT_PORT


@dataclass
class ServerConfig:
    host: str = DEFAULT_HOST
    port: int = DEFAULT_PORT
    language: str = DEFAULT_LANGUAGE
    model_arch: int = DEFAULT_MODEL_ARCH
    model_path: str | None = None
    models_dir: str | None = None


@dataclass
class TranscriptionRequest:
    language: str = DEFAULT_LANGUAGE
    model_arch: int | None = None
