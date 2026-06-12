<?php
class A {
    /** @var string */
    private $b;

    public function __construct() {
        $this->b = "foo";
    }
}

class B extends A {
    /** @var string */
    private $b;
}
