<?php
/**
 * @template T
 * @template TArray of array<array-key, T>
 * @param TArray $array
 * @return properties-of<T>
 */
function asArray($array) {
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
$objAsArray = asArray([$obj]);
$b = $objAsArray["d"];
                
