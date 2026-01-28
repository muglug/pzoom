<?php
class RequestHeaders {}

class Request {
    public RequestHeaders $headers;
    public function __construct(RequestHeaders $headers) {
        $this->headers = $headers;
    }
}

function lag(Request $req) : void  {
    if ($req->headers) {}
}
