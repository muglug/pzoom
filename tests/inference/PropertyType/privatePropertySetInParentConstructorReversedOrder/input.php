<?php
class B extends A {
    /** @var string */
    private $b;
}

class A {
    public function __construct() {
        if ($this instanceof B) {
            $this->b = "foo";
        }
    }
}
