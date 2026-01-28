<?php
/**
 * @template T
 * @param T $obj
 * @return protected-properties-of<T>
 */
function asArray($obj) {
    /** @var protected-properties-of<T> */
    $properties = [];
    return $properties;
}

final class A {
    /** @var int */
    public $a = 42;
    /** @var bool */
    private $b = true;
    /** @var string */
    protected $c = "c";
}

$obj = new A();
$objAsArray = asArray($obj);
$b = $objAsArray["a"];
                
