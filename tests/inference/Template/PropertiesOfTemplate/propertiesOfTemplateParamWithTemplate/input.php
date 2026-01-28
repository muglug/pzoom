<?php
/**
 * @template T
 * @param T $obj
 * @return properties-of<T>
 */
function asArray($obj) {
    /** @var properties-of<T> */
    $properties = [];
    return $properties;
}

/** @template T */
class A {
    /** @var bool */
    private $b = true;
    /** @var string */
    protected $c = "c";

    /** @param T $a */
    public function __construct(public $a) {}
}

$obj = new A(42);
$objAsArray = asArray($obj);
                
