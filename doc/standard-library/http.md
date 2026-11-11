# http

The `http` module sends HTTP requests, and serves them.

## Functions

-   `get(url: string) -> Result[Response, HttpError]`  
    Sends a GET request to the specified URL.

-   `post(url: string, data: string) -> Result[Response, HttpError]`  
    Sends a POST request with `data` to the specified URL.

-   `put(url: string, data: string) -> Result[Response, HttpError]`  
    Sends a PUT request with `data` to the specified URL.

-   `delete(url: string) -> Result[Response, HttpError]`  
    Sends a DELETE request to the specified URL.

## The `Response` Type

The response to a request has these fields:

-   `status: int`  
    The HTTP status code of the response.

-   `body: string`  
    The body content of the response.

-   `headers: Map[string, string]`  
    Its headers, by name in lower case (`response.headers.get('content-type')`). The values of a header sent several times are joined with `, `.

A response with an error status (such as `404` or `500`) is still `Ok(Response)`: the request reached the server. Only failures to communicate are errors.

## How requests are made

Requests use HTTP/1.1. A connection may take at most 30 seconds; up to 5 redirects are followed (`301`, `302` and `303` become a `GET` without the body; `307` and `308` are followed for a `GET` only); a body is sent as `application/json` when it starts with `{` or `[` (after white space), otherwise as `text/plain; charset=utf-8`; the body of a response is read as UTF-8 (invalid bytes become U+FFFD), up to 10 MB. Responses may come with a length, in chunks (with extensions and trailers) or until the server closes the connection.

While a request waits for the network (to connect, to send, to receive), the other [tasks](task.md) of the program run: requests in several tasks are sent at the same time.

## `https://`

Secure requests use TLS 1.3 (only: a server that has only TLS 1.2 is refused, with an error that says so), written in Koji. The certificate of the server is verified, with its name (the DNS names and IP addresses of the certificate; a wildcard stands for one label), against the certificate authorities of the system: the file named by the environment variable `SSL_CERT_FILE`, or the first of the usual files (`/etc/ssl/certs/ca-certificates.crt` on Debian and Ubuntu, `/etc/pki/tls/certs/ca-bundle.crt` on Fedora, `/etc/ssl/cert.pem` on macOS and Alpine...), then the files of the directories in `SSL_CERT_DIR` (separated by `:`). When the system has none, the authorities of Mozilla's list, built in, are used. The ciphers are AES-GCM, when the processor has instructions for it, and ChaCha20-Poly1305.

A response whose body ends with the connection must end with the TLS closing message (close_notify): otherwise it may have been cut by an attacker, and the request fails with `Tls`. A body with a length or in chunks is complete once all of it has come.

## Errors

```koji
type HttpError = {
    InvalidUrl(url: string)
    ConnectionFailed(url: string, message: string)
    Timeout(url: string)
    Tls(url: string, message: string)
    InvalidResponse(url: string, message: string)
    CannotListen(port: int, message: string)
}
```

`CannotListen` is the error of `serve`.

`Tls` is a failure of the secure connection: an invalid certificate (`invalid certificate: the certificate is not valid for this name`, `...has expired or is not yet valid`, `...is not signed by a known authority`...), a server without TLS 1.3, or an alert of the server.

## Example

```koji
import http

match (http.get('https://example.com/api')) {
    Ok(res) => if (res.status == 200) {
        print(res.body)
    }
    Err(ConnectionFailed(url, message)) => print(`cannot reach ${url}: ${message}`)
    Err(error) => print(error)
}
```

## Serving

-   `serve(port: int, handler: (Request) -> Response) -> Result[void, HttpError]`  
    Serves HTTP/1.1 on `port`, on every IPv4 address of the machine: `handler` answers each request. It runs until the program ends, and returns only when it cannot listen on the port (`CannotListen`, when another program uses it).

-   `text(status: int, body: string) -> Response`, `html(status: int, body: string) -> Response`, `json(status: int, body: string) -> Response`  
    A response with a body of plain text, HTML or JSON (its `content-type`, in UTF-8).

```koji
import http

immut failed = http.serve(8080, (request) => {
    if (request.path == '/hello') {
        immut name = match (request.query.get('name')) {
            Some(given) => given
            None => 'world'
        }

        return http.text(200, `Hello, ${name}!`)
    }

    return http.text(404, 'not found')
})

print(failed) // only when port 8080 is taken
```

A request has these fields:

-   `method: string`: `GET`, `POST`...
-   `path: string`: the path, without the query, decoded (`/users/42`).
-   `query: Map[string, string]`: the parameters of the query (`?page=2&sort=name`), decoded (`+` is a space).
-   `headers: Map[string, string]`: the headers, by name in lower case.
-   `body: string`: the body, read as UTF-8 (invalid bytes become U+FFFD).

A handler can answer with headers of its own: `immut response: http.Response = { status: 302, body: '', headers: ['location': '/new'] }`. The server writes `content-length` itself; a `content-type` the response does not give is `text/plain; charset=utf-8`; a header `connection: close` closes the connection after the response.

Each connection is served in a [task](task.md) of its own, so while a handler waits (for a request of its own to another server, a timer...), the other connections are served; a handler that computes for long without waiting holds the others up. A panic in a handler ends the program, as anywhere else.

The server keeps connections open for further requests (HTTP/1.1 keep-alive, and requests sent before the previous one is answered), and closes those that the client asks to close or leaves idle for 60 seconds. A client has 30 seconds to send a request (and to receive its response). It answers by itself, before closing the connection:

-   `400` to what is not a request of HTTP/1.x, to a request with both a length and chunks (how requests are smuggled past proxies) and to malformed headers;
-   `413` to a body over 10 MB, `431` to headers over 64 KB;
-   `501` to a body coded other than in chunks, `505` to another version of HTTP.

Bodies come with a length or in chunks, and a client that waits for leave to send its body (`Expect: 100-continue`) is given it. The response to a `HEAD` is sent without its body.

