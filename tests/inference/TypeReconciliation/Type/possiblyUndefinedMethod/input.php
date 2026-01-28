<?php
class A {
    public function foo(): void {}
}
class B {
    public function other(): void {}
}

function a(bool $cond): void {
    if ($cond) {
        $a = new A();
    } else {
        $a = new B();
    }

    if ($cond) {
        $a->foo();
    }
}
