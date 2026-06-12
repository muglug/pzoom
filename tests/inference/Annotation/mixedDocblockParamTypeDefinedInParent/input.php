<?php
class A {
    /** @param mixed $a */
    public function foo($a): void {}
}

class B extends A {
    public function foo($a): void {}
}
