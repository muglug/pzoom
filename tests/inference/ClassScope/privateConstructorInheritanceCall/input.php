<?php
class A {
    private function __construct() { }
}
class B extends A {
    public function __construct() {
        parent::__construct();
    }
}
