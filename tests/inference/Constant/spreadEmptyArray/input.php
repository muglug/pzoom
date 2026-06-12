<?php
class A {
    public const ARR = [];
}

/** @param array<never, never> $arg */
function foo(array $arg): void {}
foo([...A::ARR]);
