<?php
class A {
    /** @return string|null */
    public function blah() {
        return rand(0, 10) === 4 ? "blah" : null;
    }
}

class B extends A {
    /** @return string */
    public function blah() {
        return "blah";
    }
}

$blah = (new B())->blah();
