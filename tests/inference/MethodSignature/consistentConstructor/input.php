<?php
/**
 * @psalm-consistent-constructor
 */
class A {
    public function getInstance() : self {
        return new static();
    }
}

class AChild extends A {
    public function __construct() {}
}
