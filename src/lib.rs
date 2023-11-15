use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::fs;
use std::io::{prelude::*, Read};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use threads::ThreadPool;

mod threads;

pub struct Router {
    host: String,
    routes: Vec<Route>,
}

impl Router {
    /// # Examples
    ///
    /// ```
    /// use http_library::Router;
    ///
    /// let mut r = Router::new("127.0.0.1:12345");
    /// ```
    pub fn new(addr: &str) -> Router {
        Router {
            routes: vec![],
            host: addr.to_owned(),
        }
    }

    /// Generates new route and adds to router
    ///
    /// Routes are matched in the order they are added
    ///
    /// # Examples
    ///
    /// ```
    /// use http_library::{Router, Request, Response};
    ///
    /// let mut r = Router::new("127.0.0.1:12345");
    ///
    /// r.handle_func("/hi", test, vec!["GET"]);
    ///
    /// // Wildcard
    /// r.handle_func("/te:?", test, vec!["GET"]);
    /// r.handle_func("/test", test, vec!["GET"]); // never reached because of wildcard
    ///
    /// fn test(_req: &Request) -> Response {
    ///     Response::new(200, "hi")
    /// }
    /// ```
    pub fn handle_func(&mut self, path: &str, handler: Handler, methods: Vec<&str>) {
        let route = Route {
            path: path.to_owned(),
            methods: methods
                .into_iter()
                .map(|x| x.to_owned())
                .collect::<Vec<String>>(),
            handler,
        };

        self.routes.push(route);
    }

    /// Runs Tcp Server on specified port
    ///
    /// # Example
    ///
    /// ```
    /// use http_library::{Router, Request, Response};
    ///
    /// fn run() {
    ///     let mut r = Router::new("127.0.0.1:8000");
    ///
    ///     r.handle_func("/", home, vec!["GET"]);
    ///     r.serve().unwrap();
    /// }
    ///
    /// fn home(r: &Request) -> Response {
    ///     Response::new(200, "hi")
    /// }
    /// ```
    pub fn serve(&self) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(self.host.clone()).unwrap();
        let routes = Arc::new(self.routes.to_vec());
        let pool = ThreadPool::build(4).unwrap();

        for stream in listener.incoming() {
            let stream = stream.unwrap();
            let routes = Arc::clone(&routes);

            pool.execute(move || {
                handle_connection(stream, Arc::clone(&routes));
            });
        }

        Ok(())
    }
}

fn handle_connection(mut stream: TcpStream, routes: Arc<Vec<Route>>) {
    let mut buf = [0; 4096];
    let n = stream.read(&mut buf).unwrap();
    if n == 0 {
        // todo: Return err
    }

    let req = match Request::from_utf8(&mut buf[0..n]) {
        Ok(v) => v,
        Err(e) => panic!("{}", e),
    };

    println!("-> {}", req.path);

    let handler: Handler = match Route::match_route(&routes, req.path.as_str()) {
        Some(route) => {
            if route.methods.contains(&req.method) {
                route.handler
            } else {
                method_not_allowed_handler
            }
        }
        None => not_found_handler,
    };

    let res = handler(&req);
    let output = format!(
        "HTTP/1.1 {} {}\r\n{}",
        res.code,
        if res.code == 200 { "OK" } else { " " },
        res.to_string()
    );

    stream.write_all(output.as_bytes()).unwrap();
    stream.flush().unwrap();
}

fn method_not_allowed_handler(_req: &Request) -> Response {
    Response::new(405, "method not allowed")
}

fn not_found_handler(_req: &Request) -> Response {
    Response::new(404, "page not found")
}

#[derive(Debug, Clone)]
struct Route {
    path: String,
    methods: Vec<String>,
    handler: Handler,
}

impl Route {
    fn match_route<'a>(routes: &'a Vec<Route>, path: &str) -> Option<&'a Route> {
        routes.iter().find(|r| {
            if r.path.contains(":?") {
                let prefix = r
                    .path
                    .strip_suffix(":?")
                    .expect("wildcard ':?' must be at the end");
                path.starts_with(prefix)
            } else {
                r.path == path
            }
        })
    }
}

#[derive(Debug)]
pub struct Request {
    pub path: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl Request {
    fn from_utf8(data: &[u8]) -> Result<Request, &'static str> {
        let data = match String::from_utf8(data.to_vec()) {
            Ok(v) => v,
            Err(_) => return Err("error converting request bytes to string"),
        };

        Request::parse(data)
    }

    fn parse(data: String) -> Result<Request, &'static str> {
        let data = data.replace("\0", "");
        let mut lines = data.split("\r\n");

        let line = match lines.next() {
            Some(v) => v,
            None => return Err("invalid http data"),
        };

        let line: Vec<&str> = line.split(" ").collect();

        let method = match line.get(0) {
            Some(v) => v.to_string(),
            None => return Err("missing method in request"),
        };
        let path = match line.get(1) {
            Some(v) => v.to_string(),
            None => return Err("missing path in request"),
        };

        let mut headers = HashMap::new();
        for line in lines {
            if let Some((k, v)) = line.split_once(": ") {
                headers.insert(k.to_string(), v.to_string());
            }
        }

        let data: Vec<&str> = data.split("\r\n").collect();
        Ok(Request {
            method,
            path,
            headers,
            body: data[data.len() - 1].to_string(),
        })
    }
}

pub type Handler = fn(&Request) -> Response;

struct Json<K, V>(HashMap<K, V>);

impl<K, V> Display for Json<K, V>
where
    K: Display,
    V: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut string = String::from("{");

        for (i, (k, v)) in self.0.iter().enumerate() {
            string.push_str(&format!("\"{}\": \"{}\"", k, v));
            if i != (self.0.len() - 1) {
                string.push(',');
            }
        }

        string.push('}');
        write!(f, "{}", string)
    }
}

pub type ResponseData = Box<dyn Display + Send + 'static>;

pub struct Response {
    code: u16,
    data: Option<ResponseData>,
    headers: HashMap<String, String>,
}

impl Response {
    /// Returns new Response
    ///
    /// # Example
    ///
    /// ```
    /// use http_library::{Response, Request};
    ///
    /// fn test(_req: &Request) -> Response {
    ///     Response::new(200, "hi")
    /// }
    /// ```
    pub fn new(code: u16, data: impl Display + Send + 'static) -> Response {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_owned(), "text/plain".to_owned());
        headers.insert(
            "Content-Length".to_owned(),
            data.to_string().len().to_string(),
        );

        Response {
            code,
            data: Some(Box::new(data)),
            headers,
        }
    }

    /// Returns new response with no data
    ///
    /// # Example
    ///
    /// ```
    /// use http_library::{Request, Response};
    ///
    /// fn test(_req: &Request) -> Response {
    ///     Response::empty(200)
    /// }
    /// ```
    pub fn empty(code: u16) -> Response {
        Response {
            code,
            data: None,
            headers: HashMap::new(),
        }
    }

    /// Returns new json response
    ///
    /// # Example
    ///
    /// ```
    /// use http_library::{Request, Response};
    /// use std::collections::HashMap;
    ///
    /// fn test(_req: &Request) -> Response {
    ///     let mut data = HashMap::new();
    ///     data.insert("foo", "bar");
    ///
    ///     Response::json(200, data)
    /// }
    /// ```
    pub fn json<K, V>(code: u16, data: HashMap<K, V>) -> Response
    where
        K: Display + Send + 'static,
        V: Display + Send + 'static,
    {
        Response {
            code,
            data: Some(Box::new(Json(data))),
            headers: HashMap::new(),
        }
        .add_header("Content-Type", "application/json")
    }

    /// Returns response containing file
    ///
    /// # Example
    ///
    /// ```
    /// use http_library::{Request, Response};
    ///
    /// fn test(_req: &Request) -> Response {
    ///     Response::file(200, "templates/index.html")
    /// }
    /// ```
    pub fn file(code: u16, path: &str) -> Response {
        let contents = fs::read_to_string(path).unwrap();

        Response {
            code,
            data: Some(Box::new(contents)),
            headers: HashMap::new(),
        }
        .add_header("Content-Type", "text/html")
    }

    /// Returns new response with specified headers
    ///
    /// # Example
    ///
    /// ```
    /// use http_library::{Request, Response};
    ///
    /// fn test(_req: &Request) -> Response {
    ///     Response::empty(200).add_header("foo", "bar")
    /// }
    /// ```
    pub fn add_header(mut self, key: &str, val: &str) -> Response {
        self.headers.insert(key.to_owned(), val.to_owned());
        self
    }

    /// Adds headers to current response with specified headers
    ///
    /// Handy when adding multiple headers
    ///
    /// # Example
    ///
    /// ```
    /// use http_library::{Request, Response};
    ///
    /// fn test(_req: &Request) -> Response {
    ///     let mut res = Response::empty(200);
    ///
    ///     res.add_headers("foo", "bar");
    ///     res.add_headers("foo2", "bar");
    ///     res
    /// }
    /// ```
    pub fn add_headers(&mut self, key: &str, val: &str) {
        self.headers.insert(key.to_owned(), val.to_owned());
    }

    fn to_string(&self) -> String {
        let mut output = String::new();
        for (key, val) in self.headers.iter() {
            output.push_str(&format!("{key}: {val}\r\n"));
        }

        if self.headers.len() != 0 {
            output.push_str("\r\n")
        };

        if let Some(ref data) = self.data {
            output.push_str(&data.to_string());
        }

        output.push_str("\r\n");
        format!("{}", output)
    }
}
