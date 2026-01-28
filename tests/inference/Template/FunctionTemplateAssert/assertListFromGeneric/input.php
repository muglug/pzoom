<?php
/**
 * @template T
 */
final class Type
{
    /**
     * @param mixed $toCheck
     * @psalm-assert T $toCheck
     */
    function assert($toCheck): void
    {
    }
}

/**
 * @param list<int> $_list
 */
function acceptsIntList(array $_list): void {}

/** @var Type<list<int>> $numbersT */
$numbersT = new Type();

/** @var mixed $mixed */
$mixed = null;

$numbersT->assert($mixed);
acceptsIntList($mixed);