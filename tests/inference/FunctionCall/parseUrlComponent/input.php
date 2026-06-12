<?php
function foo(string $s) : string {
    return parse_url($s, PHP_URL_HOST) ?? "";
}

function bar(string $s) : string {
    return parse_url($s, PHP_URL_HOST);
}

function bag(string $s) : string {
    $host = parse_url($s, PHP_URL_HOST);

    if (is_string($host)) {
        return $host;
    }

    return "";
}
