<?php
/**
 * @template T
 */
final class Type
{
    /**
     * @param mixed $toCheck
     * @psalm-assert-if-true T $toCheck
     */
    function is($toCheck): bool
    {
        throw new RuntimeException("???");
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

if ($numbersT->is($mixed)) {
    acceptsIntList($mixed);
}