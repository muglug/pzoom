<?php
class A {}
class B extends A {
    public function foo() : void {}
}
class C extends A {
    public function foo() : void {}
}
function bar(A $a) : void {
    if (false === ($a instanceof B || $a instanceof C)) {
        return;
    }
    $a->foo();
}
function baz(A $a) : void {
    if (($a instanceof B || $a instanceof C) === false) {
        return;
    }
    $a->foo();
}