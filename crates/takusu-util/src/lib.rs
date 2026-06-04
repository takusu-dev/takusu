use uuid::Uuid;

pub fn generate_root_token() -> String {
    format!("tsk_{}", Uuid::now_v7())
}
