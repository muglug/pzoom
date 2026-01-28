<?php
class A {
    /** @var mixed */
    public $foo;
    /** @var mixed */
    private $bar;
    /** @var mixed */
    protected $adams;
}

/** @return private-properties-of<A> */
function returnPropertyOfA() {
    return ["foo" => true];
}
                
