<?php
/**
 * @psalm-consistent-constructor
 */
class A {
    public function getInstance() : self
    {
        return new static();
    }

    public function __construct() {}
}

class BadAChild extends A {
    public function __construct(string $s) {}
}
