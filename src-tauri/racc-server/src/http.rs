use tower_http::services::ServeDir;

pub fn static_file_service(dist_path: &str) -> ServeDir {
    ServeDir::new(dist_path)
}
