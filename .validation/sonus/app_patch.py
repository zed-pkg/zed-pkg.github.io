#!/usr/bin/env python3
from __future__ import annotations

import shutil
import sys
from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: app_patch.py <sonus-web-server-root>")

    target = Path(sys.argv[1]).resolve()
    fixture_dir = Path(__file__).resolve().parent

    app_path = target / "src/app.rs"
    app = app_path.read_text()
    app = replace_once(
        app,
        '\nconst SESSION_COOKIE: &str = "sonus_auris_session";\n',
        '\nmod marketing_session;\n\nconst SESSION_COOKIE: &str = "sonus_auris_session";\n',
        "module declaration",
    )
    app = replace_once(
        app,
        '''        .route("/auth/sign-out", post(sign_out))
        .route("/dashboard", get(dashboard))''',
        '''        .route("/auth/sign-out", post(sign_out))
        .route("/auth/session/status", get(marketing_session::status))
        .route("/auth/session/refresh", post(marketing_session::refresh))
        .route("/dashboard", get(dashboard))''',
        "session routes",
    )
    app = replace_once(
        app,
        '''    if guarded && !request_is_same_origin(request.headers(), &state.config.public_origin) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "ok": false,
                "error": "cross_site_request_rejected"
            })),
        )
            .into_response();
    }
''',
        '''    let marketing_refresh = marketing_session::is_marketing_refresh(
        request.method(),
        request.uri().path(),
        request.headers(),
    );

    if guarded
        && !request_is_same_origin(request.headers(), &state.config.public_origin)
        && !marketing_refresh
    {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "ok": false,
                "error": "cross_site_request_rejected"
            })),
        )
            .into_response();
    }
''',
        "same-origin exception",
    )
    app_path.write_text(app)

    module_target = target / "src/app/marketing_session.rs"
    module_target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(fixture_dir / "marketing_session.rs", module_target)

    docs_target = target / "docs/marketing-session-contract.md"
    docs_target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(fixture_dir / "marketing-session-contract.md", docs_target)


if __name__ == "__main__":
    main()
