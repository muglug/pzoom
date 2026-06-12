<?php
class A {
    public function __construct() {
        if ($this instanceof B) {
            $this->b = "foo";
        }
    }
}

class B extends A {
    /** @var string */
    private $b;
}
