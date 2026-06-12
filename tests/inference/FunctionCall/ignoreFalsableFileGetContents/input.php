<?php
function foo(string $s): string {
    return file_get_contents($s);
}
function bar(string $s): string {
    $a = file_get_contents($s);
    if ($a === false) {
        return "hello";
    }
    return $a;
}
/**
 * @return false|string
 */
function bat(string $s) {
    return file_get_contents($s);
}
