//! Real-JS bridge for the experimental DOM backend.
//!
//! QuickJS contexts are `!Send` (a JSContext is bound to its creating thread),
//! but `PageOps: Send` demands `DomPage` be movable across the multi-thread
//! tokio runtime. Resolution: a dedicated OS thread owns the `quick_js::Context`
//! and the DOM page holds only an `mpsc` `SyncSender` (Send+Sync). Requests go
//! through `tokio::task::spawn_blocking` so a saturated channel never stalls an
//! async worker; replies come back over a per-call oneshot.
//!
//! This makes the DOM backend genuinely JS-capable (no Chrome): `eval_text`,
//! `wait_for_truthy`, inline `<script>`, `javascript:` hrefs, **and a live
//! `document` API** (`getElementById` with readable/writable `textContent` /
//! `value`) — all backed by the shared in-memory DOM tree the page serves
//! snapshots from, so JS mutations are visible to `snapshot`/`extract`/`fill`.
//!
//! Honest limits vs. the CDP backend: no layout, no event dispatch, no network.
//! `addEventListener`/form-submit-via-onSubmit are NOT implemented (scripts
//! calling them get a JS exception); the CDP backend is the choice for real
//! event-driven flows. (See AGENTS.md "Honest limits".)

use std::sync::Arc;
use std::sync::RwLock;
use std::thread;

use tokio::sync::oneshot;
use vakbrowse_core::{Result, VakError};

use crate::Node;

type DomTree = Arc<RwLock<Node>>;

/// One round-trip to the JS worker thread. The request payload is a `String`
/// (keeps the channel `Send` + cheap); `quick_js::JsValue` never crosses threads.
enum JsCmd {
    /// Evaluate `expr` and return the JS-REPL-stringified result (CDP parity:
    /// primitives stringify, objects/arrays -> JSON).
    EvalText(String, oneshot::Sender<Reply>),
    /// Evaluate a `javascript:`-URL body; reply is `"nav:<url>"` (non-empty
    /// string result => navigate) or `"stay"` (void / non-string).
    EvalHref(String, oneshot::Sender<Reply>),
    /// Set the reflective `location.href` and clear any pending nav target.
    SetLocation(String, oneshot::Sender<Reply>),
    /// Read+clear the pending nav target recorded by `location.href =` in a
    /// script (non-empty => a script redirected the page).
    CheckNav(oneshot::Sender<Reply>),
}

type Reply = std::result::Result<String, VakError>;

pub struct JsRuntime {
    sender: std::sync::mpsc::SyncSender<JsCmd>,
}

impl JsRuntime {
    pub fn new(dom: DomTree) -> Result<Self> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<JsCmd>(8);
        // Dedicated thread: the only place the `!Send` Context lives. It captures
        // a clone of the shared DOM tree so `document` callbacks can read/write
        // the same nodes `DomPage` serves snapshots from.
        thread::Builder::new()
            .name("vakbrowse-dom-js".into())
            .spawn(move || run_worker(rx, dom))
            .map_err(|e| VakError::Engine(format!("spawn js worker: {e}")))?;
        Ok(Self { sender: tx })
    }

    /// `eval_text`: real JS, stringified per CDP semantics.
    pub fn eval_text(
        &self,
        expr: &str,
    ) -> impl std::future::Future<Output = Result<String>> + Send + '_ {
        let (tx, rx) = oneshot::channel();
        let cmd = JsCmd::EvalText(expr.to_string(), tx);
        dispatch(&self.sender, rx, cmd)
    }

    /// `javascript:`-URL evaluation returning `"nav:<url>"` / `"stay"`.
    pub fn eval_href(
        &self,
        expr: &str,
    ) -> impl std::future::Future<Output = Result<String>> + Send + '_ {
        let (tx, rx) = oneshot::channel();
        let cmd = JsCmd::EvalHref(expr.to_string(), tx);
        dispatch(&self.sender, rx, cmd)
    }

    pub fn set_location(
        &self,
        url: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send + '_ {
        let (tx, rx) = oneshot::channel();
        let cmd = JsCmd::SetLocation(url.to_string(), tx);
        async move {
            let s: String = dispatch(&self.sender, rx, cmd).await?;
            debug_assert!(s.is_empty(), "set_location reply must be empty");
            Ok(())
        }
    }

    pub fn check_nav(&self) -> impl std::future::Future<Output = Result<String>> + Send + '_ {
        let (tx, rx) = oneshot::channel();
        let cmd = JsCmd::CheckNav(tx);
        dispatch(&self.sender, rx, cmd)
    }
}

fn dispatch(
    sender: &std::sync::mpsc::SyncSender<JsCmd>,
    rx: oneshot::Receiver<Reply>,
    cmd: JsCmd,
) -> impl std::future::Future<Output = Result<String>> + Send {
    let sender = sender.clone();
    async move {
        // Offload the blocking send so a full channel can't stall an async
        // worker thread, then await the one-shot reply.
        tokio::task::spawn_blocking(move || sender.send(cmd))
            .await
            .expect("js worker thread alive")
            .map_err(|_| VakError::Engine("dom backend: js runtime died".into()))?;
        match rx.await {
            Ok(Ok(s)) => Ok(s),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(VakError::Engine(e.to_string())),
        }
    }
}

fn run_worker(rx: std::sync::mpsc::Receiver<JsCmd>, dom: DomTree) {
    let Ok(ctx) = quick_js::Context::new() else {
        return;
    };
    // `location` is reactive: reads return the current URL; assigning
    // `location.href = '<url>'` records a pending nav target (`__nav__`) the
    // host checks after running scripts. A single getter/setter defined in JS —
    // no Rust callback needed (keeps the `!Send` ctx on this thread). Then a
    // minimal DOM event system: `addEventListener`/`requestSubmit` are wired in
    // JS against a `__listeners` registry so `ctx.call_function` (name-only) is
    // never needed — dispatch runs entirely in JS with a constructed `Event`.
    if ctx.eval(
        "globalThis.__loc__ = '';\n\
         globalThis.__nav__ = undefined;\n\
         globalThis.__nav__ = undefined;\n\
         globalThis.location = {get href() { return globalThis.__loc__; }, set href(v) \
         { globalThis.__nav__ = String(v); }};\n\
         globalThis.location.assign = function(u){ globalThis.__nav__ = String(u); };\n\
         globalThis.location.replace = function(u){ globalThis.__nav__ = String(u); };\n\
         globalThis.__listeners = {};\n\
         globalThis.Event = function(type){ this.type = String(type); this.defaultPrevented = false;\n\
         this.preventDefault = function(){ this.defaultPrevented = true; }; this.target = null; };\n\
         globalThis.__fireSubmit = function(id, type){ var reg = globalThis.__listeners[id];\n\
         if (!reg) { return; } var fn = reg[type]; if (typeof fn !== 'function') { return; }\n\
         var e = new globalThis.Event(type); e.target = id; fn.call(null, e); };",
    ).is_err() {
        return;
    }
    // `document.getElementById` — backed by the shared DOM tree via callbacks.
    // quick-js 0.4 has no object/class builder, so the shim is eval'd JS whose
    // per-element proxies call back into Rust through these `_vkb_*` globals.
    let get_text = dom.clone();
    let _ = ctx.add_callback("_vkb_getText", move |id: String| -> String {
        dom_read(&get_text, &id, crate::text_content)
    });
    let get_value = dom.clone();
    let _ = ctx.add_callback("_vkb_getValue", move |id: String| -> String {
        dom_read(&get_value, &id, |n| {
            n.attr("value")
                .map(str::to_owned)
                .filter(|v| !v.is_empty())
                .or_else(|| {
                    if n.text.is_empty() {
                        None
                    } else {
                        Some(n.text.clone())
                    }
                })
                .unwrap_or_default()
        })
    });
    let set_text = dom.clone();
    let _ = ctx.add_callback("_vkb_setText", move |id: String, v: String| -> String {
        dom_write(&set_text, &id, |n| {
            if let Some(n) = n {
                n.children.clear();
                n.children.push(crate::Node {
                    tag: String::new(),
                    attrs: Vec::new(),
                    text: v.clone(),
                    children: Vec::new(),
                });
            }
        });
        String::new()
    });
    let set_value = dom.clone();
    let _ = ctx.add_callback("_vkb_setValue", move |id: String, v: String| -> String {
        dom_write(&set_value, &id, |n| {
            if let Some(n) = n {
                n.set_attr("value", &v);
            }
        });
        String::new()
    });
    // `_vkb_find(selector)` returns a JSON array of `@eN` refs for the
    // matching interactive elements (same numbering `snapshot` uses), so
    // `document.querySelector` is backed by the shared DOM tree.
    let find_dom = dom.clone();
    let _ = ctx.add_callback("_vkb_find", move |sel: String| -> String {
        match crate::find_by_css(&find_dom.read().unwrap_or_else(|e| e.into_inner()), &sel) {
            Ok(refs) if !refs.is_empty() => {
                serde_json::to_string(&refs.iter().map(|r| &r.0).collect::<Vec<_>>())
                    .unwrap_or_else(|_| "[]".to_string())
            }
            _ => "[]".to_string(),
        }
    });
    if ctx.eval(
        "globalThis.document = {getElementById: function(id){var i=String(id);return {__id:i,\
get textContent(){return _vkb_getText(this.__id);},\
set textContent(v){_vkb_setText(this.__id,String(v));},\
get value(){return _vkb_getValue(this.__id);},\
set value(v){_vkb_setValue(this.__id,String(v));},\
addEventListener:function(t,fn){var id=this.__id;globalThis.__listeners[id]=globalThis.__listeners[id]||{};globalThis.__listeners[id][String(t)]=fn;},\
requestSubmit:function(){globalThis.__fireSubmit(this.__id,'submit');}\
};},\
querySelector:function(sel){try{var a=JSON.parse(_vkb_find(String(sel)));return a.length?a[0]:null;}catch(e){return null;}},\
querySelectorAll:function(sel){try{return JSON.parse(_vkb_find(String(sel)));}catch(e){return[];}}};\
globalThis.window = {location: globalThis.location};",
    ).is_err() {
        return;
    }
    while let Ok(cmd) = rx.recv() {
        handle(cmd, &ctx);
    }
}

fn dom_read<R: Default, F: FnOnce(&Node) -> R>(dom: &DomTree, id: &str, f: F) -> R {
    let guard = dom.read().unwrap_or_else(|e| e.into_inner());
    match crate::find_by_id(&guard, id) {
        Some(n) => f(n),
        None => R::default(),
    }
}

fn dom_write<F: FnOnce(Option<&mut Node>)>(dom: &DomTree, id: &str, f: F) {
    let mut guard = dom.write().unwrap_or_else(|e| e.into_inner());
    if let Some(n) = crate::find_by_id_mut(&mut guard, id) {
        f(Some(n));
    }
}

fn handle(cmd: JsCmd, ctx: &quick_js::Context) {
    let (tx, reply) = match cmd {
        JsCmd::EvalText(expr, tx) => (tx, eval_text_on(ctx, &expr)),
        JsCmd::EvalHref(expr, tx) => (tx, eval_href_on(ctx, &expr)),
        JsCmd::SetLocation(url, tx) => (tx, set_location_on(ctx, &url)),
        JsCmd::CheckNav(tx) => (tx, check_nav_on(ctx)),
    };
    let _ = tx.send(reply);
}

fn eval_text_on(ctx: &quick_js::Context, expr: &str) -> Reply {
    let v = ctx
        .eval(expr)
        .map_err(|e| VakError::Protocol(format!("dom js: {e}")))?;
    ctx.set_global("__r", v)
        .map_err(|e| VakError::Protocol(format!("dom js: {e}")))?;
    let s = ctx
        .eval_as::<String>(
            "typeof __r === 'undefined' ? 'undefined' : __r === null ? 'null' \
             : typeof __r === 'object' ? JSON.stringify(__r) : String(__r)",
        )
        .map_err(|e| VakError::Protocol(format!("dom js: {e}")))?;
    Ok(s)
}

fn eval_href_on(ctx: &quick_js::Context, script: &str) -> Reply {
    let v = ctx
        .eval(script)
        .map_err(|e| VakError::Protocol(format!("dom js: {e}")))?;
    let s = match v.as_str() {
        Some(s) if !s.is_empty() => format!("nav:{s}"),
        _ => "stay".to_string(), // void(0) / non-string result => stay
    };
    Ok(s)
}

fn set_location_on(ctx: &quick_js::Context, url: &str) -> Reply {
    ctx.set_global("__loc__", url.to_string())
        .map_err(|e| VakError::Protocol(format!("dom js: {e}")))?;
    ctx.set_global("__nav__", quick_js::JsValue::Undefined)
        .map_err(|e| VakError::Protocol(format!("dom js: {e}")))?;
    Ok(String::new())
}

fn check_nav_on(ctx: &quick_js::Context) -> Reply {
    let s = ctx
        .eval_as::<String>(
            "typeof globalThis.__nav__ === 'string' && globalThis.__nav__ !== '' \
             ? String(globalThis.__nav__) : ''",
        )
        .map_err(|e| VakError::Protocol(format!("dom js: {e}")))?;
    // Reset so a second check after navigation doesn't re-fire the redirect.
    ctx.set_global("__nav__", quick_js::JsValue::Undefined)
        .map_err(|e| VakError::Protocol(format!("dom js: {e}")))?;
    Ok(s)
}
