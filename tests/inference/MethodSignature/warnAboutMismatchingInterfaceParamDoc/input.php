<?php
class A {}
class B {}

interface X {
    /**
     * @param B $class
     */
    public function boo(A $class): void {}
}
