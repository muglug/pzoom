<?php
function foo(string $s): string {
    $s = preg_replace("/hello/", "", $s);
    if ($s === null) {
        return "hello";
    }
    return $s;
}
function bar(string $s): string {
    $s = preg_replace("/hello/", "", $s);
    return $s;
}
function bat(string $s): ?string {
    $s = preg_replace("/hello/", "", $s);
    return $s;
}
