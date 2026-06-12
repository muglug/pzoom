<?php
class A {
    /** @var array{a: true, ...} */
    public const ARR = ["a" => true];
}

class B extends A {
    /** @var array{a: true, b: true, ...} */
    public const ARR = parent::ARR + ["b" => true];
}

class C extends B {
    public const ARR = parent::ARR + ["c" => true];
}

/** @param array{a: true, b: true, c: true} $arg */
function foo(array $arg): void {}
foo(C::ARR);
