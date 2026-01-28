<?php
namespace Bar;

class A {}
class B extends A {
    public function foo(): void {}
}

class C {
    private function assertInstanceOfB(A $var): void {
        if (!$var instanceof B) {
            throw new \Exception();
        }
    }

    private function takesA(A $a): void {
        $this->assertInstanceOfB($a);
        $a->foo();
    }
}
