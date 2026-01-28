<?php

class Foo {
    public const VARS = [
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "y",
    ];
}

/** @param list<string> $vars */
function foo(array $vars): void {
    print_r($vars);
}

foo(Foo::VARS);
                    
