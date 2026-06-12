<?php
interface A {
    /** @return string|null */
    public function blah();
}

class B implements A {
    /** @return string|null */
    public function blah() {
        return rand(0, 10) === 4 ? "blah" : null;
    }
}

$blah = (new B())->blah();
