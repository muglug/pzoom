<?php
class A {}
class B {}

const C = [
    A::class => 1,
    B::class => 2,
];

/**
 * @param class-string $s
 */
function foo(string $s) : void {
    if (isset(C[$s])) {}
}
