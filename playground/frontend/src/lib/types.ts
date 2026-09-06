// Wire types mirroring vakbrowse-server's Request/Action/Response model.
// The playground speaks the exact same protocol the daemon/CLI/MCP/API use.

export type SessionId = string;
export type ElementRef = string; // e.g. "@e42"

export type SnapshotNode = {
  ref: ElementRef;
  role: string;
  name: string;
  value: string | null;
  description: string | null;
};

export type Snapshot = {
  url: string;
  title: string;
  elements: SnapshotNode[];
};

export type TabInfo = {
  id: string;
  url: string;
};

export type Cookie = {
  name: string;
  value: string;
  domain: string;
  path: string;
  secure: boolean;
  http_only: boolean;
  same_site: string | null;
};

export type CookieInput = {
  name: string;
  value: string;
  domain: string;
  path: string;
  secure: boolean;
  http_only: boolean;
  same_site?: 'Strict' | 'Lax' | 'None';
};

export type WebMcpTool = {
  name: string;
  description: string;
};

export type ActionResult =
  | { type: 'navigated'; url: string; title: string }
  | { type: 'snapshot'; snapshot: Snapshot }
  | { type: 'text'; text: string }
  | { type: 'flag'; ok: boolean }
  | { type: 'cookies'; cookies: Cookie[] }
  | { type: 'done' }
  | { type: 'clicked'; navigated: boolean; url: string | null }
  | { type: 'elements'; refs: ElementRef[] }
  | { type: 'image'; png_base64: string }
  | { type: 'tools'; tools: WebMcpTool[] }
  | { type: 'tabs'; tabs: TabInfo[] }
  | { type: 'tab_opened'; tab: TabInfo };

export type ServiceError = {
  kind: string;
  message: string;
};

export type ResponsePayload =
  | { Opened: { id: SessionId; url: string; profile: string | null } }
  | { Closed: boolean }
  | { Sessions: Array<{ id: SessionId; url: string; profile: string | null }> }
  | { Result: ActionResult }
  | { Results: ActionResult[] }
  | { Error: Record<string, string> };

export type Response =
  | { Ok: ResponsePayload }
  | { Err: string };

export type SessionOptions = {
  url?: string;
  headless?: boolean;
  profile?: string;
  stealth?: boolean;
  stealth_seed?: string;
  proxy?: string;
  proxies?: string[];
  human_timing?: boolean;
  backend?: 'cdp';
  headed?: boolean;
};

export type Action =
  | { type: 'navigate'; url: string }
  | { type: 'snapshot' }
  | { type: 'click'; ref: ElementRef }
  | { type: 'fill'; ref: ElementRef; text: string }
  | { type: 'select_option'; ref: ElementRef; value: string }
  | { type: 'set_file_chooser'; ref: ElementRef; paths: string[] }
  | { type: 'press_key'; key: string }
  | { type: 'scroll'; dx: number; dy: number }
  | { type: 'eval_text'; expression: string }
  | { type: 'find_by_css'; selector: string }
  | { type: 'wait_for_truthy'; expression: string; timeout_ms: number }
  | { type: 'wait_for_url'; pattern: string; timeout_ms: number }
  | { type: 'cookies' }
  | { type: 'set_cookie'; cookie: CookieInput }
  | { type: 'clear_cookies' }
  | { type: 'set_download_dir'; dir: string }
  | { type: 'screenshot'; full_page: boolean }
  | { type: 'click_at'; x: number; y: number }
  | { type: 'rotate_proxy' }
  | { type: 'source' }
  | { type: 'downloads' }
  | { type: 'webmcp_tools' }
  | { type: 'webmcp_invoke'; name: string; arguments_json: string }
  | { type: 'back' }
  | { type: 'forward' }
  | { type: 'reload' }
  | { type: 'extract' }
  | { type: 'tabs' }
  | { type: 'new_tab'; url?: string }
  | { type: 'switch_tab'; tab: string }
  | { type: 'close_tab'; tab: string };
