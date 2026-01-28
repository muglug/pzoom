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
$d = $objAsArray["d"];
                
