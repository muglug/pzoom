<?php
interface A {
    public function foo() : void;
}
interface B {
    public function bar() : void;
}

/** @param A & B $a */
function f(A $a) : void {
    $a->foo();
    $a->bar();
}
