# http

The `http` module allows sending HTTP requests and handling responses.

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

A response with an error status (such as `404` or `500`) is still `Ok(Response)`: the request reached the server. Only failures to communicate are errors.

## How requests are made

Requests use HTTP/1.1. A connection may take at most 30 seconds; up to 5 redirects are followed (`301`, `302` and `303` become a `GET` without the body; `307` and `308` are followed for a `GET` only); a body is sent as `application/json` when it starts with `{` or `[` (after white space), otherwise as `text/plain; charset=utf-8`; the body of a response is read as UTF-8 (invalid bytes become U+FFFD), up to 10 MB. Responses may come with a length, in chunks (with extensions and trailers) or until the server closes the connection.

## Errors

```koji
type HttpError = {
    InvalidUrl(url: string)
    ConnectionFailed(url: string, message: string)
    Timeout(url: string)
    Tls(url: string, message: string)
    InvalidResponse(url: string, message: string)
}
```

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
