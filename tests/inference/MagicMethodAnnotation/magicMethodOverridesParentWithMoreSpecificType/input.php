<?php
class C {}
class D extends C {}

class A {
    public function foo(string $s) : C {
        return new C;
    }
}

/** @method D foo(string $s) */
class B extends A {}
