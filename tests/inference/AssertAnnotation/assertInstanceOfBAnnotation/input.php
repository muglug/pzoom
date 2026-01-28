<?php
namespace Bar;

class A {}
class B extends A {
    public function foo(): void {}
}

/** @psalm-assert B $var */
function myAssertInstanceOfB(A $var): void {
    if (!$var instanceof B) {
        throw new \Exception();
    }
}

function takesA(A $a): void {
    myAssertInstanceOfB($a);
    $a->foo();
}
