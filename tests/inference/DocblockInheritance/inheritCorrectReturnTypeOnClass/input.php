<?php
interface A {
    /**
     * @return A
     */
    public function map(): A;
}

interface B extends A {
    /**
     * @return B
     */
    public function map(): A;
}

class F implements B {
    public function map(): A {
        return new F();
    }
}

function takesF(F $f) : B {
    return $f->map();
}
