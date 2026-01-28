<?php
class HttpError {
    const ERRS = [
        403 => "a",
        404 => "b",
        500 => "c"
    ];
}

function init(string $code) : string {
    if (array_key_exists($code, HttpError::ERRS)) {
        return $code;
    }

    return "";
}