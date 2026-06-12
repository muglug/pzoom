<?php
/**
 * @pure
 */
function bar(string $st, string &$str = ""): string
{
    $st .= $str;

    return $st;
}

/**
 * @pure
 */
function baz(): string
{
    $f = "foo";
    bar(st: $f);

    return $f;
}
