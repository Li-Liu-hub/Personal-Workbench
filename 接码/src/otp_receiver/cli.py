"""Command line entry point for the standalone OTP receiver."""

from __future__ import annotations

import argparse
import ipaddress
import json
import logging
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

from . import __version__
from .config import ConfigError, load_config
from .errors import ToolError
from .server import (
    OtpInbox,
    OtpReceiver,
    detect_bind_address,
    load_or_create_token,
    mask_url_token,
    read_runtime_endpoint,
    receiver_urls,
    write_runtime_file,
    write_url_file,
)


def _build_parser() -> argparse.ArgumentParser:
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--config", default=argparse.SUPPRESS, help="配置文件路径")

    parser = argparse.ArgumentParser(
        prog="otp-receiver",
        description="本地短信验证码接收端：接收 SmsForwarder Webhook，仅在本机提供取码接口。",
        parents=[common],
    )
    parser.set_defaults(config="config.yaml")
    parser.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
    sub = parser.add_subparsers(dest="command", required=True)

    serve = sub.add_parser("serve", help="启动接收端")
    serve.add_argument("--bind-ip", default="", help="监听 IP；不填使用配置，auto 只选热点/局域网网卡")
    serve.add_argument("--host-ip", default="", help="生成手机访问地址时使用的电脑 IP")
    serve.add_argument("--port", type=int, default=None, help="监听端口；0 表示随机端口")
    serve.add_argument("--token", default="", help="临时指定 token；默认读取/生成 token 文件")
    serve.add_argument("--rotate-token", action="store_true", help="生成新 token，旧 token 立即失效")
    serve.add_argument("--ttl", type=int, default=None, help="验证码内存有效期（秒）")

    sub.add_parser("status", help="查询正在运行的接收端状态（不显示验证码）")
    return parser


def _serve(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    otp = config["otp"]

    requested_bind = args.bind_ip.strip() or str(otp["bind_ip"]).strip()
    detected_name = ""
    if requested_bind.lower() == "auto":
        detected = detect_bind_address()
        if detected:
            detected_name, requested_bind = detected
        else:
            requested_bind = "127.0.0.1"
    try:
        bind_address = ipaddress.ip_address(requested_bind)
    except ValueError as exc:
        raise ToolError("otp_bind_invalid", f"无效的绑定 IP: {requested_bind}") from exc
    if bind_address.is_unspecified:
        print("警告: 绑定 0.0.0.0 会监听所有网卡，建议使用 auto 或热点 IP。", file=sys.stderr)

    ttl = int(otp["ttl_seconds"]) if args.ttl is None else args.ttl
    if not 0 <= ttl <= 600:
        raise ToolError("otp_ttl_invalid", "ttl 必须在 0 到 600 秒之间")
    port = int(otp["port"]) if args.port is None else args.port
    if not 0 <= port <= 65535:
        raise ToolError("otp_port_invalid", "端口必须在 0 到 65535 之间")

    token = args.token.strip() or load_or_create_token(
        Path(config["_token_path"]),
        rotate=args.rotate_token,
    )
    inbox = OtpInbox(ttl_seconds=ttl)
    receiver = OtpReceiver(
        inbox,
        token,
        host=str(bind_address),
        port=port,
        max_clock_skew_seconds=int(otp["max_clock_skew_seconds"]),
        max_requests_per_minute=int(otp["max_requests_per_minute"]),
    )
    receiver.start()
    write_runtime_file(Path(config["_runtime_path"]), receiver.host, receiver.actual_port)

    logging.basicConfig(level=logging.INFO, format="[%(asctime)s] %(message)s", datefmt="%H:%M:%S")
    display_ip = args.host_ip.strip() or str(bind_address)
    if display_ip in ("0.0.0.0", "::"):
        display_ip = ""
    urls = receiver_urls(token, receiver.actual_port, host_ip=display_ip)
    if bind_address.is_loopback:
        print(
            "警告: 接收端只监听本机回环地址，手机无法访问；请检查热点连接或指定热点 IP。",
            file=sys.stderr,
        )
    try:
        write_url_file(Path(config["_url_path"]), urls)
    except OSError as exc:
        print(f"警告: 无法写入完整地址文件 {config['_url_path']}: {exc}", file=sys.stderr)

    payload = {
        "success": True,
        "listen": f"{receiver.host}:{receiver.actual_port}",
        "adapter": detected_name or (urls[0][0] if urls else display_ip or "未知"),
        "token_file": config["_token_path"],
        "url_file": config["_url_path"],
        "runtime_file": config["_runtime_path"],
        "ttl_seconds": ttl,
        "urls": [{"adapter": name, "url": mask_url_token(url)} for name, url in urls],
    }
    print(json.dumps(payload, ensure_ascii=False, indent=2), flush=True)
    logging.info("OTP receiver listening on %s:%s", receiver.host, receiver.actual_port)
    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        receiver.stop()
        logging.info("OTP receiver stopped")
    return 0


def _status(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    token_path = Path(config["_token_path"])
    if not token_path.is_file():
        raise ToolError("otp_token_missing", "找不到 token 文件，请先启动接收端")
    token = token_path.read_text(encoding="utf-8").strip()
    if not token:
        raise ToolError("otp_token_missing", "token 文件为空，请重新启动接收端")

    candidates: list[tuple[str, int]] = []
    runtime = read_runtime_endpoint(Path(config["_runtime_path"]))
    if runtime:
        candidates.append(runtime)
    otp = config["otp"]
    bind_ip = str(otp["bind_ip"]).strip()
    if bind_ip and bind_ip.lower() != "auto":
        candidates.append((bind_ip, int(otp["port"])))
    detected = detect_bind_address()
    if detected:
        candidates.append((detected[1], int(otp["port"])))
    candidates.append(("127.0.0.1", int(otp["port"])))

    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    seen: set[tuple[str, int]] = set()
    last_error: Exception | None = None
    for host, port in candidates:
        if (host, port) in seen:
            continue
        seen.add((host, port))
        url = f"http://{host}:{port}/otp/{token}/status"
        try:
            with opener.open(urllib.request.Request(url), timeout=1.5) as response:
                data = json.loads(response.read().decode("utf-8", errors="replace"))
        except urllib.error.HTTPError as exc:
            if exc.code == 403:
                raise ToolError("otp_forbidden", "接收端拒绝访问：token 不匹配") from exc
            last_error = exc
            continue
        except (urllib.error.URLError, TimeoutError, OSError, json.JSONDecodeError) as exc:
            last_error = exc
            continue
        if not isinstance(data, dict) or not data.get("ok"):
            last_error = ToolError("otp_receiver_error", "接收端状态异常")
            continue
        print(json.dumps({"success": True, "listen": f"{host}:{port}", **data}, ensure_ascii=False, indent=2))
        return 0
    raise ToolError(
        "otp_receiver_unreachable",
        "接收端未运行或不可达，请先执行 otp-receiver serve",
    ) from last_error


def main(argv: list[str] | None = None) -> int:
    args = _build_parser().parse_args(argv)
    try:
        if args.command == "serve":
            return _serve(args)
        if args.command == "status":
            return _status(args)
        raise ToolError("otp_command_invalid", f"未知命令: {args.command}")
    except (ToolError, ConfigError, OSError) as exc:
        if isinstance(exc, ToolError):
            code = exc.code
            message = exc.message
        elif isinstance(exc, ConfigError):
            code = "config_error"
            message = str(exc)
        else:
            code = "otp_io_error"
            message = str(exc)
        print(
            json.dumps({"success": False, "error": {"code": code, "message": message}}, ensure_ascii=False),
            file=sys.stderr,
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
