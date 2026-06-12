<?php
class A {
    /** @var int */
    public $a;

    public function __construct() {
        if (rand(0, 1)) {
            $this->a = 5;
        }
    }
}
