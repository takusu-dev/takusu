use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use takusu_storage::{CreateSkill, SkillRow, UpdateSkill};

use crate::error::HttpError;
use crate::state::AppState;

pub async fn create_skill(
    State(state): State<AppState>,
    Extension(token): Extension<String>,
    Json(body): Json<CreateSkill>,
) -> Result<(StatusCode, Json<SkillRow>), HttpError> {
    let skill = state.app.create_skill(&body, &token).await?;
    Ok((StatusCode::CREATED, Json(skill)))
}

pub async fn list_skills(State(state): State<AppState>) -> Result<Json<Vec<SkillRow>>, HttpError> {
    let skills = state.app.list_skills().await?;
    Ok(Json(skills))
}

pub async fn get_skill(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<SkillRow>, HttpError> {
    let skill = state.app.get_skill(&slug).await?;
    Ok(Json(skill))
}

pub async fn update_skill(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(body): Json<UpdateSkill>,
) -> Result<Json<SkillRow>, HttpError> {
    let skill = state.app.update_skill(&slug, &body).await?;
    Ok(Json(skill))
}

pub async fn delete_skill(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<StatusCode, HttpError> {
    state.app.delete_skill(&slug).await?;
    Ok(StatusCode::NO_CONTENT)
}
