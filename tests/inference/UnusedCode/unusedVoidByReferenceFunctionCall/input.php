<?php
function bar(string &$str): void
{
    $str .= "foo";
}

function baz(): string
{
    $f = "foo";
    bar($f);

    return $f;
}
