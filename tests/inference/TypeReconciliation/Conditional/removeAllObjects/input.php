<?php
class A {}
class B extends A {
    public function foo() : void {}
}
class BChild extends B {}
class C extends A {}
class D extends A {}

/** @param B|C|D $a */
function foo(A $a) : B {
    if ($a instanceof C) {
        $a = new B();
    } elseif ($a instanceof D) {
        $a = new B();
    } elseif (!$a instanceof BChild) {
        // do something
    }

    return $a;
}