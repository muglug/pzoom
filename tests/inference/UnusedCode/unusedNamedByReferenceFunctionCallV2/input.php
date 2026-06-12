<?php
function bar(string &$st, string &$str = ""): string
{
    $st .= $str;

    return $st;
}

function baz(): string
{
    $f = "foo";
    bar(st: $f);

    return $f;
}
