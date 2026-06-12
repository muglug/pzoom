<?php
class A {
    /** @var list{"a", ...} */
    public const ARR = ["a"];
}

class B extends A {
    /** @var list{"a", "b", ...} */
    public const ARR = [...parent::ARR, "b"];
}

class C extends B {
    public const ARR = [...parent::ARR, "c"];
}

/** @param array{"a", "b", "c"} $arg */
function foo(array $arg): void {}
foo(C::ARR);
