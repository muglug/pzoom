<?php
class A {}
class B {}

/** @param class-string<A> $s */
function takesAString(string $a): void {}
/** @param class-string<B> $s */
function takesBString(string $a): void {}

/** @param class-string $s */
function foo(string $s): void {
    if (is_subclass_of($s, A::class)) {
        takesAString($s);
    }
    if (is_subclass_of($s, B::class)) {
        takesBString($s);
    }
}
