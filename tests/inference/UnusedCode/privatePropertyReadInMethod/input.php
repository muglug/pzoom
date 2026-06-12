<?php
final class A {
    private string $a;

    public function __construct() {
        $this->a = "hello";
    }

    public function emitA(): void {
        echo $this->a;
    }
}

(new A())->emitA();
