//! The route table of `Phera\Http\Server`: a radix tree over URL paths,
//! stored as flat arrays.
//!
//! ```php
//! $server->get('/users/{id}', $show);        // one path segment, named
//! $server->get('/users/new', $form);         // static beats {param}
//! $server->get('/assets/*path', $files);     // the rest of the path
//! ```
//!
//! Every node lives in one `Vec<Node>` and points at its children by `u32`
//! index; the static text of all nodes lives in one `Vec<u8>`, and a node
//! only keeps a range of it. Splitting a node while routes are added only
//! splits its range, it copies no bytes. A lookup walks the path once,
//! byte by byte, with no hash lookups and no heap allocation: the values
//! of `{param}`s are byte ranges of the path, kept inline.
//!
//! Priority at every node: a static child first, then a `{param}`, then a
//! `*wildcard`. When the more specific branch fails further down (or has no
//! route for the request's method), the lookup backtracks and tries the
//! next one, so `/users/new` and `/users/{id}` can both be routes.

use smallvec::SmallVec;

/// The index of a route, given by [`Router::add`] in the order routes are
/// added. The caller keeps its handlers in a `Vec` at the same index.
pub type RouteId = u32;

/// The index of a node in [`Router::nodes`].
type NodeId = u32;

/// "No node" and "no route": the tree has fewer than 4 billion of either.
const NONE: u32 = u32::MAX;

/// Inline room for the parameters of one request: routes with more than
/// four `{param}`s spill to the heap, which no common route does.
const INLINE_PARAMS: usize = 4;

/// The HTTP methods a route can be registered for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Connect,
    Trace,
}

impl Method {
    const ALL: [Method; 9] = [
        Method::Get,
        Method::Head,
        Method::Post,
        Method::Put,
        Method::Patch,
        Method::Delete,
        Method::Options,
        Method::Connect,
        Method::Trace,
    ];

    /// The method a request line names. Methods are case-sensitive in HTTP,
    /// so `get` is not `GET`.
    pub fn from_request(name: &str) -> Option<Method> {
        Method::ALL
            .into_iter()
            .find(|method| method.as_str() == name)
    }

    /// The method a route registration names: `route('post', ...)` is
    /// accepted as `POST`, since a PHP programmer means the same thing.
    pub fn from_registration(name: &str) -> Option<Method> {
        Method::ALL
            .into_iter()
            .find(|method| method.as_str().eq_ignore_ascii_case(name))
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Options => "OPTIONS",
            Method::Connect => "CONNECT",
            Method::Trace => "TRACE",
        }
    }

    /// The method's slot in [`Node::routes`].
    fn slot(self) -> usize {
        self as usize
    }
}

/// What a route answers: one method, or every method (`any()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodFilter {
    Only(Method),
    Any,
}

impl MethodFilter {
    /// The slot in [`Node::routes`]: one per method, then one for `any()`.
    fn slot(self) -> usize {
        match self {
            MethodFilter::Only(method) => method.slot(),
            MethodFilter::Any => ANY_SLOT,
        }
    }
}

const ANY_SLOT: usize = Method::ALL.len();
const SLOTS: usize = ANY_SLOT + 1;

/// A set of methods, one bit per [`MethodFilter`] slot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MethodSet(u16);

impl MethodSet {
    fn insert(&mut self, slot: usize) {
        self.0 |= 1 << slot;
    }

    fn union(self, other: MethodSet) -> MethodSet {
        MethodSet(self.0 | other.0)
    }

    fn contains_slot(self, slot: usize) -> bool {
        self.0 & (1 << slot) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The value of an `Allow` header: `GET, HEAD, POST`. A `GET` route
    /// answers `HEAD` too, so `HEAD` is listed with it.
    pub fn allow_header(self) -> String {
        let mut names: SmallVec<[&str; SLOTS]> = SmallVec::new();
        for method in Method::ALL {
            let listed = self.contains_slot(method.slot())
                || (method == Method::Head && self.contains_slot(Method::Get.slot()));
            if listed {
                names.push(method.as_str());
            }
        }
        names.join(", ")
    }
}

/// The values of a route's `{param}`s and `*wildcard`, in pattern order,
/// as byte ranges of the request path.
pub type ParamRanges = SmallVec<[(u32, u32); INLINE_PARAMS]>;

/// The result of [`Router::lookup`].
#[derive(Debug, PartialEq, Eq)]
pub enum Lookup {
    Found {
        route: RouteId,
        params: ParamRanges,
    },
    /// Some route matches the path, none the method: answer 405 with an
    /// `Allow` header.
    MethodNotAllowed {
        allowed: MethodSet,
    },
    NotFound,
}

/// Why a route could not be added.
#[derive(Debug, PartialEq, Eq)]
pub enum RouteError {
    /// The pattern is not a valid route pattern; the message says why.
    InvalidPattern(String),
    /// The same method and pattern were already registered.
    Duplicate,
}

/// One piece of a parsed pattern.
#[derive(Debug, PartialEq, Eq)]
enum Piece<'a> {
    Static(&'a [u8]),
    Param(&'a str),
    Wildcard(&'a str),
}

#[derive(Debug, Clone)]
struct Node {
    /// The literal bytes this node matches: `text_start..text_start +
    /// text_len` in [`Router::text`]. Empty for the root and for `{param}`
    /// and `*wildcard` nodes.
    text_start: u32,
    text_len: u32,
    /// The first byte of each static child, at the same index as the child
    /// in `static_children`: the scan touches only this dense byte list.
    static_first_bytes: SmallVec<[u8; 8]>,
    static_children: SmallVec<[NodeId; 8]>,
    /// The `{param}` child, matching one non-empty path segment, or NONE.
    param_child: NodeId,
    /// The `*wildcard` child, matching the rest of the path, or NONE.
    wildcard_child: NodeId,
    /// The route of each method slot ending at this node, or NONE.
    routes: [RouteId; SLOTS],
    /// Which slots of `routes` are set.
    methods: MethodSet,
}

impl Node {
    fn new(text_start: u32, text_len: u32) -> Node {
        Node {
            text_start,
            text_len,
            static_first_bytes: SmallVec::new(),
            static_children: SmallVec::new(),
            param_child: NONE,
            wildcard_child: NONE,
            routes: [NONE; SLOTS],
            methods: MethodSet::default(),
        }
    }
}

/// The route table. Build it with [`Router::add`], then [`Router::lookup`]
/// request paths.
#[derive(Debug, Clone)]
pub struct Router {
    /// Node 0 is the root.
    nodes: Vec<Node>,
    /// The static text of every node; see [`Node::text_start`].
    text: Vec<u8>,
    /// The parameter names of every route, back to back.
    param_names: Vec<Box<str>>,
    /// For each route: where its names start in `param_names`, and how many.
    route_names: Vec<(u32, u32)>,
}

impl Default for Router {
    fn default() -> Self {
        Router::new()
    }
}

impl Router {
    pub fn new() -> Router {
        Router {
            nodes: vec![Node::new(0, 0)],
            text: Vec::new(),
            param_names: Vec::new(),
            route_names: Vec::new(),
        }
    }

    /// The parameter names of `route`, in pattern order: the names of the
    /// ranges [`Lookup::Found`] gives.
    pub fn param_names(&self, route: RouteId) -> &[Box<str>] {
        let (start, count) = self.route_names[route as usize];
        &self.param_names[start as usize..(start + count) as usize]
    }

    /// Adds a route and returns its id: ids count up from 0.
    pub fn add(&mut self, method: MethodFilter, pattern: &str) -> Result<RouteId, RouteError> {
        let pieces = parse_pattern(pattern)?;
        let mut node = 0;
        let mut names: SmallVec<[&str; INLINE_PARAMS]> = SmallVec::new();
        for piece in &pieces {
            node = match *piece {
                Piece::Static(bytes) => self.insert_static(node, bytes),
                Piece::Param(name) => {
                    names.push(name);
                    self.param_child(node)
                }
                Piece::Wildcard(name) => {
                    names.push(name);
                    self.wildcard_child(node)
                }
            };
        }
        if names.len() > 1 {
            for (index, name) in names.iter().enumerate() {
                if names[..index].contains(name) {
                    return Err(RouteError::InvalidPattern(format!(
                        "parameter \"{name}\" appears twice"
                    )));
                }
            }
        }

        let slot = method.slot();
        let target = &mut self.nodes[node as usize];
        if target.routes[slot] != NONE {
            return Err(RouteError::Duplicate);
        }
        let route = self.route_names.len() as RouteId;
        target.routes[slot] = route;
        target.methods.insert(slot);

        let start = self.param_names.len() as u32;
        self.param_names
            .extend(names.iter().map(|name| Box::<str>::from(*name)));
        self.route_names.push((start, names.len() as u32));
        Ok(route)
    }

    /// Finds the route for a request. `path` is the raw path, without the
    /// query string.
    pub fn lookup(&self, method: Method, path: &str) -> Lookup {
        let mut search = Search {
            path: path.as_bytes(),
            method,
            params: ParamRanges::new(),
            allowed: MethodSet::default(),
        };
        match self.find(0, 0, &mut search) {
            Some(route) => Lookup::Found {
                route,
                params: search.params,
            },
            None if !search.allowed.is_empty() => Lookup::MethodNotAllowed {
                allowed: search.allowed,
            },
            None => Lookup::NotFound,
        }
    }

    /// Matches `node` against the path from `position`, then its children.
    /// Recursion depth is bounded by the number of nodes on one route.
    fn find(&self, node: NodeId, position: usize, search: &mut Search<'_>) -> Option<RouteId> {
        let current = &self.nodes[node as usize];
        let text = self.node_text(current);
        let rest = &search.path[position..];
        if !rest.starts_with(text) {
            return None;
        }
        let position = position + text.len();

        if position == search.path.len() {
            if let Some(route) = search.accept(current) {
                return Some(route);
            }
            // `/files/*path` also matches `/files/`, with an empty path.
            return self.find_wildcard(current, position, search);
        }

        let next = search.path[position];
        if let Some(index) = current
            .static_first_bytes
            .iter()
            .position(|&first| first == next)
            && let Some(route) = self.find(current.static_children[index], position, search)
        {
            return Some(route);
        }

        if current.param_child != NONE {
            let segment_end = search.path[position..]
                .iter()
                .position(|&byte| byte == b'/')
                .map_or(search.path.len(), |offset| position + offset);
            if segment_end > position {
                search.params.push((position as u32, segment_end as u32));
                if let Some(route) = self.find(current.param_child, segment_end, search) {
                    return Some(route);
                }
                search.params.pop();
            }
        }

        self.find_wildcard(current, position, search)
    }

    fn find_wildcard(
        &self,
        node: &Node,
        position: usize,
        search: &mut Search<'_>,
    ) -> Option<RouteId> {
        if node.wildcard_child == NONE {
            return None;
        }
        let wildcard = &self.nodes[node.wildcard_child as usize];
        search
            .params
            .push((position as u32, search.path.len() as u32));
        let found = search.accept(wildcard);
        if found.is_none() {
            search.params.pop();
        }
        found
    }

    fn node_text(&self, node: &Node) -> &[u8] {
        let start = node.text_start as usize;
        &self.text[start..start + node.text_len as usize]
    }

    /// Walks or extends the static path `bytes` below `node` and returns the
    /// node it ends at, splitting a node whose text only partly matches.
    fn insert_static(&mut self, node: NodeId, bytes: &[u8]) -> NodeId {
        let mut node = node;
        let mut bytes = bytes;
        while let Some(&first) = bytes.first() {
            let parent = &self.nodes[node as usize];
            let Some(index) = parent
                .static_first_bytes
                .iter()
                .position(|&byte| byte == first)
            else {
                let child = self.push_node(bytes);
                let parent = &mut self.nodes[node as usize];
                parent.static_first_bytes.push(first);
                parent.static_children.push(child);
                return child;
            };
            let child = parent.static_children[index];
            let child_text = self.node_text(&self.nodes[child as usize]);
            let common = child_text
                .iter()
                .zip(bytes)
                .take_while(|(left, right)| left == right)
                .count();
            if common < child_text.len() {
                self.split(node, index, common as u32);
            }
            node = self.nodes[node as usize].static_children[index];
            bytes = &bytes[common..];
        }
        node
    }

    /// Splits the static child at `index` of `parent` after its first
    /// `at` bytes: a new node takes those bytes and the old child, with the
    /// rest of its text, becomes the new node's only child.
    fn split(&mut self, parent: NodeId, index: usize, at: u32) {
        let child = self.nodes[parent as usize].static_children[index];
        let (start, length) = {
            let old = &self.nodes[child as usize];
            (old.text_start, old.text_len)
        };
        let middle = self.nodes.len() as NodeId;
        let mut middle_node = Node::new(start, at);
        middle_node
            .static_first_bytes
            .push(self.text[(start + at) as usize]);
        middle_node.static_children.push(child);
        self.nodes.push(middle_node);

        let old = &mut self.nodes[child as usize];
        old.text_start = start + at;
        old.text_len = length - at;
        self.nodes[parent as usize].static_children[index] = middle;
    }

    fn push_node(&mut self, text: &[u8]) -> NodeId {
        let start = self.text.len() as u32;
        self.text.extend_from_slice(text);
        let id = self.nodes.len() as NodeId;
        self.nodes.push(Node::new(start, text.len() as u32));
        id
    }

    fn param_child(&mut self, node: NodeId) -> NodeId {
        match self.nodes[node as usize].param_child {
            NONE => {
                let child = self.push_node(b"");
                self.nodes[node as usize].param_child = child;
                child
            }
            existing => existing,
        }
    }

    fn wildcard_child(&mut self, node: NodeId) -> NodeId {
        match self.nodes[node as usize].wildcard_child {
            NONE => {
                let child = self.push_node(b"");
                self.nodes[node as usize].wildcard_child = child;
                child
            }
            existing => existing,
        }
    }
}

/// The state of one lookup.
struct Search<'a> {
    path: &'a [u8],
    method: Method,
    params: ParamRanges,
    /// The methods of every node the path reached without one for
    /// `method`, for a 405 answer.
    allowed: MethodSet,
}

impl Search<'_> {
    /// The route `node` has for the request's method: its own, else the
    /// `any()` route, else (for `HEAD`) the `GET` route. A node with routes
    /// for other methods only adds them to `allowed`.
    fn accept(&mut self, node: &Node) -> Option<RouteId> {
        if node.methods.is_empty() {
            return None;
        }
        let mut route = node.routes[self.method.slot()];
        if route == NONE {
            route = node.routes[ANY_SLOT];
        }
        if route == NONE && self.method == Method::Head {
            route = node.routes[Method::Get.slot()];
        }
        if route == NONE {
            self.allowed = self.allowed.union(node.methods);
            return None;
        }
        Some(route)
    }
}

/// Splits a pattern such as `/users/{id}/files/*path` into its pieces.
fn parse_pattern(pattern: &str) -> Result<Vec<Piece<'_>>, RouteError> {
    let invalid = |reason: &str| {
        RouteError::InvalidPattern(format!("invalid route pattern \"{pattern}\": {reason}"))
    };
    if !pattern.starts_with('/') {
        return Err(invalid("it must start with \"/\""));
    }
    let bytes = pattern.as_bytes();
    let mut pieces = Vec::new();
    let mut static_start = 0;
    let mut position = 0;
    while position < bytes.len() {
        match bytes[position] {
            b'{' => {
                if bytes[position - 1] != b'/' {
                    return Err(invalid("a {parameter} must be a whole path segment"));
                }
                let Some(length) = pattern[position..].find('}') else {
                    return Err(invalid("\"{\" is not closed"));
                };
                let name = &pattern[position + 1..position + length];
                if !is_parameter_name(name) {
                    return Err(invalid(
                        "a parameter name must be letters, digits and underscores",
                    ));
                }
                let end = position + length + 1;
                if end < bytes.len() && bytes[end] != b'/' {
                    return Err(invalid("a {parameter} must be a whole path segment"));
                }
                pieces.push(Piece::Static(&bytes[static_start..position]));
                pieces.push(Piece::Param(name));
                position = end;
                static_start = end;
            }
            b'*' => {
                if bytes[position - 1] != b'/' {
                    return Err(invalid("a *wildcard must start a path segment"));
                }
                let name = &pattern[position + 1..];
                let name = if name.is_empty() { "*" } else { name };
                if name != "*" && !is_parameter_name(name) {
                    return Err(invalid("a *wildcard must be the end of the pattern"));
                }
                pieces.push(Piece::Static(&bytes[static_start..position]));
                pieces.push(Piece::Wildcard(name));
                position = bytes.len();
                static_start = position;
            }
            b'}' => return Err(invalid("\"}\" without \"{\"")),
            _ => position += 1,
        }
    }
    pieces.push(Piece::Static(&bytes[static_start..]));
    pieces.retain(|piece| !matches!(piece, Piece::Static(bytes) if bytes.is_empty()));
    Ok(pieces)
}

fn is_parameter_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router(routes: &[(MethodFilter, &str)]) -> Router {
        let mut router = Router::new();
        for (method, pattern) in routes {
            router.add(*method, pattern).expect(pattern);
        }
        router
    }

    fn get(pattern: &str) -> (MethodFilter, &str) {
        (MethodFilter::Only(Method::Get), pattern)
    }

    fn names(router: &Router, route: RouteId) -> Vec<&str> {
        router
            .param_names(route)
            .iter()
            .map(|name| &**name)
            .collect()
    }

    /// The route id and the parameter values a lookup found.
    fn found(router: &Router, method: Method, path: &str) -> Option<(RouteId, Vec<String>)> {
        match router.lookup(method, path) {
            Lookup::Found { route, params } => Some((
                route,
                params
                    .iter()
                    .map(|&(start, end)| path[start as usize..end as usize].to_owned())
                    .collect(),
            )),
            _ => None,
        }
    }

    #[test]
    fn static_routes_match_exactly() {
        let router = router(&[get("/"), get("/plaintext"), get("/plain"), get("/json")]);
        assert_eq!(found(&router, Method::Get, "/"), Some((0, vec![])));
        assert_eq!(found(&router, Method::Get, "/plaintext"), Some((1, vec![])));
        assert_eq!(found(&router, Method::Get, "/plain"), Some((2, vec![])));
        assert_eq!(found(&router, Method::Get, "/json"), Some((3, vec![])));
        assert_eq!(router.lookup(Method::Get, "/pla"), Lookup::NotFound);
        assert_eq!(router.lookup(Method::Get, "/plaintext/"), Lookup::NotFound);
        assert_eq!(router.lookup(Method::Get, "/jso"), Lookup::NotFound);
        assert_eq!(router.lookup(Method::Get, ""), Lookup::NotFound);
    }

    #[test]
    fn splitting_keeps_every_route() {
        // Each insert splits the node the previous one made.
        let router = router(&[
            get("/romane"),
            get("/romanus"),
            get("/romulus"),
            get("/rubens"),
            get("/ruber"),
            get("/rubicon"),
            get("/rubicundus"),
            get("/r"),
        ]);
        for (index, path) in [
            "/romane",
            "/romanus",
            "/romulus",
            "/rubens",
            "/ruber",
            "/rubicon",
            "/rubicundus",
            "/r",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                found(&router, Method::Get, path),
                Some((index as RouteId, vec![])),
                "{path}"
            );
        }
        assert_eq!(router.lookup(Method::Get, "/rom"), Lookup::NotFound);
        assert_eq!(router.lookup(Method::Get, "/rubic"), Lookup::NotFound);
    }

    #[test]
    fn params_match_one_segment() {
        let router = router(&[get("/users/{id}"), get("/users/{id}/posts/{post}")]);
        assert_eq!(
            found(&router, Method::Get, "/users/42"),
            Some((0, vec!["42".into()]))
        );
        assert_eq!(
            found(&router, Method::Get, "/users/42/posts/7"),
            Some((1, vec!["42".into(), "7".into()]))
        );
        assert_eq!(names(&router, 1), ["id", "post"]);
        // An empty segment is no value.
        assert_eq!(router.lookup(Method::Get, "/users/"), Lookup::NotFound);
        assert_eq!(router.lookup(Method::Get, "/users/42/"), Lookup::NotFound);
        assert_eq!(
            router.lookup(Method::Get, "/users/42/posts"),
            Lookup::NotFound
        );
    }

    #[test]
    fn routes_sharing_a_param_node_keep_their_own_names() {
        let router = router(&[get("/users/{id}"), get("/users/{name}/profile")]);
        assert_eq!(names(&router, 0), ["id"]);
        assert_eq!(names(&router, 1), ["name"]);
        assert_eq!(
            found(&router, Method::Get, "/users/ann/profile"),
            Some((1, vec!["ann".into()]))
        );
    }

    #[test]
    fn static_beats_param_beats_wildcard() {
        let router = router(&[get("/users/{id}"), get("/users/new"), get("/users/*rest")]);
        assert_eq!(found(&router, Method::Get, "/users/new"), Some((1, vec![])));
        assert_eq!(
            found(&router, Method::Get, "/users/newer"),
            Some((0, vec!["newer".into()]))
        );
        assert_eq!(
            found(&router, Method::Get, "/users/7"),
            Some((0, vec!["7".into()]))
        );
        assert_eq!(
            found(&router, Method::Get, "/users/7/edit"),
            Some((2, vec!["7/edit".into()]))
        );
    }

    #[test]
    fn a_failed_static_branch_backtracks_to_the_param() {
        let router = router(&[get("/a/b/c"), get("/a/{x}/d")]);
        assert_eq!(
            found(&router, Method::Get, "/a/b/d"),
            Some((1, vec!["b".into()]))
        );
        assert_eq!(found(&router, Method::Get, "/a/b/c"), Some((0, vec![])));
    }

    #[test]
    fn wildcard_takes_the_rest_including_slashes_and_nothing() {
        let router = router(&[get("/static/*path"), get("/*")]);
        assert_eq!(
            found(&router, Method::Get, "/static/css/app.css"),
            Some((0, vec!["css/app.css".into()]))
        );
        assert_eq!(
            found(&router, Method::Get, "/static/"),
            Some((0, vec!["".into()]))
        );
        assert_eq!(
            found(&router, Method::Get, "/anything/else"),
            Some((1, vec!["anything/else".into()]))
        );
        assert_eq!(names(&router, 1), ["*"]);
    }

    #[test]
    fn method_dispatch_and_405() {
        let router = router(&[
            get("/items"),
            (MethodFilter::Only(Method::Post), "/items"),
            (MethodFilter::Only(Method::Delete), "/items/{id}"),
        ]);
        assert_eq!(found(&router, Method::Get, "/items"), Some((0, vec![])));
        assert_eq!(found(&router, Method::Post, "/items"), Some((1, vec![])));
        let Lookup::MethodNotAllowed { allowed } = router.lookup(Method::Put, "/items") else {
            panic!("expected 405");
        };
        assert_eq!(allowed.allow_header(), "GET, HEAD, POST");
        let Lookup::MethodNotAllowed { allowed } = router.lookup(Method::Get, "/items/3") else {
            panic!("expected 405");
        };
        assert_eq!(allowed.allow_header(), "DELETE");
        assert_eq!(router.lookup(Method::Get, "/nothing"), Lookup::NotFound);
    }

    #[test]
    fn head_falls_back_to_get_and_any_matches_every_method() {
        let router = router(&[
            get("/page"),
            (MethodFilter::Any, "/hook"),
            (MethodFilter::Only(Method::Head), "/own-head"),
            get("/own-head"),
        ]);
        assert_eq!(found(&router, Method::Head, "/page"), Some((0, vec![])));
        assert_eq!(found(&router, Method::Patch, "/hook"), Some((1, vec![])));
        assert_eq!(found(&router, Method::Get, "/hook"), Some((1, vec![])));
        assert_eq!(found(&router, Method::Head, "/own-head"), Some((2, vec![])));
        assert_eq!(found(&router, Method::Get, "/own-head"), Some((3, vec![])));
    }

    #[test]
    fn a_method_mismatch_on_a_specific_route_falls_back_to_a_general_one() {
        let router = router(&[
            (MethodFilter::Only(Method::Post), "/users/new"),
            get("/users/{id}"),
        ]);
        // `/users/new` exists for POST only; GET takes the param route.
        assert_eq!(
            found(&router, Method::Get, "/users/new"),
            Some((1, vec!["new".into()]))
        );
        assert_eq!(
            found(&router, Method::Post, "/users/new"),
            Some((0, vec![]))
        );
        let Lookup::MethodNotAllowed { allowed } = router.lookup(Method::Put, "/users/new") else {
            panic!("expected 405");
        };
        assert_eq!(allowed.allow_header(), "GET, HEAD, POST");
    }

    #[test]
    fn duplicates_are_refused() {
        let mut router = Router::new();
        router
            .add(MethodFilter::Only(Method::Get), "/users/{id}")
            .unwrap();
        assert_eq!(
            router.add(MethodFilter::Only(Method::Get), "/users/{other}"),
            Err(RouteError::Duplicate)
        );
        // The refused route took no id.
        assert_eq!(
            router.add(MethodFilter::Only(Method::Post), "/users/{id}"),
            Ok(1)
        );
    }

    #[test]
    fn invalid_patterns_are_refused() {
        for pattern in [
            "",
            "users",
            "/users/{",
            "/users/{}",
            "/users/{id",
            "/users/x{id}",
            "/users/{id}x",
            "/users/{a-b}",
            "/users/}",
            "/files/*path/more",
            "/files/x*",
            "/a/{id}/b/{id}",
        ] {
            assert!(
                matches!(
                    Router::new().add(MethodFilter::Any, pattern),
                    Err(RouteError::InvalidPattern(_))
                ),
                "{pattern:?}"
            );
        }
    }

    #[test]
    fn methods_parse_from_requests_and_registrations() {
        assert_eq!(Method::from_request("GET"), Some(Method::Get));
        assert_eq!(Method::from_request("get"), None);
        assert_eq!(Method::from_request("PROPFIND"), None);
        assert_eq!(Method::from_registration("patch"), Some(Method::Patch));
    }

    #[test]
    fn many_params_spill_but_still_match() {
        let router = router(&[get("/{a}/{b}/{c}/{d}/{e}/{f}")]);
        assert_eq!(
            found(&router, Method::Get, "/1/2/3/4/5/6"),
            Some((0, ["1", "2", "3", "4", "5", "6"].map(String::from).to_vec()))
        );
    }

    #[test]
    fn non_ascii_and_encoded_paths_match_bytewise() {
        let router = router(&[get("/café/{name}")]);
        assert_eq!(
            found(&router, Method::Get, "/café/b%20c"),
            Some((0, vec!["b%20c".into()]))
        );
        assert_eq!(router.lookup(Method::Get, "/caf%C3%A9/x"), Lookup::NotFound);
    }
}
