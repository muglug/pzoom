<?php
function bar(string &$str): string
{
    $str .= "foo";

    return $str;
}

function baz(): string
{
    $f = "foo";
    bar($f);

    return $f;
}
