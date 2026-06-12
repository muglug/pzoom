<?php
class A {
    public const ARR = [];
}

class B extends A {
    public const ARR = [...parent::ARR];
}

class C extends B {
    public const ARR = [...parent::ARR];
}

/** @param array<never, never> $arg */
function foo(array $arg): void {}
foo(C::ARR);
