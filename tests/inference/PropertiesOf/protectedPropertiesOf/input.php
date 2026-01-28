<?php
class A {
    /** @var bool */
    public $foo = false;
    /** @var string */
    private $bar = "";
    /** @var int */
    protected $adams = 42;
}

/** @return protected-properties-of<A> */
function returnPropertyOfA() {
    return ["adams" => 42];
}
                
