<?php
function bar(string $s): void { }

class A {
    /** @var array<string, string> */
    public static $a = [];
}

function foo(): void {
    $b = "hello";

    if (!isset(A::$a[$b])) {
        return;
    }

    bar(A::$a[$b]);
}
