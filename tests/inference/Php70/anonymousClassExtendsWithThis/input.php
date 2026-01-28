<?php
class A {
    public function foo() : void {}
}
$class = new class extends A {
    public function f(): int {
        $this->foo();
        return 42;
    }
};
