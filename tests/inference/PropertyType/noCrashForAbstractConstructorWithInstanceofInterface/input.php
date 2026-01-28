<?php
abstract class A {
    /** @var int */
    public $a;

    public function __construct() {
        if ($this instanceof I) {
            $this->a = $this->bar();
        } else {
            $this->a = 6;
        }
    }
}

interface I {
    public function bar() : int;
}
