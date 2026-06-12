<?php
class A {
    /** @var Closure(int):int */
    private Closure $a;

    public function __construct() {
        $this->a = fn(int $a) : int => $a + 5;
    }

    public function invoker(int $b) : int {
        return $this->a->__invoke($b);
    }
}
