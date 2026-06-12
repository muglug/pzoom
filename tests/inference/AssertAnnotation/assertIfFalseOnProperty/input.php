<?php
class A {
    public function foo() : void {}
}

class B {
    private ?A $a = null;

    public function bar() : void {
        if ($this->assertProperty()) {
            $this->a->foo();
        }
    }

    /**
     * @psalm-assert-if-false null $this->a
     */
    public function assertProperty() : bool {
        return $this->a !== null;
    }
}
