use wasm_bindgen::JsValue;
use worker::{Env, Request, Response};

use crate::error::WorkerError;
use crate::handlers::auth::db;
use crate::handlers::d1::safe_all;
use crate::handlers::tokens::{json_created, json_ok, parse_json};
use crate::models::{CreateSkill, SkillRow, UpdateSkill};

const SKILL_COLS: &str = "slug, name, description, body, built_in, created_at, updated_at";

fn select_skills() -> String {
    format!("SELECT {SKILL_COLS} FROM skills")
}

fn validate_slug(slug: &str) -> Result<(), WorkerError> {
    if slug.is_empty() || slug.len() > 64 {
        return Err(WorkerError::BadRequest(
            "slug must be 1..64 characters".into(),
        ));
    }
    if slug.starts_with('.') || slug.contains('/') || slug.contains("..") {
        return Err(WorkerError::BadRequest(
            "slug must not contain path components".into(),
        ));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(WorkerError::BadRequest(
            "slug must contain only ASCII letters, digits, '-', '_'".into(),
        ));
    }
    Ok(())
}

fn validate_create(body: &CreateSkill) -> Result<(), WorkerError> {
    validate_slug(&body.slug)?;
    if body.name.is_empty() || body.name.len() > 100 {
        return Err(WorkerError::BadRequest(
            "name must be 1..100 characters".into(),
        ));
    }
    if body.description.len() > 500 {
        return Err(WorkerError::BadRequest(
            "description must be at most 500 characters".into(),
        ));
    }
    if body.body.is_empty() || body.body.len() > 64 * 1024 {
        return Err(WorkerError::BadRequest(
            "body must be 1..65536 characters".into(),
        ));
    }
    Ok(())
}

pub async fn list(_req: Request, env: Env) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let stmt = database.prepare(format!(
        "{select} ORDER BY created_at DESC",
        select = select_skills()
    ));
    let rows: Vec<SkillRow> = safe_all(&stmt).await?;
    json_ok(&rows)
}

pub async fn create(mut req: Request, env: Env) -> Result<Response, WorkerError> {
    let body: CreateSkill = parse_json(&mut req).await?;
    validate_create(&body)?;
    if body.built_in == Some(true) && !crate::handlers::auth::is_root(&req, &env)? {
        return Err(WorkerError::Unauthorized);
    }
    let database = db(&env)?;
    match select_one(&database, &body.slug).await {
        Err(WorkerError::NotFound(_)) => {}
        Ok(_) => {
            return Err(WorkerError::Conflict(format!(
                "skill {} already exists",
                body.slug
            )));
        }
        Err(e) => return Err(e),
    }
    let built_in = body.built_in.unwrap_or(false);

    let stmt = database.prepare(
        "INSERT INTO skills (slug, name, description, body, built_in, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
    );
    stmt.bind(&[
        JsValue::from_str(&body.slug),
        JsValue::from_str(&body.name),
        JsValue::from_str(&body.description),
        JsValue::from_str(&body.body),
        JsValue::from_bool(built_in),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;

    let row = select_one(&database, &body.slug).await?;
    json_created(&row)
}

pub async fn get(_req: Request, env: Env, slug: &str) -> Result<Response, WorkerError> {
    validate_slug(slug)?;
    let database = db(&env)?;
    let row = select_one(&database, slug).await?;
    json_ok(&row)
}

pub async fn update(mut req: Request, env: Env, slug: &str) -> Result<Response, WorkerError> {
    let body: UpdateSkill = parse_json(&mut req).await?;
    validate_slug(slug)?;
    if body
        .name
        .as_ref()
        .is_some_and(|n| n.is_empty() || n.len() > 100)
    {
        return Err(WorkerError::BadRequest(
            "name must be 1..100 characters".into(),
        ));
    }
    if body.description.as_ref().is_some_and(|d| d.len() > 500) {
        return Err(WorkerError::BadRequest(
            "description must be at most 500 characters".into(),
        ));
    }
    if body
        .body
        .as_ref()
        .is_some_and(|b| b.is_empty() || b.len() > 64 * 1024)
    {
        return Err(WorkerError::BadRequest("body length is invalid".into()));
    }

    let database = db(&env)?;
    let existing = select_one(&database, slug).await?;
    if existing.built_in {
        return Err(WorkerError::Conflict(format!(
            "built-in skill {slug} cannot be edited"
        )));
    }

    let stmt = database.prepare(
        "UPDATE skills SET name=COALESCE(?1,name), description=COALESCE(?2,description), body=COALESCE(?3,body), updated_at=strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE slug = ?4",
    );
    stmt.bind(&[
        body.name
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.description
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.body
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        JsValue::from_str(slug),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;

    let row = select_one(&database, slug).await?;
    json_ok(&row)
}

pub async fn delete(_req: Request, env: Env, slug: &str) -> Result<Response, WorkerError> {
    validate_slug(slug)?;
    let database = db(&env)?;
    let existing = select_one(&database, slug).await?;
    if existing.built_in {
        return Err(WorkerError::Conflict(format!(
            "built-in skill {slug} cannot be deleted"
        )));
    }
    let stmt = database.prepare("DELETE FROM skills WHERE slug = ?1");
    stmt.bind(&[JsValue::from_str(slug)])?
        .run()
        .await
        .map_err(WorkerError::Worker)?;
    Ok(Response::empty()?)
}

async fn select_one(database: &worker::D1Database, slug: &str) -> Result<SkillRow, WorkerError> {
    let stmt = database.prepare(format!(
        "{select} WHERE slug = ?1",
        select = select_skills()
    ));
    let row: Option<SkillRow> = stmt
        .bind(&[JsValue::from_str(slug)])?
        .first(None)
        .await
        .map_err(WorkerError::Worker)?;
    row.ok_or_else(|| WorkerError::NotFound(format!("skill {slug} not found")))
}
