<?php
function bar(string $s): void { }

class A {
    /** @var array<string, string> */
    public static $a = [];
}

function foo(): void {
    $b = "hello";

    if (!isset(A::$a[$b])) {
        $g = "bar";
    } else {
        bar(A::$a[$b]);
        $g = "foo";
    }

    bar($g);
}
