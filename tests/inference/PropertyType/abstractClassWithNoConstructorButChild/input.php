<?php
abstract class A {
    /** @var string */
    public $foo;
}

class B extends A {
    public function __construct() {}
}
