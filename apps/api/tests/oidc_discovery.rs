//! G1：Discovery / JWKS 契约（进程内 HTTP，无需数据库）。
//!
//! - Discovery 的 issuer 与所有端点 URL 前缀逐字节一致，只宣告已实现能力；
//! - JWKS 只含公钥，active key 签发的 ID Token 可只凭 JWKS 验签；
//! - `[oidc].enabled = false`（未注册 provider）时端点 404。

mod oidc_support;

use actix_web::{App, test, web};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, jwk::JwkSet};
use oidc_support::*;
use serde_json::Value;

#[actix_web::test]
async fn discovery_document_matches_issuer_and_declared_capabilities() {
    let provider = web::Data::new(provider(test_config(), test_keys(false)));
    let app = test::init_service(
        App::new()
            .app_data(provider)
            .configure(floatctf::bootstrap::configure_all_routes),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/.well-known/openid-configuration")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("application/json")
    );
    let doc: Value = test::read_body_json(resp).await;
    assert_eq!(doc["issuer"], ISSUER);
    for key in [
        "authorization_endpoint",
        "token_endpoint",
        "userinfo_endpoint",
        "jwks_uri",
    ] {
        assert!(
            doc[key]
                .as_str()
                .unwrap()
                .starts_with(&format!("{ISSUER}/")),
            "{key}"
        );
    }
    assert_eq!(doc["response_types_supported"], serde_json::json!(["code"]));
    assert_eq!(
        doc["subject_types_supported"],
        serde_json::json!(["public"])
    );
    assert_eq!(
        doc["id_token_signing_alg_values_supported"],
        serde_json::json!(["RS256"])
    );
    assert_eq!(
        doc["code_challenge_methods_supported"],
        serde_json::json!(["S256"])
    );

    // RFC 8414 视图与 Discovery 一致。
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/.well-known/oauth-authorization-server")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let as_meta: Value = test::read_body_json(resp).await;
    assert_eq!(as_meta, doc);
}

#[actix_web::test]
async fn jwks_is_public_only_and_verifies_signed_id_tokens() {
    let provider = web::Data::new(provider(test_config(), test_keys(false)));
    let app = test::init_service(
        App::new()
            .app_data(provider.clone())
            .configure(floatctf::bootstrap::configure_all_routes),
    )
    .await;
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/.well-known/jwks.json")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let raw: Value = test::read_body_json(resp).await;
    for key in raw["keys"].as_array().unwrap() {
        for private in ["d", "p", "q", "dp", "dq", "qi"] {
            assert!(key.get(private).is_none(), "JWKS leaked {private}");
        }
    }
    let jwks: JwkSet = serde_json::from_value(raw).unwrap();
    assert_eq!(jwks.keys.len(), 2, "active + 过渡 key 都应发布");

    #[derive(serde::Serialize, serde::Deserialize)]
    struct C {
        iss: String,
        sub: String,
        aud: String,
        exp: i64,
    }
    let token = provider
        .keys
        .sign(&C {
            iss: ISSUER.into(),
            sub: "s".into(),
            aud: "client".into(),
            exp: chrono::Utc::now().timestamp() + 60,
        })
        .unwrap();
    let kid = jsonwebtoken::decode_header(&token).unwrap().kid.unwrap();
    let key = DecodingKey::from_jwk(jwks.find(&kid).unwrap()).unwrap();
    let mut v = Validation::new(Algorithm::RS256);
    v.set_audience(&["client"]);
    v.set_issuer(&[ISSUER]);
    let data = jsonwebtoken::decode::<C>(&token, &key, &v).unwrap();
    assert_eq!(data.claims.sub, "s");
}

#[actix_web::test]
async fn endpoints_are_404_when_oidc_disabled() {
    let app =
        test::init_service(App::new().configure(floatctf::bootstrap::configure_all_routes)).await;
    for uri in [
        "/.well-known/openid-configuration",
        "/.well-known/oauth-authorization-server",
        "/.well-known/jwks.json",
    ] {
        let resp = test::call_service(&app, test::TestRequest::get().uri(uri).to_request()).await;
        assert_eq!(resp.status(), 404, "{uri}");
    }
}
