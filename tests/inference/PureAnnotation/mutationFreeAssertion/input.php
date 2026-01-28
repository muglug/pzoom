<?php
class A {
    private ?A $other = null;

    public function setVar(A $other): void {
        $this->other = $other;
    }

    /**
     * @psalm-mutation-free
     * @psalm-assert !null $this->other
     */
    public function checkNotNullNested(): bool {
        if ($this->other === null) {
            throw new RuntimeException("oops");
        }

        return !!$this->other->other;
    }

    public function foo() : void {}

    public function doSomething(): void {
        $this->checkNotNullNested();
        $this->other->foo();
    }
}
