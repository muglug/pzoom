<?php
/**
 * @pure
 */
function bar(string $c, string &$str = ""): string
{
    $c .= $str;

    return $c;
}

/**
 * @pure
 */
function baz(): string
{
    $f = "foo";
    bar($f);

    return $f;
}
