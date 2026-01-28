<?php
function foo(string $a, string $b) : void {
    $o = getopt($a, [$b]);

    $a = isset($o["a"]) && is_string($o["a"]) ? $o["a"] : "foo";
    $a = isset($o["a"]) && is_string($o["a"]) ? $o["a"] : "foo";
    echo $a;
}