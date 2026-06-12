<?php
class A {
    /** @param int $a */
    public function foo($a): void {}
}

class B extends A {
    public function foo($a): void {}
}
