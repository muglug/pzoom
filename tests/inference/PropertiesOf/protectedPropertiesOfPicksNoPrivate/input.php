<?php
class A {
    /** @var mixed */
    public $foo;
    /** @var mixed */
    private $bar;
    /** @var mixed */
    protected $adams;
}

/** @return protected-properties-of<A> */
function returnPropertyOfA() {
    return ["bar" => true];
}
                
