<?php
abstract class A {
    /** @var int */
    public $a;

    public function __construct() {
        if ($this instanceof B) {
            $this->a = $this->bar();
        } else {
            $this->a = 6;
        }
    }
}

class B extends A {
    public function bar() : int {
        return 3;
    }
}
