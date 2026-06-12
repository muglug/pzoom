<?php
/** @template T */
abstract class AbstractValue
{
    /** @param T $value */
    public function __construct(public readonly mixed $value) {}
}
/**
 * @template TValue
 * @extends AbstractValue<TValue>
 */
final class ConcreteValue extends AbstractValue
{
    /**
     * @param TValue $value
     */
    public function __construct(mixed $value)
    {
        parent::__construct($value);
    }
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
$object = new ConcreteValue(new ConcreteValue(42));
$value = getValue($object);
$nestedValue = getNestedValue($object);
