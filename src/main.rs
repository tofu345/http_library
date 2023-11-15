use std::{collections::HashMap, env, fs};

use http_library::{Request, Response, Router};

fn main() {
    let port = "127.0.0.1:4221";
    let mut r = Router::new(port);

    r.handle_func("/", base_handler, vec!["GET"]);
    r.handle_func("/echo/:?", echo_handler, vec!["GET"]);
    r.handle_func("/user-agent", user_agent_handler, vec!["GET"]);
    r.handle_func("/files/:?", files_handler, vec!["GET", "POST"]);
    r.handle_func("/json", json_handler, vec!["GET"]);

    println!("Listening on port {}", port);
    if let Err(e) = r.serve() {
        eprintln!("Err: {}", e);
    };
}

fn json_handler(_req: &Request) -> Response {
    let mut data = HashMap::new();
    data.insert("foo", "bar");

    Response::json(200, data)
}

fn base_handler(_req: &Request) -> Response {
    Response::file(200, "index.html")
}

fn echo_handler(req: &Request) -> Response {
    let x = req.path.strip_prefix("/echo/").unwrap().to_owned();

    Response::new(200, x)
}

fn user_agent_handler(req: &Request) -> Response {
    let agent = req.headers.get("User-Agent").unwrap().to_owned();

    Response::new(200, agent)
}

fn files_handler(req: &Request) -> Response {
    let filename = req.path.strip_prefix("/files/").unwrap();
    let args: Vec<String> = env::args().collect();
    let directory = env::current_dir()
        .unwrap()
        .join(&args.get(2).expect("missing directory param"));
    let file_path = directory.join(filename);
    let contents = fs::read_to_string(file_path.clone());

    if req.method == "POST" {
        fs::write(file_path, req.body.clone()).expect("unable to write");
        return Response::empty(201);
    }

    if let Err(e) = contents {
        return Response::new(404, e);
    }

    let contents = contents.unwrap();
    Response::new(200, contents).add_header("Content-Type", "application/octet-stream")
}
