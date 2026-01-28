<?php
class RequestHeaders {
    public function has(string $s) : bool {
        return true;
    }
}

class Request {
    public RequestHeaders $headers;
    public function __construct(RequestHeaders $headers) {
        $this->headers = $headers;
    }
}

function lag(Request $req) : void  {
    if ($req->headers && $req->headers->has("foo")) {}
}
