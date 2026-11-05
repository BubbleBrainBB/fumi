// Globals JavaScriptCore lacks: Node's `process`, and WHATWG `URL` (enough
// of it for the renderer: protocol, credentials, host and path of a URL).
var process = globalThis.process = { argv: [], env: {}, exitCode: 0, emitWarning() {} };
if (typeof globalThis.URL === 'undefined') {
  globalThis.URL = class URL {
    constructor(input) {
      const match = /^([a-z][a-z0-9+.-]*:)(?:\/\/(?:([^:@/?#]*)(?::([^@/?#]*))?@)?([^/?#:]*)(?::(\d+))?)?([^?#]*)(\?[^#]*)?(#.*)?$/i.exec(String(input));
      if (!match) throw new TypeError(`Invalid URL: ${input}`);
      this.href = String(input);
      this.protocol = match[1].toLowerCase();
      this.username = match[2] || '';
      this.password = match[3] || '';
      this.hostname = (match[4] || '').toLowerCase();
      this.port = match[5] || '';
      this.pathname = match[6] || (match[4] ? '/' : '');
      this.search = match[7] || '';
      this.hash = match[8] || '';
    }
    toString() { return this.href; }
  };
}
