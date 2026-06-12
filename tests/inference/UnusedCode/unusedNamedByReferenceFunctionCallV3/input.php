<?php
function bar(string &$st, ?string &$str = ""): string
{
    $st .= (string) $str;

    return $st;
}

function baz(): string
{
    $f = "foo";
    bar(st: $f, str: $c);

    return $f;
}
