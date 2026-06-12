<?php
function foo(string $s) : void {
    if (trait_exists($s) || enum_exists($s)) {
        new ReflectionClass($s);
    }
}

function bar(string $s) : void {
    if (enum_exists($s) || trait_exists($s)) {
        new ReflectionClass($s);
    }
}

function baz(string $s) : void {
    if (enum_exists($s) || interface_exists($s) || trait_exists($s)) {
        new ReflectionClass($s);
    }
}
