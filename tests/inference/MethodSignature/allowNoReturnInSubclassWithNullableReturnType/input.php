<?php
class A {
    /** @return ?int */
    public function foo() {
        if (rand(0, 1)) return 5;
    }
}

class B extends A {
    public function foo() {}
}
