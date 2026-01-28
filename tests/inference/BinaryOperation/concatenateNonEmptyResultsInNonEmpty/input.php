<?php
/** @param non-empty-lowercase-string $arg */
function foobar($arg): string
{
    return $arg;
}

$foo = rand(0, 1) ? "a" : "b";
$bar = rand(0, 1) ? "c" : "d";
$baz = $foo . $bar;
foobar($baz);
