<?php
class A {
    /**
     * @return string
     */
    public function A() {
        return "hello";
    }
}

class B extends A {
    public function __construct() {
        parent::__construct();
    }
}
