<?php
function foo(string $s) : string {
    $parts = parse_url($s);
    return $parts["host"] ?? "";
}

function hereisanotherone(string $s) : string {
    $parsed = parse_url($s);

    if (isset($parsed["host"])) {
        return $parsed["host"];
    }

    return "";
}

function hereisthelastone(string $s) : string {
    $parsed = parse_url($s);

    if (isset($parsed["host"])) {
        return $parsed["host"];
    }

    return "";
}

function portisint(string $s) : int {
    $parsed = parse_url($s);

    if (isset($parsed["port"])) {
        return $parsed["port"];
    }

    return 80;
}

function portismaybeint(string $s) : ? int {
    $parsed = parse_url($s);

    return $parsed["port"] ?? null;
}

$porta = parse_url("", PHP_URL_PORT);
$porte = parse_url("localhost:443", PHP_URL_PORT);
