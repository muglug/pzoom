<?php
abstract class A {
    /** @return static */
    public function foo() {
        return $this;
    }
}

interface I {
    /** @return I */
    public function foo();
}

class C extends A implements I {}
