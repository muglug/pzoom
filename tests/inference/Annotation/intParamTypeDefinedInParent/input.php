<?php
class A {
    public function foo(int $a): void {}
}

class B extends A {
    public function foo($a): void {}
}
