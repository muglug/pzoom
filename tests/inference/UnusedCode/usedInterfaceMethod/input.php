<?php
interface I {
    public function foo(): void;
}

final class A implements I {
    public function foo(): void {}
}

(new A)->foo();
