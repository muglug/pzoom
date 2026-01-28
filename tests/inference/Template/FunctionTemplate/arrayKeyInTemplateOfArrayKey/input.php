<?php

/**
 * @template TKey of array-key
 * @template TValue
 * @template TNewKey of array-key
 * @template TNewValue
 * @psalm-param iterable<TKey, TValue> $iterable
 * @psalm-param callable(TKey): iterable<TNewKey, TNewValue> $mapper
 * @psalm-return \Generator<TNewKey, TNewValue>
 */
function map(iterable $iterable, callable $mapper): Generator
{
    foreach ($iterable as $key => $_) {
        yield from $mapper($key);
    }
}

/**
 * @psalm-return iterable<array-key, \stdClass>
 */
function iter(): iterable
{
    return [];
}

/**
 * @template TKey of array-key
 * @psalm-param TKey $key
 * @psalm-return Generator<TKey, string>
 */
function mapper($key): Generator
{
    yield $key => "a";
}

map(iter(), "mapper");
