<?php
namespace Bar;

class A {}
class B extends A {
    public function foo(): void {}
}

function assertInstanceOfB(A $var): void {
    if (!$var instanceof B) {
        throw new \Exception();
    }
}

function takesA(A $a): void {
    assertInstanceOfB($a);
    $a->foo();
}
