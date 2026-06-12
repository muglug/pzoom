<?php
class A {
    const C = [
        "a" => 1,
        "b" => 2,
    ];
}

function foo(string $s) : void {
    if (!isset(A::C[$s])) {
        return;
    }

    if ($s === "Hello") {}
}
