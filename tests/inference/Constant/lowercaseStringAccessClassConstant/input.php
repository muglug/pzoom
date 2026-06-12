<?php
class A {
    const C = [
        "a" => 1,
        "b" => 2,
        "c" => 3
    ];
}

/**
 * @param lowercase-string $s
 */
function foo(string $s, string $t) : void {
    echo A::C[$t];
    echo A::C[$s];
}
