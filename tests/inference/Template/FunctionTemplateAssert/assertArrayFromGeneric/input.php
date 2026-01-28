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
 * @param array<string, int> $_list
 */
function acceptsArray(array $_list): void {}

/** @var Type<array<string, int>> $numbersT */
$numbersT = new Type();

/** @var mixed $mixed */
$mixed = null;

$numbersT->assert($mixed);
acceptsArray($mixed);