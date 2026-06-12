<?php
class A {
    /** @var non-empty-string */
    public const STR = "a";
}

class B extends A {
    /** @var non-empty-string */
    public const STR = parent::STR . "b";
}

class C extends B {
    /** @var non-empty-string */
    public const STR = parent::STR . "c";
}

/** @param "abc" $foo */
function foo(string $foo): void {}
foo(C::STR);
