<?php
namespace Bar;

class A {
    public function bar() : void {}
}
interface I {
    public function foo(): void;
}
class B extends A implements I {
    public function foo(): void {}
}

function assertInstanceOfI(A $var): void {
    if (!$var instanceof I) {
        throw new \Exception();
    }
}

function takesA(A $a): void {
    assertInstanceOfI($a);
    $a->bar();
    $a->foo();
}
