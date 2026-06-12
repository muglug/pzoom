<?php
/** @template T */
final class Value
{
    /** @param T $value */
    public function __construct(public readonly mixed $value) {}
}
/**
 * @template T
 * @param object{value: T} $object
 * @return T
 */
function getValue(object $object): mixed
{
    return $object->value;
}
/**
 * @template T
 * @param object{value: object{value: T}} $object
 * @return T
 */
function getNestedValue(object $object): mixed
{
    return $object->value->value;
}
$object = new Value(new Value(42));
$value = getValue($object);
$nestedValue = getNestedValue($object);
