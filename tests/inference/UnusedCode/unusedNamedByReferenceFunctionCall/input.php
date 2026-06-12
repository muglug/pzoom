<?php
function bar(string $c = "", string &$str = ""): string
{
    $c .= $str;
    $str .= $c;

    return $c;
}

function baz(): string
{
    $f = "foo";
    bar(str: $f);

    return $f;
}
