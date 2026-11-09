import json
import re
import secrets
import subprocess
import sys
import time


def fail(message):
    print(message, file=sys.stderr)
    raise SystemExit(2)


def sql_text(value):
    return "CONVERT(X'%s' USING utf8mb4)" % value.encode("utf-8").hex()


def option_value(value):
    if "\n" in value or "\r" in value:
        fail("数据库配置格式无效")
    return '"%s"' % value.replace("\\", "\\\\").replace('"', '\\"')


def docker(args, input_bytes=None, timeout=25):
    try:
        result = subprocess.run(
            ["sudo", "-n", "docker", *args],
            input=input_bytes,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        fail("无法调用服务器上的 MySQL 客户端")
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        fail(detail[-600:] if detail else "服务器数据库命令失败")
    return result.stdout


def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        fail("文章数据无法读取")

    post_id = payload.get("post_id", "")
    tenant_id = payload.get("tenant_id", "")
    title = payload.get("title", "")
    content = payload.get("content", "")
    published_date = payload.get("published_date", "")
    published_at = payload.get("published_at", 0)
    tags = payload.get("tags", [])
    if not re.fullmatch(r"[0-9a-f]{16}", post_id):
        fail("文章编号无效")
    if tenant_id != "0000000000000000":
        fail("博客租户编号无效")
    if not isinstance(title, str) or not title.strip() or len(title) > 200:
        fail("博客标题无效")
    if not isinstance(content, str) or len(content.encode("utf-8")) > 200000:
        fail("博客正文超过 200 KB")
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", published_date):
        fail("发布日期无效")
    if not isinstance(published_at, int) or published_at <= 0:
        fail("发布时间无效")
    if not isinstance(tags, list) or len(tags) > 8:
        fail("文章标签数量无效")
    tags = [tag.strip() for tag in tags if isinstance(tag, str) and tag.strip()]
    tags = list(dict.fromkeys(tags))
    if len(tags) > 8 or any(len(tag) > 50 for tag in tags):
        fail("文章标签无效")

    try:
        config = subprocess.run(
            ["sudo", "-n", "cat", "/opt/blog/blog.env"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        fail("无法读取博客数据库配置")
    if config.returncode != 0:
        fail("无法读取博客数据库配置")
    dsn = None
    for line in config.stdout.decode("utf-8", errors="replace").splitlines():
        if line.startswith("BLOG_DB_DSN="):
            dsn = line.split("=", 1)[1].strip()
            if len(dsn) >= 2 and dsn[0] == dsn[-1] and dsn[0] in "'\"":
                dsn = dsn[1:-1]
            break
    match = re.fullmatch(
        r"([^:]+):(.*)@tcp\(([^:()]+):(\d+)\)/([A-Za-z0-9_]+)(?:\?.*)?",
        dsn or "",
    )
    if not match or match.group(5) != "blog" or match.group(4) != "3306":
        fail("博客数据库配置格式无效")
    db_user, db_password = match.group(1), match.group(2)
    temporary = "/tmp/.personaldesk-%s.cnf" % secrets.token_hex(8)
    config_text = (
        "[client]\n"
        "user=%s\n" % option_value(db_user)
        + "password=%s\n" % option_value(db_password)
        + "host=127.0.0.1\nport=3306\ndatabase=blog\n"
        "default-character-set=utf8mb4\n"
    )
    try:
        docker(
            ["exec", "-i", "ap-mysql", "sh", "-c", "umask 077; cat > " + temporary],
            config_text.encode("utf-8"),
        )

        now = int(time.time() * 1000)
        safe_title = sql_text(title.strip())
        safe_content = sql_text(content)
        statements = [
            "START TRANSACTION",
            "INSERT IGNORE INTO posts "
            "(id,tenant_id,title,content,status,created_at,updated_at) VALUES "
            "('%s','%s',%s,%s,'draft',%d,%d)"
            % (post_id, tenant_id, safe_title, safe_content, now, now),
            "UPDATE posts SET title=%s,content=%s,status='published',"
            "published_at=COALESCE(published_at,%d),"
            "published_date=COALESCE(published_date,'%s'),updated_at=%d "
            "WHERE id='%s' AND tenant_id='%s'"
            % (safe_title, safe_content, published_at, published_date, now, post_id, tenant_id),
            "DELETE FROM post_tags WHERE post_id='%s' AND EXISTS "
            "(SELECT 1 FROM posts WHERE id='%s' AND tenant_id='%s')"
            % (post_id, post_id, tenant_id),
        ]
        for tag in tags:
            safe_tag = sql_text(tag)
            tag_id = secrets.token_hex(8)
            statements.append(
                "INSERT IGNORE INTO tags(id,tenant_id,name) "
                "SELECT '%s','%s',%s WHERE EXISTS "
                "(SELECT 1 FROM posts WHERE id='%s' AND tenant_id='%s')"
                % (tag_id, tenant_id, safe_tag, post_id, tenant_id)
            )
            statements.append(
                "INSERT IGNORE INTO post_tags(post_id,tag_id) "
                "SELECT '%s',id FROM tags WHERE tenant_id='%s' AND name=%s "
                "AND EXISTS (SELECT 1 FROM posts WHERE id='%s' AND tenant_id='%s')"
                % (post_id, tenant_id, safe_tag, post_id, tenant_id)
            )
        statements.extend(
            [
                "SELECT COUNT(*) FROM posts WHERE id='%s' AND tenant_id='%s'"
                % (post_id, tenant_id),
                "COMMIT",
            ]
        )
        output = docker(
            [
                "exec",
                "-i",
                "ap-mysql",
                "mysql",
                "--defaults-extra-file=" + temporary,
                "--batch",
                "--silent",
                "--skip-column-names",
                "--raw",
            ],
            (";\n".join(statements) + ";\n").encode("utf-8"),
        )
        rows = [line.strip() for line in output.decode("utf-8", errors="replace").splitlines()]
        if "1" not in rows:
            fail("该文章编号不属于当前博客账户")
    finally:
        subprocess.run(
            ["sudo", "-n", "docker", "exec", "ap-mysql", "rm", "-f", temporary],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=10,
            check=False,
        )

    print("PUBLISHED " + post_id)


main()
