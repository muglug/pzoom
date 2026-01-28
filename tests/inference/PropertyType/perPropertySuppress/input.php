<?php
class A {
    /**
     * @var int
     * @psalm-suppress PropertyNotSetInConstructor
     */
    public $a;

    public function __construct() { }
}
