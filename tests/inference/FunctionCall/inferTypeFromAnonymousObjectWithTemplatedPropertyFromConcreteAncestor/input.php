<?php
/** @template T */
abstract class AbstractValue
{
    /** @param T $value */
    public function __construct(public readonly mixed $value) {}
}
/** @extends AbstractValue<int> */
final class IntValue extends AbstractValue {}
final class Nested
{
    public function __construct(public readonly IntValue $value) {}
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
$object = new Nested(new IntValue(42));
$value = getValue($object);
$nestedValue = getNestedValue($object);
