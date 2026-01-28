<?php
namespace Bar;

class A {
    public function foo(): void {}
}

class B {
    /** @var A|null */
    public $a;

    private function assertNotNullProperty(): void {
        if (!$this->a) {
            throw new \Exception();
        }
    }

    public function takesA(A $a): void {
        $this->assertNotNullProperty();
        $a->foo();
    }
}
