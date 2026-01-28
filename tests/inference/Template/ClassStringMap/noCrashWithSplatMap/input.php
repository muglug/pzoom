<?php
class A {}

/** @param array<array-key, mixed> $args */
function takesVariadic(...$args): void {
}

/** @param class-string-map<A, A> $arr */
function foo(array $arr) : void {
    takesVariadic(...$arr);
}