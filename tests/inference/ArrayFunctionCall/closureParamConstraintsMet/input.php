<?php
class A {}
class B {}

$test = [new A(), new B()];

usort(
    $test,
    /**
     * @param A|B $a
     * @param A|B $b
     */
    function($a, $b): int
    {
        return $a === $b ? 1 : -1;
    }
);
