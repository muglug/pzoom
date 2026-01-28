<?php
trait T {
    public $foo = null;
}
class A {
    use T;

    public function __construct() {
        $this->foo = 5;
    }
}
