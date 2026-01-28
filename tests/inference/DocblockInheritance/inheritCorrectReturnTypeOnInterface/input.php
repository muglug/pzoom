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

function takesB(B $f) : B {
    return $f->map();
}
