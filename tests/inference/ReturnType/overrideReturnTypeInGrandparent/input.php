<?php
abstract class A {
    /** @return string|null */
    abstract public function blah();
}

abstract class B extends A {
}

class C extends B {
    /** @return string|null */
    public function blah() {
        return rand(0, 10) === 4 ? "blahblah" : null;
    }
}

$blah = (new C())->blah();
