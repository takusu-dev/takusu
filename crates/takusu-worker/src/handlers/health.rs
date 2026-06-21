use worker::Response;

pub fn health() -> Response {
    let mut resp = Response::ok("ok").expect("ok body is valid");
    resp.headers_mut()
        .set("content-type", "text/plain")
        .expect("valid header");
    resp
}
